#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$root/_build"
fixture="$(mktemp -d "$root/_build/build-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
fail() { printf 'build: %s\n' "$*" >&2; exit 1; }

python3 "$root/scripts/tests/fontin.py"

repo="$fixture/gui"
mkdir -p "$repo/scripts" "$repo/commands" "$repo/apps/wfgui"
cp "$root/Makefile" "$root/VERSION" "$repo/"
cat > "$repo/CMakeLists.txt" <<'CMAKE'
cmake_minimum_required(VERSION 3.28)
project(build_fixture LANGUAGES CXX)
add_subdirectory(apps/wfgui)
CMAKE
cat > "$repo/apps/wfgui/CMakeLists.txt" <<'CMAKE'
add_executable(wfgui main.cpp)
CMAKE
cat > "$repo/apps/wfgui/main.cpp" <<'CPP'
#include <cstdio>
int main() { std::printf("%d", TEST_VALUE); }
CPP
cat > "$repo/CMakePresets.json" <<'JSON'
{
  "version": 6,
  "configurePresets": [{
    "name": "gui-dev", "generator": "Ninja",
    "binaryDir": "${sourceDir}/_build/cmake/gui-dev"
  }],
  "buildPresets": [{"name": "gui-dev", "configurePreset": "gui-dev", "targets": ["wfgui"]}]
}
JSON
printf '#!/bin/sh\nexit 0\n' > "$repo/scripts/stage-gui"
chmod +x "$repo/scripts/stage-gui"
ln -s "$(command -v clang++)" "$repo/commands/old-clang++"
make --no-print-directory -C "$repo" gui-dev SCCACHE= \
    CXX="$repo/commands/old-clang++" CXXFLAGS=-DTEST_VALUE=1
binary="$repo/_build/cmake/gui-dev/apps/wfgui/wfgui"
[[ "$("$binary")" == 1 ]] || fail 'initial compiler flags missing'
rm "$repo/commands/old-clang++"
ln -s "$(command -v clang++)" "$repo/commands/new-clang++"
make --no-print-directory -C "$repo" gui-dev SCCACHE= \
    CXX="$repo/commands/new-clang++" CXXFLAGS=-DTEST_VALUE=2
[[ "$("$binary")" == 2 ]] || fail 'stale compiler or flags survived'
timestamp="$(stat -c %y "$binary")"
make --no-print-directory -C "$repo" gui-dev SCCACHE= \
    CXX="$repo/commands/new-clang++" CXXFLAGS=-DTEST_VALUE=2
[[ "$(stat -c %y "$binary")" == "$timestamp" ]] || fail 'fresh configure rebuilt unchanged executable'

cat > "$repo/order.mk" <<'MAKE'
dev-erlang prod-erlang dev-companion prod-companion:
	@sleep 0.05
gui-configure-dev gui-configure-prod:
	@:
gui-dev gui-prod:
	@sleep 0.1; touch $@.done
MAKE
printf '#!/bin/sh\ntest -f gui-dev.done && test -f gui-prod.done\n' > "$repo/scripts/native-compile-commands"
chmod +x "$repo/scripts/native-compile-commands"
ln -s prod/bin/wfdaemon "$repo/wfdaemon"
ln -s dev/bin/wfdaemon "$repo/wfdaemond"
make --no-print-directory -C "$repo" -f Makefile -f order.mk -j8 build
[[ ! -L "$repo/wfdaemon" && ! -L "$repo/wfdaemond" ]] || fail 'retired daemon links remain'
for name in retinue wfcli poe2cli arccli retinue-daemon retinue-inspect wfinspect wfcompanion poe2companion arccompanion wfgui; do
    [[ "$(readlink "$repo/$name")" == "prod/bin/$name" ]] || fail "$name production link missing"
    [[ "$(readlink "$repo/${name}d")" == "dev/bin/$name" ]] || fail "$name development link missing"
done

repo="$fixture/native"
mkdir -p "$repo/crates/retinue-overlay/src" "$repo/crates/retinue-overlay/native" \
    "$repo/crates/retinue-overlay/vendor/"{blend2d,asmjit} \
    "$repo/apps/retinue_daemon/src/runtime" "$repo/commands"
cp "$root/crates/retinue-overlay/build.rs" "$repo/crates/retinue-overlay/"
printf '0.0.0\n' > "$repo/VERSION"
printf 'fn main() {}\n' > "$repo/crates/retinue-overlay/src/main.rs"
touch "$repo/crates/retinue-overlay/vendor/"{blend2d,asmjit}/CMakeLists.txt
printf '%s\n' '-define(ENVELOPE_VERSION, 1).' '-define(INTERFACE_PLAYER, 1).' \
    'interfaces() -> #{<<"player">> => ?INTERFACE_PLAYER}.' \
    > "$repo/apps/retinue_daemon/src/runtime/retinue_local_protocol.erl"
cat > "$repo/crates/retinue-overlay/Cargo.toml" <<'TOML'
[workspace]
[package]
name = "native-build-fixture"
version = "0.0.0"
edition = "2024"
TOML
cat > "$repo/commands/cmake" <<'SH'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$WF_TEST_CALLS"
if [[ "$1" == --build ]]; then
    [[ "$MAKEFLAGS" == *jobserver* ]]
    [[ "$*" != *--parallel* ]]
else
    [[ "$1" == --fresh && "$3" == 'Unix Makefiles' ]]
fi
SH
printf '#!/bin/sh\nexec %s "$@"\n' "$(command -v clang)" > "$repo/commands/cc"
cp "$repo/commands/cc" "$repo/commands/c++"
chmod +x "$repo/commands/"*
export WF_TEST_CALLS="$repo/cmake-calls"
native_check() {
    PATH="$repo/commands:$PATH" CC="$repo/commands/cc" CXX="$repo/commands/c++" \
        CARGO_TARGET_DIR="$repo/target" RUSTC_WRAPPER='' \
        "${CARGO:-cargo}" check --quiet --manifest-path "$repo/crates/retinue-overlay/Cargo.toml"
}
native_check
[[ "$(wc -l < "$WF_TEST_CALLS")" == 2 ]] || fail 'native configure/build not called'
native_check
[[ "$(wc -l < "$WF_TEST_CALLS")" == 2 ]] || fail 'unchanged native build reran'
printf '\n' >> "$repo/commands/cc"
native_check
[[ "$(wc -l < "$WF_TEST_CALLS")" == 4 ]] || fail 'compiler replacement did not rerun native build'
CXXFLAGS=-DNEW_FLAG native_check
[[ "$(wc -l < "$WF_TEST_CALLS")" == 6 ]] || fail 'compiler flag change did not rerun native build'

repo="$fixture/cargo"
mkdir -p "$repo/scripts" "$repo/.cache/downloads" "$repo/.cache/tools/oozextract-0.5.5" "$repo/commands"
cp "$root/scripts/build-companion" "$root/scripts/build-oozextract" "$repo/scripts/"
touch "$repo/.cache/downloads/oozextract-0.5.5.crate" "$repo/.cache/tools/oozextract-0.5.5/.unpacked"
printf '#!/bin/sh\nexit 0\n' > "$repo/commands/sha256sum"
printf '#!/bin/sh\nprintf "called\\n" >> "$WF_TEST_CALLS"\nexit 42\n' > "$repo/commands/custom-cargo"
chmod +x "$repo/commands/"*
export WF_TEST_CALLS="$repo/cargo-calls"
if PATH="$repo/commands:$PATH" CARGO="$repo/commands/custom-cargo" \
    bash "$repo/scripts/build-oozextract" dev; then fail 'cargo failure swallowed'; fi
[[ "$(wc -l < "$WF_TEST_CALLS")" == 1 ]] || fail 'oozextract ignored CARGO'
printf '#!/bin/sh\nexit 0\n' > "$repo/scripts/build-oozextract"
cp "$repo/scripts/build-oozextract" "$repo/scripts/build-debug-bridge"
chmod +x "$repo/scripts/"*
if CARGO="$repo/commands/custom-cargo" bash "$repo/scripts/build-companion" dev; then
    fail 'cargo failure swallowed'
fi
[[ "$(wc -l < "$WF_TEST_CALLS")" == 2 ]] || fail 'companion ignored CARGO'

mkdir -p "$repo/scripts/lib" "$repo/tools/staging" \
    "$repo/apps/wfcompanion/assets" "$repo/tools/wf-luau-decompiler" \
    "$repo/apps/poe2companion/assets" "$repo/assets/fonts/roboto" \
    "$repo/_build/native/dev" "$repo/target/debug" \
    "$repo/target/oozextract/debug/licenses/oozextract" \
    "$repo/target/wf-luau-decompiler/debug"
cp "$root/scripts/lib/staging.sh" "$repo/scripts/lib/"
cp "$root/tools/staging/exchange.c" "$repo/tools/staging/"
printf '#!/bin/sh\nexit 0\n' > "$repo/commands/custom-cargo"
printf '#!/bin/sh\nprintf "old-companion\\n"\n' > "$repo/target/debug/wfcompanion"
printf '#!/bin/sh\nprintf "poe2-companion\\n"\n' > "$repo/target/debug/poe2companion"
printf '#!/bin/sh\nprintf "arc-companion\\n"\n' > "$repo/target/debug/arccompanion"
printf '#!/bin/sh\nprintf "inspector\\n"\n' > "$repo/target/debug/retinue-inspect"
printf '#!/bin/sh\nexit 0\n' > "$repo/_build/native/dev/wfcompanion-debug-output.exe"
cp "$repo/_build/native/dev/wfcompanion-debug-output.exe" "$repo/target/oozextract/debug/unoodle"
cp "$repo/_build/native/dev/wfcompanion-debug-output.exe" "$repo/target/wf-luau-decompiler/debug/wf-luau-decompiler"
touch "$repo/apps/wfcompanion/assets/LuauDecompiler-LICENSE.txt" "$repo/tools/wf-luau-decompiler/LICENSE.txt"
touch "$repo/apps/poe2companion/assets/Lato-OFL.txt" "$repo/assets/fonts/roboto/Roboto-LICENSE.txt"
touch "$repo/apps/poe2companion/assets/Fontin-NOTICE.txt"
CARGO="$repo/commands/custom-cargo" CARGO_TARGET_DIR="$repo/target" \
    bash "$repo/scripts/build-companion" dev
[[ -x "$repo/dev/bin/retinue-inspect" && "$(readlink "$repo/dev/bin/wfinspect")" == retinue-inspect ]] || fail 'inspector public entry or alias missing'
[[ "$("$repo/dev/bin/poe2companion")" == poe2-companion ]] || fail 'PoE2 companion missing'
[[ -f "$repo/dev/share/licenses/poe2companion/Lato-OFL.txt" && \
   -f "$repo/dev/share/licenses/poe2companion/Fontin-NOTICE.txt" && \
   -f "$repo/dev/share/licenses/fonts/Roboto-LICENSE.txt" ]] || fail 'overlay font licenses missing'
[[ "$("$repo/dev/bin/retinue-inspect")" == inspector && \
   "$("$repo/dev/bin/wfinspect")" == inspector ]] || fail 'inspector alias does not execute'
printf '#!/bin/sh\nprintf "changed-companion\\n"\n' > "$repo/target/debug/wfcompanion"
rm "$repo/target/debug/retinue-inspect"
if CARGO="$repo/commands/custom-cargo" CARGO_TARGET_DIR="$repo/target" \
    bash "$repo/scripts/build-companion" dev; then fail 'missing inspector staging succeeded'; fi
[[ "$("$repo/dev/bin/wfcompanion")" == old-companion ]] || fail 'failed companion stage replaced installation'

cmake "-DTEST_DIR=$fixture/toolchain" -P "$root/cmake/tests/gui-toolchain.cmake"
cmake "-DTEST_DIR=$fixture/vcpkg" -P "$root/cmake/tests/gui-vcpkg.cmake"
python3 "$root/scripts/tests/cache_helpers.py"
printf 'build system checks passed\n'
