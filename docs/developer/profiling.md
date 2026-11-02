# Profiling

Use external tools to explain performance problems. Applications need no embedded
recorder or capture service. Keep profiled runs separate from the unprofiled
[regression measurements](../../tools/benchmarks/README.md).

## Matching Symbols

```bash
make profile-build PROFILE_APP=wfgui PROFILE_FLAVOR=prod \
  PROFILE_OUTPUT=research/profiles/gui-symbols
make profile-build PROFILE_APP=poe2companion PROFILE_FLAVOR=dev \
  PROFILE_OUTPUT=research/profiles/poe2-symbols
```

The target builds only the selected executable, with Rust and native C++ debug
information. It preserves the chosen flavor's optimization and assertion settings,
including development companion reload. Native rendering stays Release. Nothing
is staged, launched or reloaded; normal build output is untouched.

Each new output directory contains the executable, its `.debug` file, an ELF
build-ID index and a manifest with binary hashes and source/build provenance.
Build output goes to `build.log`; `native-compile-commands.json` records C/C++ flags.
Run that exact executable when profiling. Symbols from another rebuild are not
interchangeable, even at the same Git revision. Keep the matching checkout,
generated native sources in the build directory and runtime libraries available;
this is not a portable release package.

Existing unstripped ELF executables, including benchmark binaries, can also be
split with `python3 tools/profiling/symbols.py BINARY --output NEW_DIRECTORY`.
Keep the `.debug` file beside the executable or use the bundle as the profiler's
symbol directory. Splitting does not modify the input or executable code sections.
The helper needs Python 3.11+, `objcopy` and `readelf` (LLVM or GNU). `OBJCOPY` and
`READELF` can select explicit executables.

## Choosing A Tool

| Question | Tool | Limits |
| --- | --- | --- |
| Which Rust/C++ functions consume CPU? | Samply; perf with Hotspot | Sampling, not exact per-frame timings |
| Is a thread stalled or preempted? | perf scheduling/off-CPU analysis | Needs suitable kernel permissions |
| Which allocations retain memory? | Heaptrack; Valgrind Massif | Allocator hooks and custom arenas affect attribution |
| Which paths execute excessive work? | Valgrind Callgrind | Large slowdown; use offline fixtures |
| Which GPU commands are expensive? | RenderDoc | Not applicable to CPU Blend2D drawing |

For Linux user-space sampling:

```bash
perf record --no-buildid-cache -e cycles:u --user-callchains \
  -F 199 --call-graph dwarf,16384 -o research/profiles/gui.data \
  -- research/profiles/gui-symbols/wfgui
perf report -i research/profiles/gui.data

# Optional flame-graph UI:
samply import --save-only --output research/profiles/gui.json.gz \
  --symbol-dir research/profiles/gui-symbols research/profiles/gui.data
samply load research/profiles/gui.json.gz \
  --symbol-dir research/profiles/gui-symbols
```

Close the profiled GUI normally. To inspect a running companion, use `perf record`
with `-p PID` instead of launching another process, keeping the user-only event and
callchain options. Target the observer PID, not the Steam/Proton launcher. Preserve
its exact executable/symbols before rebuilding or reloading; attach permissions
may differ from profiling a child process.

DWARF unwinding uses debug artifacts without forcing frame pointers or disabling
optimization. The stack-copy size and sampling rate are starting points, not a
universal policy; check truncation, lost samples and profiling overhead.

Samply can also record directly, but its Linux recorder may reject
`perf_event_paranoid=2` even when user-only `perf` sampling works. The `:u` event
modifier alone does not exclude kernel callchains; retain `--user-callchains`.
Do not silently change sysctls or run the game as root. Callgrind provides an
offline option without perf-event access:

```bash
valgrind --tool=callgrind --smc-check=all-non-file \
  --callgrind-out-file=research/profiles/callgrind.out \
  PATH_TO_FIXTURE_BINARY --exact TEST_NAME --ignored --test-threads=1
```

Never compare Valgrind wall times to normal rendering latency. Keep profiles local
until reviewed: paths, arguments and allocation payloads may contain private data.

## Known Gaps

Debug symbols cover compiled Rust/C++, not code generated at runtime by Blend2D's
JIT. Kernel samples also do not identify our view IDs, asset jobs or exact frame
boundaries. Retain benchmark counters for these questions. If real captures show
missing JIT attribution or another essential gap, prefer a standard jitdump/perf-map
adapter or an external analysis extension over an application-side recorder.
No such adapter is currently supplied.

## References

- [Separate debug files and build IDs](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Separate-Debug-Files.html)
- [Samply](https://github.com/mstange/samply)
- [perf record](https://man7.org/linux/man-pages/man1/perf-record.1.html)
- [Hotspot](https://github.com/KDAB/hotspot)
- [Heaptrack](https://github.com/KDE/heaptrack)
- [Callgrind](https://valgrind.org/docs/manual/cl-manual.html)
- [RenderDoc](https://github.com/baldurk/renderdoc)
