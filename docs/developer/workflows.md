# Development Workflows

## Build

Required toolchains:

| Area | Requirements |
| --- | --- |
| Common | Git, GNU Make, CMake, and a C/C++ compiler |
| Erlang | Erlang/OTP 29 and Rebar3 |
| Companion | Rust, Cargo, Python 3, `jq`, MinGW-w64, pkg-config, and libxkbcommon development files |
| Desktop GUI | Rust, Cargo, pkg-config, and libxkbcommon development files |
| Qt reference only | LLVM with libc++, Ninja, vcpkg, Autoconf, Autoconf Archive, Automake, and Libtool |

Warframe's runtime integration requires KDE Plasma on Wayland, KScreen, and
Tesseract 5 with English language data. `sccache` is optional. Animated previews
need FFmpeg with VP9 support; release archives need `zip`.

Initialize submodules with `git submodule update --init`, then use these targets:

```bash
make dev             # debug Erlang, companion, and GUI
make prod            # optimized Erlang, companion, and GUI
make dev-erlang
make prod-erlang
make dev-companion
make prod-companion
make inspect         # inspector only, no overlay renderer
make gui             # Rust desktop client (gui-dev)
make gui-prod
make test-gui
make package
```

`make build` builds both development and production trees. Executables are staged
under `dev/bin/` and `prod/bin/`; both include `libexec/` helpers and runtime libraries.
Use these directories directly or the repository-root links:

| Development | Production |
| --- | --- |
| `retinued` | `retinue` |
| `wfclid`, `poe2clid`, `arcclid` | `wfcli`, `poe2cli`, `arccli` |
| `retinue-daemond` | `retinue-daemon` |
| `wfcompaniond`, `poe2companiond`, `arccompaniond` | `wfcompanion`, `poe2companion`, `arccompanion` |
| `retinue-inspectd`, `wfinspectd` | `retinue-inspect`, `wfinspect` |
| `wf-luau-decompilerd` | `wf-luau-decompiler` |
| `wfguid` | `wfgui` |

Run `make dev-erlang` before direct `retinued` or game-alias tests and after daemon
application metadata changes. Run `make prod-erlang` before production CLI tests. Run the matching companion target after Rust,
asset, or native bridge changes.

Companion dev builds keep application code unoptimized with debug checks and symbols.
Third-party Rust dependencies are optimized; the native renderer always uses Release.

The first PoE2 build downloads the checksum-pinned Fontin archive from its author
for program embedding. Later builds use `.cache/fonts/fontin` without network
access. To prepare that cache separately, run `python3 scripts/setup-fontin.py`.

Each staging step prepares a private prefix under `.staging/`, then atomically exchanges it with
`dev/` or `prod/`. Writers are serialized; failures before activation leave the installed tree
unchanged. Preparation uses reflinks where available and copies otherwise. Activation requires
Linux filesystem support for `renameat2(RENAME_EXCHANGE)` on the destination filesystem.
Both Erlang releases contain copied files; dev keeps debug metadata and `_build/` source paths.

### Qt Reference

Qt is excluded from normal builds. Existing `wfgui-qt` executables remain usable
alongside Rust. To rebuild that reference, use vcpkg manifest mode:

```bash
export VCPKG_ROOT=/path/to/vcpkg
make gui-qt-dev
```

Configuration runs `vcpkg` from `PATH`; `VCPKG_ROOT` supplies the shared ports and CMake
scripts. Homebrew's executable works directly. Override the executable with
`-DWFCLI_VCPKG_EXECUTABLE=/path/to/vcpkg` when configuring CMake.

`make gui-qt-dev` selects Clang from `PATH`; set `LLVM_ROOT` to select another complete LLVM prefix.
System packages must include the matching LLVM tools, libc++, libc++abi, and libunwind development
files. GUI builds refresh CMake configuration automatically, preserving unchanged objects,
vcpkg installations and compiler caches. No clean is needed after an LLVM update. Keep custom
CMake options in presets, not only in the generated cache.
Host tools and target libraries share one fixed `x86-64-v2` triplet in dev and prod. The build environment supplies LLVM's runtime
path while generated tools execute. vcpkg archives and sccache data remain under `.cache/`;
compiler output remains under `_build/`.

VS Code CMake Tools uses the Qt reference presets and existing `_build/cmake/` trees.

## Tests

- `./scripts/test-quiet eunit`: EUnit with passing output suppressed.
- `./scripts/test-quiet ct`: Common Test with passing output suppressed.
- `./scripts/test-quiet gui`: native desktop model tests with build output suppressed.
- `make test-staging`: failed/interrupted installs, concurrent writers, and prefix activation.
- `make test-build`: compiler changes, incremental builds, tool overrides and build ordering.
- `make test-companion`: shared native libraries, inspector, and all companion tests.
- `make test-poe2`: staged observer, daemon and CLI integration in isolated storage and networking; requires Python 3.
  Reports remain under `research/retinue/tests/poe2/`; temporary runtime files are removed.
- `make test-gui`: native desktop model tests.
- `make test-release`: production startup under the x86-64-v2 baseline using QEMU user emulation.
- `make test`: Erlang, Rust, native desktop, staging and PoE2 integration suites.
- `make check`: Rust formatting, xref, tests, and both staged builds.

Normal GUI builds compile the app only; `make test-gui` also builds its test executables.

`make bench-overlays` runs opt-in release-mode render/resource measurements for
both companions. Reports and comparison commands: [overlay benchmarks](../../tools/render-bench/README.md).
`make bench-overlay-scheduling` compares polling and event-driven scheduling on
a real Wayland surface using a click-through Evaluate fixture. It does not reload
companions or interact with the game.

The quiet wrapper prints one line on success. On failure it prints a bounded tail and retains the
full log under `/tmp`. Use direct `rebar3` only while debugging a failure.
Move logs needed as lasting evidence to ignored `research/` before referencing them in notes.

Run tests after code, fixture, build, or behavior changes. Documentation-only changes do not need
tests. Manually exercise changed CLI commands after automated tests pass.

## Generated Files

```bash
make native-compile-commands
make fix-executables
make previews PREVIEW_MEDIA=image
```

`fix-executables` applies executable mode to every tracked shebang file.
Preview variables and reference setup are documented in the
[companion guide](../companion.md#previews).

## Erlang Tools

OTP 29 is the source and runtime baseline. ELP discovers the umbrella from root `rebar.config`,
which also scans application `src/` trees recursively. Run xref after application-boundary
changes:

```bash
rebar3 xref
```

## Packaging

Treat `prod/` as an installation prefix. Packages must preserve `bin/`, `libexec/`, `BUILD_ID`,
and `BUILD_FLAVOR` together. Executables locate private files relative to that prefix. Package
adapters may relocate the complete tree but must not split those files across unrelated roots.
Declare `wfcompanion` runtime tools as package dependencies rather than copying host executables
into `libexec/`.

## Fixtures

- Worldstate and query fixtures: `apps/retinue_cli/test/fixtures/`
- Companion image fixtures: `apps/wfcompanion/tests/fixtures/`

Fixtures are read-only inputs. Writable caches and generated output belong under Common Test
`priv_dir` or a unique `/tmp` path.
