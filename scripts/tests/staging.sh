#!/usr/bin/env bash
# shellcheck disable=SC2016 # Generated script variables expand when the fixtures run.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script="$root/scripts/tests/staging.sh"
source "$root/scripts/lib/staging.sh"

if [[ "${1:-}" == writer ]]; then
    destination="$2"
    marker="$3"
    mode="${4:-success}"
    gate="${5:-}"
    [[ -z "$gate" ]] || touch "$gate/$marker.started"
    stage_begin "$destination"
    mkdir -p "$stage/bin"
    printf '%s\n' "$marker" > "$stage/bin/$marker"
    case "$mode" in
        fail) exit 17 ;;
        pause)
            touch "$gate/$marker.ready"
            while [[ ! -e "$gate/release" ]]; do sleep 0.02; done
            ;;
    esac
    stage_commit
    exit
fi

mkdir -p "$root/_build"
fixture="$(mktemp -d "$root/_build/staging-test.XXXXXX")"
foreign=""
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    rm -rf "$fixture"
    [[ -z "$foreign" ]] || rm -rf "$foreign"
}
trap cleanup EXIT
fail() { printf 'staging: %s\n' "$*" >&2; exit 1; }
await_file() {
    for _ in {1..250}; do
        [[ ! -e "$1" ]] || return 0
        sleep 0.02
    done
    fail "timed out waiting for $1"
}

prefix="$fixture/install"
bash "$script" writer "$prefix" first
[[ "$(< "$prefix/bin/first")" == first ]] || fail 'first installation failed'
[[ "$(stat -c %a "$prefix")" == 755 ]] || fail 'new prefix is not traversable by installed users'
printf 'untouched\n' > "$prefix/sentinel"
if bash "$script" writer "$prefix" failed fail; then fail 'injected failure succeeded'; fi
[[ ! -e "$prefix/bin/failed" && -f "$prefix/bin/first" ]] || fail 'failed stage changed installation'
[[ "$(< "$prefix/sentinel")" == untouched ]] || fail 'copy modified its source'

gate="$fixture/interruption"
mkdir "$gate"
bash "$script" writer "$prefix" interrupted pause "$gate" &
pids=("$!")
await_file "$gate/interrupted.ready"
[[ ! -e "$prefix/bin/interrupted" ]] || fail 'preparation became visible'
kill -TERM "${pids[0]}"
if wait "${pids[0]}"; then fail 'interrupted stage succeeded'; fi
pids=()
[[ -f "$prefix/bin/first" && ! -e "$prefix/bin/interrupted" ]] || fail 'interruption changed installation'

gate="$fixture/killed"
mkdir "$gate"
bash "$script" writer "$prefix" killed pause "$gate" &
pids=("$!")
await_file "$gate/killed.ready"
kill -KILL "${pids[0]}"
wait "${pids[0]}" 2>/dev/null || true
pids=()
[[ -f "$prefix/bin/first" && ! -e "$prefix/bin/killed" ]] || fail 'SIGKILL changed installation'
bash "$script" writer "$prefix" recovered
for temporary in "$fixture/.staging/install.tmp."*; do
    [[ ! -d "$temporary" ]] || fail 'abandoned preparation was not cleaned'
done

gate="$fixture/concurrent"
mkdir "$gate"
bash "$script" writer "$prefix" left pause "$gate" &
pids=("$!")
await_file "$gate/left.ready"
bash "$script" writer "$prefix" right success "$gate" &
pids+=("$!")
await_file "$gate/right.started"
[[ ! -e "$prefix/bin/left" && ! -e "$prefix/bin/right" ]] || fail 'concurrent writer bypassed lock'
touch "$gate/release"
for pid in "${pids[@]}"; do wait "$pid"; done
pids=()
[[ -f "$prefix/bin/left" && -f "$prefix/bin/right" ]] || fail 'concurrent writer lost another component'

ln -s "$prefix" "$fixture/linked-prefix"
bash "$script" writer "$fixture/linked-prefix" linked
[[ -L "$fixture/linked-prefix" && -f "$prefix/bin/linked" ]] || fail 'linked prefix was replaced'

repo="$fixture/repo"
release="$repo/_build/default/rel/retinue_daemon"
mkdir -p "$repo/scripts/lib" "$repo/tools/staging" "$repo/commands" \
    "$repo/_build/default/bin" "$release/bin" "$release/erts-test/bin" \
    "$release/lib" "$repo/source/ebin"
cp "$root/scripts/"{stage-erlang,stage-gui,retinue-wrapper,retinue-daemon-wrapper} "$repo/scripts/"
cp "$root/scripts/lib/staging.sh" "$repo/scripts/lib/"
cp "$root/tools/staging/exchange.c" "$repo/tools/staging/"
cat > "$repo/_build/default/bin/retinue" <<'SH'
#!/usr/bin/env bash
[[ "${FAIL_AT:-}" != completion ]] || exit 17
if [[ "${1:-}" == completion ]]; then
    printf 'completion\n'
else
    printf '%s\n' "$RETINUE_CLI_SCOPE" "$RETINUE_BUILD_FLAVOR" "$RETINUE_INSTALL_ROOT" "$@"
fi
SH
printf '#!/usr/bin/env bash\nexec "$@"\n' > "$release/erts-test/bin/escript"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$RETINUE_BUILD_FLAVOR" "$RETINUE_INSTALL_ROOT" "$@"\n' \
    > "$release/bin/retinue_daemon"
cat > "$repo/commands/erl" <<'SH'
#!/usr/bin/env bash
set -eu
[[ "${FAIL_AT:-}" != identity ]] || exit 17
[[ -d "$RETINUE_PLATFORM_EBIN" && "$RETINUE_PLATFORM_EBIN" == */retinue_core-*/ebin ]]
[[ -d "$RETINUE_CORE_EBIN" && "$RETINUE_CORE_EBIN" == */wfcore-*/ebin ]]
[[ -d "$RETINUE_DAEMON_EBIN" && "$RETINUE_DAEMON_EBIN" == */retinue_daemon-*/ebin ]]
printf '%s' "$FAKE_ID"
SH
chmod +x "$repo/_build/default/bin/retinue" "$release/erts-test/bin/escript" \
    "$release/bin/retinue_daemon" "$repo/commands/erl"
printf old-beam > "$repo/source/ebin/core.beam"
cp -a "$repo/source" "$release/lib/wfcore-test"
cp -a "$repo/source" "$release/lib/retinue_core-test"
cp -a "$repo/source" "$release/lib/retinue_daemon-test"
mkdir -p "$repo/dev/bin" "$repo/dev/libexec/wfdaemon" "$repo/dev/libexec/wfcli" \
    "$repo/dev/share/bash-completion/completions"
touch "$repo/dev/bin/wfdaemon" "$repo/dev/share/bash-completion/completions/unrelated.bash"
PATH="$repo/commands:$PATH" FAKE_ID=old bash "$repo/scripts/stage-erlang" dev
[[ "$(< "$repo/dev/BUILD_ID")" == old ]] || fail 'Erlang identity was not installed'
[[ ! -e "$repo/dev/bin/wfdaemon" && ! -e "$repo/dev/libexec/wfdaemon" && \
   ! -e "$repo/dev/libexec/wfcli" ]] || fail 'retired Erlang entry points remain installed'
[[ -f "$repo/dev/share/bash-completion/completions/unrelated.bash" ]] || fail 'unrelated completion removed'
for name in retinue retinued wfcli wfclid poe2cli poe2clid arccli arcclid; do
    [[ "$(< "$repo/dev/share/bash-completion/completions/$name.bash")" == completion ]] || fail "$name completion missing"
done
for name in retinue wfcli poe2cli arccli; do
    case "$name" in retinue) scope=retinue ;; wfcli) scope=warframe ;; poe2cli) scope=poe2 ;; arccli) scope=arc ;; esac
    expected="$(printf '%s\n' "$scope" dev "$repo/dev" '--json' 'two words')"
    [[ "$(RETINUE_CLI_SCOPE=invalid "$repo/dev/bin/$name" --json 'two words')" == "$expected" ]] || fail "$name wrapper lost scope or arguments"
    ln -s "dev/bin/$name" "$repo/${name}d"
    [[ "$("$repo/${name}d" --json 'two words')" == "$expected" ]] || fail "$name development link lost scope"
done
expected="$(printf '%s\n' retinue dev "$repo/dev" paths retinue_daemon)"
[[ "$("$repo/dev/bin/retinue-daemon" paths)" == "$expected" ]] || fail 'daemon paths uses wrong CLI owner'
expected="$(printf '%s\n' dev "$repo/dev" foreground '--flag=two words')"
[[ "$("$repo/dev/bin/retinue-daemon" foreground '--flag=two words')" == "$expected" ]] || fail 'daemon release wrapper lost arguments'
printf new-beam > "$release/lib/wfcore-test/ebin/core.beam"
[[ "$(< "$repo/dev/libexec/retinue_daemon/lib/wfcore-test/ebin/core.beam")" == old-beam ]] || fail 'compilation changed installed BEAMs'
for point in completion identity; do
    if PATH="$repo/commands:$PATH" FAKE_ID=new FAIL_AT="$point" \
        bash "$repo/scripts/stage-erlang" dev; then fail "$point failure succeeded"; fi
    [[ "$(< "$repo/dev/BUILD_ID")" == old && -x "$repo/dev/bin/wfcli" ]] || fail "$point failure replaced installation"
done
PATH="$repo/commands:$PATH" FAKE_ID=new bash "$repo/scripts/stage-erlang" dev
[[ "$(< "$repo/dev/BUILD_ID")" == new && \
   "$(< "$repo/dev/libexec/retinue_daemon/lib/wfcore-test/ebin/core.beam")" == new-beam ]] || fail 'Erlang activation was incomplete'

cat > "$repo/commands/cmake" <<'SH'
#!/usr/bin/env bash
set -eu
mkdir -p "$4/lib" "$4/Qt6" "$4/share/retinue/games/warframe/icons" "$4/share/licenses/wfgui"
printf new-lib > "$4/lib/qt"
touch "$4/share/retinue/games/warframe/icons/wfgui.svg" "$4/share/licenses/wfgui/LLVM-LICENSE.txt"
[[ "${FAIL_AT:-}" != gui ]]
SH
chmod +x "$repo/commands/cmake"
mkdir -p "$repo/dev/lib" "$repo/dev/Qt6" "$repo/dev/share/wfcli/icons" "$repo/dev/share/licenses/wfcli"
printf old-lib > "$repo/dev/lib/qt"
touch "$repo/dev/share/wfcli/icons/"{wfgui.svg,wfgui-256.png,unrelated.svg} \
    "$repo/dev/share/licenses/wfcli/"{Lucide-LICENSE.txt,LLVM-LICENSE.txt,unrelated.txt}
if PATH="$repo/commands:$PATH" FAIL_AT=gui bash "$repo/scripts/stage-gui" dev qt; then
    fail 'failed GUI installation succeeded'
fi
[[ "$(< "$repo/dev/lib/qt")" == old-lib ]] || fail 'GUI failure removed live libraries'
[[ -f "$repo/dev/share/wfcli/icons/wfgui.svg" && -f "$repo/dev/share/licenses/wfcli/LLVM-LICENSE.txt" ]] || fail 'GUI failure removed live resources'
PATH="$repo/commands:$PATH" bash "$repo/scripts/stage-gui" dev qt
[[ "$(< "$repo/dev/lib/qt")" == new-lib && -x "$repo/dev/bin/wfcli" ]] || fail 'GUI activation lost Erlang files'
[[ ! -e "$repo/dev/share/wfcli/icons/wfgui.svg" && ! -e "$repo/dev/share/wfcli/icons/wfgui-256.png" && \
   ! -e "$repo/dev/share/licenses/wfcli/LLVM-LICENSE.txt" && ! -e "$repo/dev/share/licenses/wfcli/Lucide-LICENSE.txt" ]] || fail 'retired GUI resources remain'
[[ -f "$repo/dev/share/retinue/games/warframe/icons/wfgui.svg" && -f "$repo/dev/share/licenses/wfgui/LLVM-LICENSE.txt" ]] || fail 'GUI resources missing'
[[ -f "$repo/dev/share/wfcli/icons/unrelated.svg" && -f "$repo/dev/share/licenses/wfcli/unrelated.txt" ]] || fail 'unrelated resources removed'
rm "$repo/dev/share/wfcli/icons/unrelated.svg" "$repo/dev/share/licenses/wfcli/unrelated.txt"
PATH="$repo/commands:$PATH" bash "$repo/scripts/stage-gui" dev qt
[[ ! -d "$repo/dev/share/wfcli" && ! -d "$repo/dev/share/licenses/wfcli" ]] || fail 'empty retired resource directories remain'

if [[ -w /dev/shm && "$(stat -c %d /dev/shm)" != "$(stat -c %d "$fixture")" ]]; then
    foreign="$(mktemp -d /dev/shm/wfcli-staging-test.XXXXXX)"
    printf 'foreign\n' > "$foreign/sentinel"
    if "$root/_build/tools/stage-exchange" "$foreign" "$prefix"; then
        fail 'cross-filesystem activation succeeded'
    fi
    [[ "$(< "$prefix/sentinel")" == untouched && -f "$foreign/sentinel" ]] || fail 'failed exchange damaged a tree'
fi

printf 'staging checks passed\n'
