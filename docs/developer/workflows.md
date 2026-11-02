# Development Workflows

## Build

### Dependencies

For normal source builds:

| Area | Requirements |
| --- | --- |
| Common | Git, GNU Make, CMake, and a C/C++ compiler |
| Erlang | Erlang/OTP 29 and Rebar3 |
| Companion | Rust, Cargo, Python 3, `jq`, MinGW-w64, pkg-config, and libxkbcommon development files |
| Desktop GUI | Rust, Cargo, pkg-config, and libxkbcommon development files |

Warframe's runtime integration requires KDE Plasma on Wayland, KScreen, and
Tesseract 5 with English language data. `sccache` is optional. Animated previews
need FFmpeg with VP9 support; release archives need `zip`.

### Targets

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

Native renderer configuration is owned by Cargo's build script. Compiler or flag
changes trigger reconfiguration; no clean is normally needed after a toolchain
update. Make selects Clang from `PATH`; use `LLVM_ROOT` for a particular LLVM
installation, or set `CC` and `CXX` explicitly.

## Tests

See [testing](testing.md) for ownership, fixture rules and performance methodology.

- `./scripts/test-quiet eunit`: EUnit with passing output suppressed.
- `./scripts/test-quiet ct`: Common Test with passing output suppressed.
- `./scripts/test-quiet gui`: native desktop model tests with build output suppressed.
- `make test-staging`: failed/interrupted installs, concurrent writers, and prefix activation.
- `make test-build`: compiler changes, incremental builds, tool overrides and build ordering.
- `make test-native`: shared protocol, client, native/game libraries and inspector tests.
- `make test-ui`: renderer/SDK tests across headless, overlay and desktop features.
- `make test-companion`: the three companion applications.
- `make test-tools`: benchmark, integration-helper and documentation-tool tests.
- `make test-protocol`: generated native schemas and client framing tests.
- `make test-integration` (or `test-poe2`): staged observer, daemon and CLI integration in isolated storage and networking; requires Python 3.
  Reports remain under `research/retinue/tests/poe2/`; temporary runtime files are removed.
- `make test-gui`: native desktop model tests.
- `make test-gui GUI_TEST=presentation::foundry::`: focused app tests.
- `make test-ui-sdk UI_TEST='--test modifiers'`: one SDK integration suite.
- `make test-composition`: shared authoring, layout, paint retention and damage-repair tests.
- `make test-ui-macros`: authoring diagnostics and generated-code integration tests.
- `make docs`: guides and generated Rust/Erlang/CLI/protocol reference, with link checks.
- `make docs-serve`: local site at `http://127.0.0.1:4321/retinue/`.
- `make test-docs`: documentation generator regression tests.
- `make view-demo`: standalone shared UI example; `VIEW_ARGS='--capture'` saves
  offscreen images and layout reports. `make bench-views` measures its retained pipeline.
- `make view-sdk-demo`: new `View::body` runtime example, rendered offscreen at
  100/125/200%; `VIEW_OUTPUT` overrides the capture/report directory.
- `make view-sdk-content`: composed/Canvas card equivalence, streamed image states
  and partial-repair checks with captures at the same scales.
- `make test-release`: production startup under the x86-64-v2 baseline using QEMU user emulation.
- `make test`: Erlang, Rust, native desktop, staging and PoE2 integration suites.
- `make check`: Rust formatting, xref, tests, and both staged builds.

Normal GUI builds compile the app only; `make test-gui` also builds its test executables.

`make bench-gui GUI_BENCH_CASE=foundry_scroll_benchmark
GUI_BENCH_OUTPUT=research/gui-scroll.json` measures scrolling at 100/125/200%.
Use `rail_animation_benchmark` for rail motion and `GUI_BENCH_THREADS=1..8` to compare
thread budgets. Scroll reports separate tree update, submission, native completion,
caller/process CPU and actual thread counts. Run benchmarks separately from builds;
alternate baseline/candidate runs and retain raw samples. They exclude presentation
and game performance.

`make bench-overlays` runs opt-in release-mode render/resource measurements for
both companions. `make bench-list` lists maintained workloads and experiments;
`make bench BENCH_SUITE=gui-rails` records independent release runs with provenance.
Reports and comparison commands: [benchmarks](../../tools/benchmarks/README.md).
`make bench-overlay-scheduling` compares polling and event-driven scheduling on
a real Wayland surface using a click-through Evaluate fixture. It does not reload
companions or interact with the game.

The quiet wrapper prints one line on success. On failure it prints a bounded tail and retains the
full log under `research/retinue/tests/`. Use direct `rebar3` only while debugging a failure.

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

- Shared worldstate, query and trade fixtures: `test/fixtures/`
- Companion image fixtures: `apps/wfcompanion/tests/fixtures/`

Fixtures are read-only inputs. Writable caches and generated output belong under Common Test
`priv_dir` or a unique `/tmp` path.
