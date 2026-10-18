#!/usr/bin/env python3
"""Staged PoE2 native session API/CLI smoke test using synthetic wire samples.

Run: python3 scripts/tests/poe2.py [--stage prod]
No games, memory reads, builds, or live daemon access. Native reader/transport tests
run separately. Evidence stays in /tmp. All owned processes are stopped.
"""

import argparse
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import time


def sample(sequence, status="observed"):
    process = {"pid": 42, "started": 1000, "executable": "/fixture/PathOfExile_x64.exe",
               "image_base": 0x140000000}
    snapshot = {
        "scene": "in_game", "states": ["InGameState"],
        "player": {"name": "ExampleCharacter", "class": "Huntress", "level": 2,
                   "experience": 100, "gold": 42, "life": 100},
        "area": {"name": "The Riverbank", "in_town": False, "change_key": 7},
        "panels": {"inventory": False, "stash": None}, "issues": [],
    }
    return {"stream": "native-fixture", "sequence": sequence,
            "observed_at": 1791678000000 + sequence * 30000, "status": status,
            "process": None if status == "waiting" else process,
            "snapshot": snapshot if status == "observed" else None,
            "error": "layout unresolved" if status == "error" else None}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def only(paths, description):
    paths = list(paths)
    require(len(paths) == 1, f"expected exactly one {description}: {paths}")
    return paths[0]


def await_ready(predicate, process, description, timeout=8):
    deadline = time.monotonic() + timeout
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

    def hello(self, client, **extra):
        self.send({"id": 1, "op": "hello", "game": "poe2", "client": client,
                   "envelope": 2, "interfaces": {"session": 2}, "features": [], **extra})
        reply = self.receive()
        require(reply.get("id") == 1 and reply.get("ok") is True
                and reply.get("compatible") is True and reply.get("game") == "poe2"
                and reply.get("envelope") == 2
                and reply.get("interfaces") == {"session": 2, "trade": 2, "checks": 1},
                f"real daemon contract mismatch: {reply}")

    def snapshot(self, sequence):
        value = self.receive()
        require(value.get("event") == "session", "expected subscribed session event")
        require(value.get("game") == "poe2" and value.get("subscription") == 2,
                "subscription lost game/ID scope")
        require(value["data"]["sequence"] == sequence, "unexpected sample publication")
        return value["data"]

    def silence(self):
        require(not self.buffer, "unexpected buffered session notification")
        self.sock.settimeout(0.15)
        try:
            self.sock.recv(8192)
            raise RuntimeError("retry/heartbeat notified or disconnected subscriber")
        except socket.timeout:
            pass
        finally:
            self.sock.settimeout(8)


def execute(stage, root, report):
    release = stage / "libexec/retinue_daemon"
    erts = only(release.glob("erts-*/bin"), "staged Erlang runtime")
    boot = only(release.glob("releases/*/start_clean.boot"), "clean boot file")
    ebins = sorted((release / "lib").glob("*/ebin"))
    for app in ("retinue_core", "wfcore", "retinue_daemon"):
        only((release / "lib").glob(f"{app}-*/ebin"), f"staged {app}")
    cli = stage / "bin/retinue"
    for executable in (cli, erts / "erl", erts / "epmd"):
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
    peer = publisher = None

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
        await_ready(ready_path.exists, daemon, "isolated production application", timeout=30)
        wire = (root / "subscription.jsonl").open("w")
        handles.append(wire)
        peer = Peer(socket_path, wire)
        peer.hello("poe2-e2e")
        peer.send({"id": 2, "op": "subscribe", "dataset": "session"})
        subscribed = peer.receive()
        require(subscribed.get("id") == 2 and subscribed.get("ok") is True,
                "session subscription rejected")
        require(subscribed["data"]["schema"] == 2 and subscribed["data"]["events"] == [],
                "isolated daemon has unexpected schema/history")
        publisher = Peer(socket_path, wire)
        publisher.hello("poe2companion", mode="watch", pid=os.getpid())
        report["synthetic_publisher"] = True

        def publish(data, unchanged=False):
            request_id = data["sequence"] + 10
            publisher.send({"id": request_id, "op": "publish", "dataset": "session", "data": data})
            reply = publisher.receive()
            require(reply.get("id") == request_id and reply.get("ok") is True,
                    f"sample rejected: {reply}")
            if unchanged:
                require(reply["data"].get("unchanged") is True, "unchanged sample changed revision")
                peer.silence()
                return reply["data"]
            return peer.snapshot(data["sequence"])

        waiting = publish(sample(1, "waiting"))
        require(waiting["current"] is None and waiting["process"] is None
                and waiting["events"] == [], "waiting fabricated native state")
        observed_sample = sample(2)
        observed_sample["raw"] = "DO_NOT_FORWARD"
        observed_sample["snapshot"]["player"]["raw"] = "DO_NOT_FORWARD"
        observed = publish(observed_sample)
        require(observed["current"] == sample(2)["snapshot"], "native projection/allowlist mismatch")
        require([event["kind"] for event in observed["events"]] ==
                ["state", "area", "character", "xp", "level", "gold", "panel"],
                "initial native events mismatch")
        connected = status("status-initial")
        require(len(connected["observers"]) == 1 and connected["observers"][0]["os_pid"] == os.getpid()
                and connected["observers"][0]["mode"] == "watch", "wrong fixture publisher connection")

        partial = sample(3)
        partial["snapshot"]["player"].update(gold=None, life=None)
        partial["snapshot"].update(area=None, panels={"inventory": None},
                                   issues=[{"field": "life", "reason": "unreadable component"}])
        incomplete = publish(partial)
        require(incomplete["current"] == partial["snapshot"]
                and incomplete["events"] == observed["events"], "unknown read fabricated events")
        zero = sample(4)
        zero["snapshot"]["player"]["life"] = 0
        recovered = publish(zero)
        require(not any(event["kind"] == "death" for event in recovered["events"]),
                "unknown-to-zero life fabricated death")
        publish(sample(5))
        dead = sample(6)
        dead["snapshot"]["player"].update(life=0, experience=200, level=3, gold=99)
        dead["snapshot"]["panels"].update(inventory=True, stash=False)
        observed = publish(dead)
        deaths = [event for event in observed["events"] if event["kind"] == "death"]
        require(len(deaths) == 1 and deaths[0]["from"] == 100 and deaths[0]["to"] == 0
                and deaths[0]["character"] == "ExampleCharacter", "death transition mismatch")
        for event in observed["events"]:
            require({"kind", "stream", "sequence", "observed_at", "process", "from", "to"} <= set(event),
                    "normalized event lacks native identity")
            require(event["process"] == {"pid": 42, "started": 1000}, "event process identity mismatch")
        cache = Path(env["XDG_CACHE_HOME"]) / "retinue/games/poe2/session.json"
        cache_before = (cache.read_bytes(), cache.stat().st_mtime_ns, cache.stat().st_ino)
        ack = publish(dead, unchanged=True)
        require(ack["revision"] == observed["revision"], "retry changed revision")
        heartbeat = dict(dead, sequence=7, observed_at=dead["observed_at"] + 30000)
        ack = publish(heartbeat, unchanged=True)
        require(ack["revision"] == observed["revision"], "heartbeat changed revision")
        require(cache_before == (cache.read_bytes(), cache.stat().st_mtime_ns, cache.stat().st_ino),
                "retry/heartbeat rewrote durable projection")
        refreshed = status("status-heartbeat")
        require(refreshed["sequence"] == 7 and refreshed["observed_at"] == heartbeat["observed_at"]
                and refreshed["updated_at"] == observed["updated_at"], "heartbeat read time mismatch")
        failed = publish(sample(8, "error"))
        require(failed["current"] is None and failed["error"] == "layout unresolved"
                and failed["events"] == observed["events"], "failed read fabricated state")
        zero = sample(9)
        zero["snapshot"]["player"]["life"] = 0
        publish(zero)
        replaced = sample(10)
        replaced["process"]["started"] += 1
        replaced["snapshot"]["player"]["life"] = 0
        observed = publish(replaced)
        require(len([event for event in observed["events"] if event["kind"] == "death"]) == 1,
                "failed read or process replacement fabricated death")
        current = observed["current"]
        connected = status("status-observed")
        require(connected["current"] == current and len(connected["observers"]) == 1,
                "staged CLI status differs from subscribed snapshot")
        require(events("events-observed") == observed["events"], "staged CLI events differ")
        status_text = command("status-text", ["--utc", "poe2", "companion", "status"], False)
        event_text = command("events-text", ["--utc", "poe2", "companion", "events"], False)
        require(all(part in status_text for part in ("PoE2 native session", "The Riverbank",
                "ExampleCharacter", "Huntress", "level: 2", "XP: 100", "gold: 42", "inventory: closed")),
                "human-readable status lacks native details")
        require("ExampleCharacter" in event_text and "Death:" in event_text
                and "2026-" in event_text and ".000Z" in event_text,
                "human-readable events missing native details/UTC timestamps")

        publisher.sock.close()
        publisher = None
        disconnected = status("status-disconnected")
        require(disconnected["observers"] == [] and disconnected["current"] == current,
                "disconnect did not retain evidence and clear connection")
        persisted = json.loads(cache.read_bytes())
        require(persisted["schema"] == 2 and persisted["events"] == observed["events"]
                and persisted["current"] == current, "isolated persistence mismatch")
        public = json.dumps([observed, connected, disconnected, persisted]) + status_text + event_text
        require("DO_NOT_FORWARD" not in public, "unknown native fields leaked")
        report.update(event_count=len(observed["events"]), death_count=1,
                      checks=["native_schema", "waiting", "partial_reads", "no_false_death", "allowlist",
                              "retry_dedup", "quiet_heartbeat", "heartbeat_read_time",
                              "process_replacement", "subscription", "staged_cli_json_and_text",
                              "isolated_persistence", "publisher_disconnect"])
    finally:
        failed = sys.exc_info()[0] is not None
        signal.alarm(0)
        if peer is not None:
            peer.sock.close()
        if publisher is not None:
            publisher.sock.close()
        forced = []
        for name, process in reversed(owned):
            if process.poll() is None:
                try:
                    if name == "daemon":
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
        with socket.socket() as probe:
            probe.settimeout(0.5)
            report["private_epmd_closed"] = probe.connect_ex(("127.0.0.1", port)) != 0
        report["api_socket_removed"] = not socket_path.exists()
        if not failed:
            require(not forced, f"cleanup required forced termination: {forced}")
            if "daemon" in report["process_exit_codes"]:
                require(report["process_exit_codes"]["daemon"] == 0, "isolated daemon shutdown failed")
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
