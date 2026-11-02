#!/usr/bin/env bash
# shellcheck disable=SC2016
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$root/_build"
fixture="$(mktemp -d "$root/_build/build-test.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
fail() { printf 'build: %s\n' "$*" >&2; exit 1; }

python3 "$root/scripts/tests/fontin.py"

proto="$fixture/protobuf"
mkdir -p "$proto/scripts"
cp "$root/scripts/protobuf-deps" "$proto/scripts/"
printf '{}' > "$proto/rebar.config"
for app in retinue_core wfcore poe2core; do
    mkdir -p "$proto/apps/$app/proto" "$proto/apps/$app/src/generated"
    printf '{}' > "$proto/apps/$app/rebar.config"
    printf 'syntax = "proto3";' > "$proto/apps/$app/proto/test.proto"
done
bash "$proto/scripts/protobuf-deps"
for app in retinue_core wfcore poe2core; do
    printf generated > "$proto/apps/$app/src/generated/test_pb.erl"
done
bash "$proto/scripts/protobuf-deps"
[[ -f "$proto/apps/wfcore/src/generated/test_pb.erl" ]] || fail 'unchanged protobuf inputs invalidated output'
printf '\n// import changed' >> "$proto/apps/retinue_core/proto/test.proto"
bash "$proto/scripts/protobuf-deps"
for app in retinue_core wfcore poe2core; do
    [[ ! -f "$proto/apps/$app/src/generated/test_pb.erl" ]] || fail "stale protobuf import in $app"
    printf generated > "$proto/apps/$app/src/generated/test_pb.erl"
done
printf '\n// game changed' >> "$proto/apps/wfcore/proto/test.proto"
bash "$proto/scripts/protobuf-deps"
[[ ! -f "$proto/apps/wfcore/src/generated/test_pb.erl" ]] || fail 'changed game schema was not invalidated'
[[ -f "$proto/apps/poe2core/src/generated/test_pb.erl" ]] || fail 'unrelated game schema was invalidated'

repo="$fixture/gui"
mkdir -p "$repo/scripts"
cp "$root/Makefile" "$root/VERSION" "$repo/"
printf '#!/bin/sh\nexit 0\n' > "$repo/scripts/stage-gui"
chmod +x "$repo/scripts/stage-gui"
# Make executes '+' recipes even under -n.
plan="$(make --no-print-directory -n -C "$repo" gui-dev SCCACHE= CARGO=true)"
[[ "$plan" == *'build --locked -p wfgui'* && "$plan" != *cmake* ]] || fail 'default GUI build still uses Qt'
plan="$(make --no-print-directory -n -C "$repo" gui-prod SCCACHE= CARGO=true)"
[[ "$plan" == *'build --locked --release -p wfgui'* && "$plan" != *cmake* ]] || fail 'production GUI bypassed Cargo'
if make --no-print-directory -n -C "$repo" gui-qt-dev 2>/dev/null; then
    fail 'retired Qt build target remains'
fi

cat > "$repo/order.mk" <<'MAKE'
dev-erlang prod-erlang dev-companion prod-companion:
	@sleep 0.05
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
[[ ! -e "$repo/wfgui-qt" && ! -L "$repo/wfgui-qt" ]] || fail 'retired GUI alias was created'

repo="$fixture/native"
mkdir -p "$repo/crates/retinue-render/src" "$repo/crates/retinue-render/native" \
    "$repo/crates/retinue-render/vendor/"{blend2d,asmjit} \
    "$repo/apps/retinue_daemon/src/runtime" "$repo/commands"
cp "$root/crates/retinue-render/build.rs" "$repo/crates/retinue-render/"
printf '0.0.0\n' > "$repo/VERSION"
printf 'fn main() {}\n' > "$repo/crates/retinue-render/src/main.rs"
touch "$repo/crates/retinue-render/vendor/"{blend2d,asmjit}/CMakeLists.txt
printf '%s\n' '-define(ENVELOPE_VERSION, 1).' '-define(INTERFACE_PLAYER, 1).' \
    'interfaces() -> #{<<"player">> => ?INTERFACE_PLAYER}.' \
    > "$repo/apps/retinue_daemon/src/runtime/retinue_local_protocol.erl"
cat > "$repo/crates/retinue-render/Cargo.toml" <<'TOML'
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
        "${CARGO:-cargo}" check --quiet --manifest-path "$repo/crates/retinue-render/Cargo.toml"
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

cat > "$repo/commands/sccache" <<'SH'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >> "$WF_TEST_CALLS"
[[ "$1" != --start-server ]] || exit 0
exec "$@"
SH
chmod +x "$repo/commands/sccache"
export WF_TEST_CALLS="$repo/sccache-calls"
cmake -S "$root/crates/retinue-render/native" -B "$repo/configure" \
    -DBLEND2D_DIR="$root/crates/retinue-render/vendor/blend2d" \
    -DASMJIT_DIR="$root/crates/retinue-render/vendor/asmjit" \
    -DRETINUE_SCCACHE="$repo/commands/sccache" \
    -DCMAKE_C_COMPILER="$(command -v clang)" \
    -DCMAKE_CXX_COMPILER="$(command -v clang++)"
if [[ -f "$WF_TEST_CALLS" ]] && grep -q -- '--start-server' "$WF_TEST_CALLS"; then
    fail 'native configure must not replace the shared sccache server'
fi

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

cp "$root/scripts/stage-gui" "$repo/scripts/"
mkdir -p "$repo/apps/wfgui/resources/app"
cp "$root/apps/wfgui/resources/Lucide-LICENSE.txt" "$repo/apps/wfgui/resources/"
cp "$root/apps/wfgui/resources/app/"wfgui* "$repo/apps/wfgui/resources/app/"
printf '#!/bin/sh\nprintf "native-gui\\n"\n' > "$repo/target/debug/wfgui"
printf 'unrelated staged resource\n' > "$repo/dev/resource-marker"
CARGO_TARGET_DIR="$repo/target" bash "$repo/scripts/stage-gui" dev
[[ "$("$repo/dev/bin/wfgui")" == native-gui && -f "$repo/dev/resource-marker" ]] || fail 'GUI staging replaced unrelated files'
if CARGO_TARGET_DIR="$repo/target" bash "$repo/scripts/stage-gui" dev qt; then
    fail 'retired GUI backend accepted'
fi
for size in 16 24 32 48 64 128 256 512; do
    [[ -f "$repo/dev/share/retinue/games/warframe/icons/wfgui-$size.png" ]] || fail 'desktop icon missing'
done
rm "$repo/target/debug/wfgui"
if CARGO_TARGET_DIR="$repo/target" bash "$repo/scripts/stage-gui" dev; then fail 'missing native GUI staging succeeded'; fi
[[ "$("$repo/dev/bin/wfgui")" == native-gui ]] || fail 'failed native GUI stage replaced installation'

python3 "$root/scripts/tests/cache_helpers.py"
printf 'build system checks passed\n'
