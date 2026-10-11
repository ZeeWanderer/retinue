#!/usr/bin/env python3
"""Staged PoE2 observer/API/CLI smoke test; no games, builds, or live daemon access.

Run: python3 scripts/tests/poe2.py [--stage prod]
Evidence stays in the printed /tmp directory. All owned processes are stopped.
"""

import argparse
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import tempfile
import time


PREFIX = "2026/05/30 19:16:35"
RECORDS = [
    f'{PREFIX} 8147076 2caa233f [DEBUG Client 312] Generating level 1 area "G1_1" with seed 1850020125\r\n',
    f"{PREFIX} 8147077 7fbd1225 [INFO Client 312] [SCENE] Set Source [The Riverbank]\r\n",
    f"{PREFIX} 8147078 3ef23347 [INFO Client 312] : ExampleCharacter (Huntress) is now level 2\r\n",
    f"{PREFIX} 8147079 3ef23347 [INFO Client 312] : ExampleCharacter has been slain.\r\n",
]
FIELDS = {
    "area_generated": {"area_id", "area_level", "seed"},
    "area_name": {"area_name"},
    "character_level": {"character", "character_class", "character_level"},
    "death": {"character"},
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def only(paths, description):
    paths = list(paths)
    require(len(paths) == 1, f"expected exactly one {description}: {paths}")
    return paths[0]


def await_ready(predicate, process, description):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        require(process.poll() is None, f"{description} exited: {process.returncode}")
        if predicate():
            return
        time.sleep(0.05)
    raise TimeoutError(f"waiting for {description}")


class Peer:
    def __init__(self, path, evidence):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(8)
        self.sock.connect(str(path))
        self.buffer = b""
        self.evidence = evidence

    def send(self, value):
        self.sock.sendall(json.dumps(value).encode() + b"\n")

    def receive(self):
        while b"\n" not in self.buffer:
            chunk = self.sock.recv(8192)
            require(chunk, "API disconnected before expected frame")
            self.buffer += chunk
            require(len(self.buffer) <= 1024 * 1024, "API frame exceeded fixture limit")
        line, self.buffer = self.buffer.split(b"\n", 1)
        value = json.loads(line)
        self.evidence.write(json.dumps(value) + "\n")
        self.evidence.flush()
        return value

    def snapshot(self, cursor, count):
        # Only a bounded number of empty-watermark and data publications are expected.
        for _ in range(16):
            value = self.receive()
            require(value.get("event") == "session", "expected subscribed session event")
            require(value.get("game") == "poe2" and value.get("subscription") == 2,
                    "subscription lost game/ID scope")
            data = value["data"]
            if data["cursor"] == cursor:
                require(len(data["events"]) == count, "unexpected historical/event replay")
                return data
        raise RuntimeError("expected cursor not observed within bounded publications")


def execute(stage, root, report):
    release = stage / "libexec/retinue_daemon"
    erts = only(release.glob("erts-*/bin"), "staged Erlang runtime")
    boot = only(release.glob("releases/*/start_clean.boot"), "clean boot file")
    ebins = sorted((release / "lib").glob("*/ebin"))
    for app in ("retinue_core", "wfcore", "retinue_daemon"):
        only((release / "lib").glob(f"{app}-*/ebin"), f"staged {app}")
    cli, native = stage / "bin/retinue", stage / "bin/poe2companion"
    for executable in (cli, native, erts / "erl", erts / "epmd"):
        require(os.access(executable, os.X_OK), f"missing staged executable: {executable}")

    # Never inherit the user's HOME, Erlang cookie/flags, Retinue overrides, or session bus.
    env = {"PATH": f"{erts}:/usr/bin:/bin", "LANG": "C.UTF-8"}
    for name, leaf in {
        "HOME": "home", "XDG_CONFIG_HOME": "config", "XDG_DATA_HOME": "data",
        "XDG_CACHE_HOME": "cache", "XDG_STATE_HOME": "state", "XDG_RUNTIME_DIR": "run",
        "XDG_CONFIG_DIRS": "config-dirs", "XDG_DATA_DIRS": "data-dirs", "TMPDIR": "tmp",
    }.items():
        path = root / leaf
        path.mkdir(mode=0o700)
        env[name] = str(path)
    cookie = Path(env["HOME"]) / ".erlang.cookie"
    cookie.write_text(secrets.token_hex(24))
    cookie.chmod(0o400)
    socket_path = root / "api.sock"
    ready_path = root / "ready"
    env.update({
        "RETINUE_DAEMON_SOCKET": str(socket_path),
        "RETINUE_DAEMON_SCRIPT": "/bin/false",
        "RETINUE_COMMAND": "/bin/false",
        "RETINUE_INSTALL_ROOT": str(stage),
        "RETINUE_UPDATE_ROOT": str(stage),
        "RETINUE_BUILD_FLAVOR": (stage / "BUILD_FLAVOR").read_text().strip(),
        "RETINUE_PACKAGE_MANAGER": "standalone",
        "POE2_E2E_READY": str(ready_path),
        "ERL_CRASH_DUMP": str(root / "erl-crash.dump"),
        "ERL_AFLAGS": "+S 2:2 +A 2",
        "ERL_EPMD_ADDRESS": "127.0.0.1",
    })
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    require(port != 4369, "refusing default EPMD port")
    env["ERL_EPMD_PORT"] = str(port)
    report.update(stage=str(stage), root=str(root), epmd_port=port,
                  node="retinue_daemon@localhost", socket=str(socket_path))
    owned = []
    handles = []
    peer = None

    def spawn(name, command, stdin=subprocess.DEVNULL):
        log = (root / f"{name}.log").open("wb")
        handles.append(log)
        process = subprocess.Popen(command, cwd=root, env=env, stdin=stdin,
                                   stdout=log, stderr=subprocess.STDOUT)
        owned.append((name, process))
        return process

    def command(name, arguments, as_json=True):
        output = subprocess.run([str(cli), *arguments], cwd=root, env=env,
                                capture_output=True, timeout=10)
        (root / f"{name}.out").write_bytes(output.stdout)
        (root / f"{name}.err").write_bytes(output.stderr)
        require(output.returncode == 0, f"staged CLI failed: {name}; see {root}")
        return json.loads(output.stdout) if as_json else output.stdout.decode()

    def status(name):
        return command(name, ["--json", "poe2", "companion", "status"])

    def events(name):
        return command(name, ["--json", "poe2", "companion", "events"])

    try:
        epmd = spawn("epmd", [str(erts / "epmd"), "-port", str(port),
                              "-address", "127.0.0.1"])

        def epmd_ready():
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                    return True
            except OSError:
                return False

        await_ready(epmd_ready, epmd, "private EPMD")
        evaluation = (
            'ok = application:load(retinue_daemon), '
            'application:set_env(retinue_daemon, daemon_enabled, true), '
            'application:set_env(retinue_daemon, daemon_idle_shutdown, false), '
            'application:set_env(retinue_daemon, source_refresh_interval_ms, 0), '
            '{ok, _} = application:ensure_all_started(retinue_daemon), '
            'undefined = maps:get(refresh_timer, sys:get_state(wf_source_manager)), '
            'ok = file:write_file(os:getenv("POE2_E2E_READY"), <<"ready">>), '
            '"stop\\n" = io:get_line(""), '
            'ok = application:stop(retinue_daemon), halt(0).'
        )
        daemon = spawn("daemon", [str(erts / "erl"), "-noshell",
            "-boot", str(boot.with_suffix("")), "-sname", "retinue_daemon@localhost",
            "-kernel", "inet_dist_use_interface", "{127,0,0,1}",
            "-pa", *map(str, ebins), "-eval", evaluation], stdin=subprocess.PIPE)
        await_ready(ready_path.exists, daemon, "isolated production application")
        wire = (root / "subscription.jsonl").open("w")
        handles.append(wire)
        peer = Peer(socket_path, wire)
        peer.send({"id": 1, "op": "hello", "game": "poe2", "client": "poe2-e2e",
                   "envelope": 2, "interfaces": {"session": 1}, "features": []})
        hello = peer.receive()
        require(hello.get("id") == 1 and hello.get("ok") is True
                and hello.get("compatible") is True and hello.get("game") == "poe2"
                and hello.get("envelope") == 2 and hello.get("interfaces") == {"session": 1},
                "real daemon contract mismatch")
        peer.send({"id": 2, "op": "subscribe", "dataset": "session"})
        subscribed = peer.receive()
        require(subscribed.get("id") == 2 and subscribed.get("ok") is True,
                "session subscription rejected")
        require(subscribed["data"]["events"] == [], "isolated daemon has unexpected history")

        log = root / "Client.txt"
        history = RECORDS[2].replace("ExampleCharacter", "HistoricalActor").encode()
        log.write_bytes(history)
        observer = spawn("observer", [str(native), "watch", "--log", str(log)])
        baseline = peer.snapshot(len(history), 0)
        require(baseline["current"] == {} and baseline["updated_at"] is None,
                "EOF watermark fabricated fresh state")
        connected = status("status-initial")
        require(len(connected["observers"]) == 1, "staged CLI did not see observer connection")
        require(connected["observers"][0]["os_pid"] == observer.pid, "wrong observer PID")
        require(connected["observers"][0]["mode"] == "watch", "observer mode was not preserved")
        require(connected["current"] == {} and events("events-initial") == [],
                "historical records reached staged CLI")

        private = (
            f"{PREFIX} 8147000 3ef23347 [INFO Client 312] : @From Private: DO_NOT_FORWARD_CHAT\r\n"
            f"{PREFIX} 8147001 3ef23347 [INFO Client 312] Connecting to instance server at 192.0.2.17:12345\r\n"
        ).encode()
        with log.open("ab") as output:
            output.write(private)
        cursor = len(history) + len(private)
        skipped = peer.snapshot(cursor, 0)
        require(skipped["current"] == {} and skipped["updated_at"] is None,
                "skipped lines fabricated current state")

        offsets = []
        for record in RECORDS:
            cursor += len(record.encode())
            offsets.append(cursor)
        death = RECORDS[-1].encode()
        with log.open("ab") as output:
            output.write("".join(RECORDS[:-1]).encode() + death[:40])
        peer.snapshot(offsets[-2], 3)
        with log.open("ab") as output:
            output.write(death[40:])
        observed = peer.snapshot(offsets[-1], 4)
        require(observed["source"] == str(log.resolve()), "noncanonical source")
        require(observed["generation"] == baseline["generation"], "spurious generation change")
        require(observed["updated_at"] is not None, "accepted evidence lacks timestamp")
        for index, (kind, fields) in enumerate(FIELDS.items()):
            event = observed["events"][index]
            require(set(event) == fields | {"kind", "offset", "timestamp_local", "client_pid", "tick_ms"},
                    "event escaped its allowlist")
            require(event["kind"] == kind and event["offset"] == offsets[index]
                    and event["timestamp_local"] == PREFIX and event["client_pid"] == 312
                    and event["tick_ms"] == 8147076 + index, "event prefix/end offset mismatch")
        current = observed["current"]
        require(current["area_name"] == "The Riverbank" and current["area_id"] == "G1_1"
                and current["area_level"] == 1 and current["seed"] == 1850020125,
                "area projection mismatch")
        require(current["actors"] == {"ExampleCharacter": {
            "character_class": "Huntress", "character_level": 2, "observed_deaths": 1}},
            "actor projection mismatch")
        connected = status("status-observed")
        require(connected["current"] == current and len(connected["observers"]) == 1,
                "staged CLI status differs from subscribed snapshot")
        require(events("events-observed") == observed["events"], "staged CLI events differ")
        status_text = command("status-text", ["poe2", "companion", "status"], False)
        event_text = command("events-text", ["poe2", "companion", "events"], False)
        require("Last-observed state" in status_text and "The Riverbank" in status_text,
                "human-readable status lacks observed area")
        require("ExampleCharacter" in event_text and "Death:" in event_text,
                "human-readable events missing actor/death")

        observer.send_signal(signal.SIGINT)
        require(observer.wait(timeout=5) == 130, "observer did not stop gracefully")
        disconnected = status("status-disconnected")
        require(disconnected["observers"] == [] and disconnected["current"] == current,
                "disconnect did not retain last-known evidence and clear connection")
        require(events("events-disconnected") == observed["events"], "disconnect changed history")
        persisted = json.loads((Path(env["XDG_CACHE_HOME"]) / "retinue/games/poe2/session.json").read_text())
        require(persisted["events"] == observed["events"] and persisted["current"] == current,
                "isolated daemon persistence mismatch")
        public = json.dumps([observed, connected, disconnected, persisted]) + status_text + event_text
        for forbidden in ("HistoricalActor", "DO_NOT_FORWARD_CHAT", "192.0.2.17"):
            require(forbidden not in public, f"private/historical content leaked: {forbidden}")
        report.update(event_count=4, kinds=list(FIELDS), observer_exit=observer.returncode,
                      checks=["fresh_eof", "cursor_only_not_fresh", "allowlist", "no_chat_or_server",
                              "fragmentation", "subscription", "staged_cli_json_and_text",
                              "actor_projection", "isolated_persistence", "observer_disconnect"])
    finally:
        signal.alarm(0)
        if peer is not None:
            peer.sock.close()
        forced = []
        for name, process in reversed(owned):
            if process.poll() is None:
                try:
                    if name == "observer":
                        process.send_signal(signal.SIGINT)
                    elif name == "daemon":
                        process.stdin.write(b"stop\n")
                        process.stdin.flush()
                    else:
                        process.terminate()
                    process.wait(timeout=5)
                except (OSError, subprocess.TimeoutExpired):
                    forced.append(name)
                    process.kill()
                    process.wait(timeout=5)
            if process.stdin is not None:
                process.stdin.close()
        for handle in handles:
            handle.close()
        report["owned_processes_stopped"] = all(p.poll() is not None for _, p in owned)
        report["owned_process_ids"] = {name: p.pid for name, p in owned}
        report["process_exit_codes"] = {name: p.returncode for name, p in owned}
        report["forced_cleanup"] = forced
        require(not forced, f"cleanup required forced termination: {forced}")
        if "daemon" in report["process_exit_codes"]:
            require(report["process_exit_codes"]["daemon"] == 0, "isolated daemon shutdown failed")
        with socket.socket() as probe:
            probe.settimeout(0.5)
            report["private_epmd_closed"] = probe.connect_ex(("127.0.0.1", port)) != 0
        report["api_socket_removed"] = not socket_path.exists()
        require(report["private_epmd_closed"], "private EPMD endpoint still accepting connections")
        require(report["api_socket_removed"], "isolated API socket not removed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", type=Path,
                        default=Path(__file__).resolve().parents[2] / "prod")
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="rpoe-", dir="/tmp"))
    report = {"result": "failed"}

    def expired(_signum, _frame):
        raise TimeoutError("PoE2 E2E exceeded 60-second deadline")

    def interrupted(_signum, _frame):
        raise InterruptedError("PoE2 E2E interrupted")

    signal.signal(signal.SIGALRM, expired)
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    signal.alarm(60)
    try:
        execute(args.stage.resolve(), root, report)
        report["result"] = "passed"
    except Exception as error:
        report["error"] = str(error)
    finally:
        signal.alarm(0)
        (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"poe2 e2e: {report['result']}; evidence: {root}")
    if report["result"] != "passed":
        print(report["error"])
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
