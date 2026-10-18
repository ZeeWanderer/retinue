# Development Workflows

## Build

See [README build instructions](../../README.md#build) for staged
tree layout and dependencies. Contributor targets:

```bash
make dev             # debug Erlang, companion, and GUI
make prod            # optimized Erlang, companion, and GUI
make dev-erlang
make prod-erlang
make dev-companion
make prod-companion
make inspect         # inspector only, no overlay renderer
make gui            # native Qt Widgets development client
make package
```

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

The GUI uses vcpkg manifest mode with the tracked LLVM/libc++ triplet:

```bash
export VCPKG_ROOT=/path/to/vcpkg
make gui
```

Configuration runs `vcpkg` from `PATH`; `VCPKG_ROOT` supplies the shared ports and CMake
scripts. Homebrew's executable works directly. Override the executable with
`-DWFCLI_VCPKG_EXECUTABLE=/path/to/vcpkg` when configuring CMake.

`make gui` selects Clang from `PATH`; set `LLVM_ROOT` to select another complete LLVM prefix.
System packages must include the matching LLVM tools, libc++, libc++abi, and libunwind development
files. GUI builds refresh CMake configuration automatically, preserving unchanged objects,
vcpkg installations and compiler caches. No clean is needed after an LLVM update. Keep custom
CMake options in presets, not only in the generated cache.
Host tools and target libraries share one fixed `x86-64-v2` triplet in dev and prod. The build environment supplies LLVM's runtime
path while generated tools execute. vcpkg archives and sccache data remain under `.cache/`;
compiler output remains under `_build/`.

GUI prerequisites include CMake, Ninja, vcpkg, Autoconf, Autoconf Archive, Automake, and Libtool.
VS Code CMake Tools uses the tracked presets and existing `_build/cmake/` trees.

## Tests

- `./scripts/test-quiet eunit`: EUnit with passing output suppressed.
- `./scripts/test-quiet ct`: Common Test with passing output suppressed.
- `./scripts/test-quiet gui`: native desktop model tests with build output suppressed.
- `make test-staging`: failed/interrupted installs, concurrent writers, and prefix activation.
- `make test-build`: compiler changes, incremental builds, tool overrides and build ordering.
- `make test-companion`: shared native libraries, inspector, and all companion tests.
- `make test-poe2`: staged observer, daemon and CLI integration in isolated storage and networking; requires Python 3.
- `make test-gui`: native desktop model tests.
- `make test-release`: production startup under the x86-64-v2 baseline using QEMU user emulation.
- `make test`: Erlang, Rust, native desktop, staging and PoE2 integration suites.
- `make check`: Rust formatting, xref, tests, and both staged builds.

Normal GUI builds compile the app only; `make test-gui` also builds its test executables.

The quiet wrapper prints one line on success. On failure it prints a bounded tail and retains the
full log under `/tmp`. Use direct `rebar3` only while debugging a failure.

Run tests after code, fixture, build, or behavior changes. Documentation-only changes do not need
tests. Manually exercise changed CLI commands after automated tests pass.

`rebar3 ct` emits an expected `-compile(export_all)` warning for
`apps/retinue_cli/test/wf_forma_plan_SUITE.erl`.

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
