use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wfinspect-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn capture(
        &self,
        path: &str,
        base: u64,
        permissions: &str,
        name: &str,
        bytes: &[u8],
    ) -> PathBuf {
        let directory = self.0.join(path);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("scaleform-memory.bin"), bytes).unwrap();
        fs::write(
            directory.join("maps.txt"),
            format!(
                "{base:x}-{:x} {permissions} 0 00:00 0 {name}\n",
                base + bytes.len() as u64
            ),
        )
        .unwrap();
        fs::write(directory.join("ui.json"), serde_json::to_vec(&json!({
            "schema": 2, "captured_at_unix_ms": 123,
            "snapshot": { "pid": 42, "movies": [{ "path": "/Lotus/Interface/Old.swf", "record_address": base, "path_address": base, "width": 2560, "height": 1440, "scale_x": 1.0, "scale_y": 1.0 }] },
            "memory": {
                "file": "scaleform-memory.bin", "block_size": 512, "max_depth": 6,
                "max_blocks": 65536, "max_blocks_per_root": 1024, "max_children_per_block": 48,
                "bytes": bytes.len(), "truncated": false, "roots": [],
                "blocks": [{ "address": base, "length": bytes.len(), "file_offset": 0, "depth": 0, "permissions": permissions, "region": name }],
            },
        })).unwrap()).unwrap();
        directory
    }

    fn memory_capture(&self) -> PathBuf {
        use retinue_native::inspect::{
            capture,
            memory::SourceMetadata,
            query::{Scope, Selection, WalkOptions},
        };
        use retinue_native::process::{CaptureBlock, ProcessMemory};
        let directory = self.0.join("generic");
        let mut bytes = vec![0; 128];
        bytes[16..24].copy_from_slice(&0x1040_u64.to_le_bytes());
        bytes[64..71].copy_from_slice(b"GENERIC");
        let memory = ProcessMemory::from_capture(
            42,
            "1000-1080 rw-p 0 00:00 0 [heap]\n".into(),
            vec![CaptureBlock {
                address: 0x1000,
                length: bytes.len(),
                file_offset: 0,
            }],
            bytes,
        )
        .unwrap();
        capture::write(
            &memory,
            SourceMetadata {
                kind: "live".into(),
                pid: 42,
                identity: None,
                capture: None,
            },
            &directory,
            &[],
            &WalkOptions {
                selection: Selection {
                    scope: Scope::Readable,
                    ranges: vec![0x1000..0x1080],
                },
                ..WalkOptions::default()
            },
        )
        .unwrap();
        directory
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str], capture: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_retinue-inspect"));
    command.arg0("retinue-inspect");
    command.args(args);
    command.env("RETINUE_DAEMON_SOCKET", "/absent/daemon.sock");
    if let Some(capture) = capture {
        command.arg("--capture").arg(capture);
    }
    command.output().unwrap()
}

fn document(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn copy_executable(source: &Path, destination: &Path) {
    // Other test threads may fork; do not let them inherit a writable executable FD.
    assert!(
        Command::new("cp")
            .arg(source)
            .arg(destination)
            .status()
            .unwrap()
            .success()
    );
}

fn await_path(path: &Path) {
    for _ in 0..500 {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting for {}", path.display());
}

fn daemon_output(
    args: &[&str],
    serve: impl FnOnce(BufReader<UnixStream>, UnixStream) + Send + 'static,
) -> Output {
    let fixture = Fixture::new();
    let socket = fixture.0.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "inspector did not connect");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        serve(BufReader::new(stream.try_clone().unwrap()), stream);
    });
    let output = Command::new(env!("CARGO_BIN_EXE_retinue-inspect"))
        .arg0("retinue-inspect")
        .args(args)
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .output()
        .unwrap();
    server.join().unwrap();
    output
}

fn receive(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}

fn reply(stream: &mut UnixStream, response: Value) {
    serde_json::to_writer(&mut *stream, &response).unwrap();
    stream.write_all(b"\n").unwrap();
}

fn hello_response(reader: &mut BufReader<UnixStream>, game: &str) -> Value {
    let hello = receive(reader);
    assert_eq!(hello["op"], "hello");
    assert_eq!(hello["client"], "retinue-inspect");
    assert_eq!(hello["envelope"], 2);
    assert_eq!(hello["game"], game);
    let interfaces = match game {
        "warframe" => json!(retinue_client::local_protocol::interfaces()),
        "poe2" => json!({"session": 1}),
        _ => panic!("unexpected game"),
    };
    assert_eq!(hello["interfaces"], interfaces);
    json!({
        "id": 1, "ok": true, "compatible": true,
        "envelope": 2, "game": game, "interfaces": interfaces,
    })
}

#[test]
fn warframe_memory_source_requires_warframe_identity() {
    let error = retinue_warframe::inspect::memory::open_memory(
        retinue_warframe::inspect::memory::Source::Live(std::process::id()),
    )
    .unwrap_err();
    assert_eq!(error, "Warframe executable is absent from process maps");
}

#[test]
fn generic_memory_requires_explicit_source_and_accepts_non_game_pid() {
    let missing = run(&["memory", "maps"], None);
    assert!(!missing.status.success());
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("require --pid PID or --capture PATH")
    );
    // exec preserves the shell PID, so the inspector reads only its own process.
    let output = Command::new("sh")
        .args(["-c", "exec \"$1\" memory maps --pid \"$$\"", "sh"])
        .arg(env!("CARGO_BIN_EXE_retinue-inspect"))
        .env("RETINUE_DAEMON_SOCKET", "/absent/daemon.sock")
        .output()
        .unwrap();
    let maps = document(output);
    assert_eq!(maps["source"]["kind"], "live");
    assert!(!maps["regions"].as_array().unwrap().is_empty());
    assert!(
        maps["source"]["identity"]["executable"]["path"]
            .as_str()
            .unwrap()
            .ends_with("retinue-inspect")
    );
}

#[test]
fn generic_capture_supports_queries_capture_and_recording_offline() {
    let fixture = Fixture::new();
    let capture = fixture.memory_capture();
    let maps = document(run(&["memory", "maps"], Some(&capture)));
    assert!(maps["source"].get("identity").is_none());
    let read = run(&["memory", "read", "0x1040", "7"], Some(&capture));
    assert!(read.status.success());
    assert_eq!(read.stdout, b"GENERIC");
    for command in [
        ["memory", "find", "GENERIC"],
        ["memory", "scan", "47454e45524943"],
    ] {
        let found = document(run(&command, Some(&capture)));
        assert_eq!(found["matches"][0]["address"], 0x1040);
    }
    let refs = document(run(&["memory", "refs", "0x1040"], Some(&capture)));
    assert_eq!(refs["matches"][0]["address"], 0x1010);
    let path = document(run(
        &["memory", "path", "0x1000", "0x1040", "--block-size", "64"],
        Some(&capture),
    ));
    assert_eq!(path["path"]["hops"].as_array().unwrap().len(), 1);
    let copy = fixture.0.join("copied");
    let report = document(run(
        &[
            "memory",
            "capture",
            copy.to_str().unwrap(),
            "--range",
            "0x1000:128",
            "--scope",
            "readable",
        ],
        Some(&capture),
    ));
    assert_eq!(report["bytes"], 128);
    assert_eq!(
        run(&["memory", "read", "0x1040", "7"], Some(&copy)).stdout,
        b"GENERIC"
    );
    let recording = fixture.0.join("recording.ndjson");
    let summary = document(run(
        &[
            "memory",
            "watch",
            "--range",
            "0x1040:7",
            recording.to_str().unwrap(),
            "--seconds",
            "0.02",
            "--interval-ms",
            "10",
        ],
        Some(&capture),
    ));
    assert!(summary["samples"].as_u64().unwrap() > 0);
    let replay = document(run(
        &[
            "memory",
            "replay",
            recording.to_str().unwrap(),
            "--sample",
            "0",
        ],
        None,
    ));
    assert_eq!(
        replay["sample"]["data"]["blocks"][0]["hex"],
        "47454e45524943"
    );
}

#[test]
fn native_report_is_game_independent_and_warframe_adds_adapter_metadata() {
    let fixture = Fixture::new();
    let path = fixture.0.join("native.json");
    let sha = "d01b5cb5cff51afc5ffb7d3af051674aafa84000bee764780ff71d9d073cad93";
    fs::write(&path, serde_json::to_vec(&json!({
        "schema": "wfinspect.ghidra-report", "schema_version": 1,
        "producer": { "name": "ExportWfinspectReport", "version": 1 },
        "program": {
            "name": "program", "path": "/test/program", "format": "ELF",
            "sha256": sha, "image_base": "400000", "language": "x86:LE:64:default", "compiler": "gcc",
        },
        "queries": [{ "kind": "string", "input": "generic", "matches": [] }]
    })).unwrap()).unwrap();
    let file = path.to_str().unwrap();
    let report = document(run(
        &["native", "report", "verify", file, "--sha256", sha],
        None,
    ));
    assert_eq!(report["program"]["name"], "program");
    assert!(report.get("adapter").is_none());
    assert_eq!(
        document(run(&["native", "report", "list", file], None))["queries"][0]["kind"],
        "string"
    );
    assert_eq!(
        document(run(&["native", "report", "get", file, "0"], None))["query"]["input"],
        "generic"
    );
    assert!(
        !run(
            &[
                "native",
                "report",
                "verify",
                file,
                "--sha256",
                "d01b5cb5cff5"
            ],
            None
        )
        .status
        .success()
    );
    let report = document(run(
        &[
            "warframe",
            "report",
            "verify",
            file,
            "--adapter",
            "d01b5cb5cff5",
        ],
        None,
    ));
    assert_eq!(report["adapter"]["status"], "supported");
}

#[test]
fn helper_prefix_survives_directory_exchange() {
    if let Some(root) = std::env::var_os("WFCLI_TEST_MOVED_PREFIX") {
        let root = PathBuf::from(root);
        let initial = retinue_client::executable_path().unwrap().to_owned();
        fs::write(root.join("ready"), "").unwrap();
        await_path(&root.join("continue"));
        assert_ne!(std::env::current_exe().unwrap(), initial);
        assert_eq!(retinue_client::executable_path().unwrap(), initial);
        assert!(initial.parent().unwrap().join("helper").is_file());
        return;
    }
    let fixture = Fixture::new();
    let prefix = fixture.0.join("prefix");
    fs::create_dir_all(prefix.join("bin")).unwrap();
    let executable = prefix.join("bin/test");
    copy_executable(&std::env::current_exe().unwrap(), &executable);
    let mut child = Command::new(&executable)
        .args([
            "--exact",
            "helper_prefix_survives_directory_exchange",
            "--nocapture",
        ])
        .env("WFCLI_TEST_MOVED_PREFIX", &fixture.0)
        .spawn()
        .unwrap();
    await_path(&fixture.0.join("ready"));
    fs::rename(&prefix, fixture.0.join("retired")).unwrap();
    fs::create_dir_all(prefix.join("bin")).unwrap();
    fs::write(prefix.join("bin/helper"), "replacement").unwrap();
    fs::write(fixture.0.join("continue"), "").unwrap();
    assert!(child.wait().unwrap().success());
}

#[test]
fn bundled_decoder_is_relocatable_and_explicit_overrides_win() {
    let fixture = Fixture::new();
    let bin = fixture.0.join("bin");
    let libexec = fixture.0.join("libexec");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(&libexec).unwrap();
    let executable = bin.join("retinue-inspect");
    copy_executable(
        Path::new(env!("CARGO_BIN_EXE_retinue-inspect")),
        &executable,
    );
    let helper = libexec.join("unoodle");
    fs::write(&helper, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();

    let mut command = Command::new(&executable);
    command
        .args(["warframe", "cache", "decoder"])
        .env("PATH", &bin)
        .env_remove("WFINSPECT_OODLE_LIBRARY")
        .env_remove("WFINSPECT_OODLE_COMMAND");
    let report = document(command.output().unwrap());
    assert_eq!(report["available"], true);
    assert_eq!(report["source"], helper.to_str().unwrap());

    command.env("WFINSPECT_OODLE_COMMAND", fixture.0.join("missing"));
    let report = document(command.output().unwrap());
    assert_eq!(report["available"], false);
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("WFINSPECT_OODLE_COMMAND")
    );

    command.env_remove("WFINSPECT_OODLE_COMMAND");
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o644)).unwrap();
    let report = document(command.output().unwrap());
    assert_eq!(report["available"], false);
    assert!(report["error"].as_str().unwrap().contains("not executable"));
}

#[test]
fn cli_help_validation_completion_and_literals() {
    let root_help = run(&["--help"], None);
    assert!(root_help.status.success());
    let root_help = String::from_utf8_lossy(&root_help.stdout);
    assert!(root_help.contains("retinue-inspect"));
    assert!(root_help.contains("warframe"));
    assert_eq!(
        document(run(&["game", "adapter", "--list"], None)),
        document(run(&["warframe", "adapter", "--list"], None)),
    );
    let help = run(
        &["warframe", "cache", "paths", "/absent", "Font", "--help"],
        None,
    );
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage:"));
    for args in [
        vec!["warframe", "memory", "scan", "a\u{20ac}"],
        vec!["warframe", "ui", "find", "--mistyped"],
    ] {
        let output = run(&args, None);
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
    let completion = run(&["completion", "bash"], None);
    assert!(completion.status.success());
    assert!(String::from_utf8_lossy(&completion.stdout).contains("retinue-inspect"));
    assert!(
        document(run(&["warframe", "adapter", "--list"], None))
            .as_array()
            .unwrap()
            .len()
            > 0
    );
    let fixture = Fixture::new();
    let path = fixture.capture("literal", 0x1000, "rw-p", "[heap]", b"literal help text\0");
    let output = run(
        &[
            "warframe",
            "ui",
            "find",
            "--capture",
            path.to_str().unwrap(),
            "--",
            "help",
        ],
        None,
    );
    assert_eq!(document(output)["text"]["terms"][0]["term"], "help");
}

#[test]
fn pointers_use_image_roots_and_report_exhausted_depth() {
    let fixture = Fixture::new();
    let mut bytes = vec![0; 8192];
    for index in 0..20 {
        bytes[index * 256..index * 256 + 8]
            .copy_from_slice(&(0x1000 + (index as u64 + 1) * 256).to_le_bytes());
    }
    let path = fixture.capture("chain", 0x1000, "r--p", "/game/Warframe.x64.exe", &bytes);
    let short = document(run(
        &[
            "warframe",
            "memory",
            "path",
            "0x1000",
            "0x1100",
            "--block-size",
            "8",
        ],
        Some(&path),
    ));
    assert_eq!(short["path"]["hops"].as_array().unwrap().len(), 1);
    let limited = document(run(
        &[
            "warframe",
            "memory",
            "path",
            "0x1000",
            "0x2300",
            "--block-size",
            "8",
        ],
        Some(&path),
    ));
    assert_eq!(limited["path"]["coverage"]["depth_limited"], true);
    assert_eq!(limited["path"]["truncated"], true);
    let found = document(run(
        &[
            "warframe",
            "memory",
            "path",
            "0x1000",
            "0x2300",
            "--depth",
            "24",
            "--block-size",
            "8",
        ],
        Some(&path),
    ));
    assert_eq!(found["path"]["hops"].as_array().unwrap().len(), 19);
    assert!(
        !run(
            &["warframe", "memory", "path", "0xffff", "0x1100"],
            Some(&path)
        )
        .status
        .success()
    );
}

#[test]
fn scan_scope_and_coverage_are_explicit() {
    let fixture = Fixture::new();
    let path = fixture.capture(
        "mapped",
        0x1000,
        "r--p",
        "/game/resource.cache",
        b"xxmarkerxx",
    );
    let default = document(run(&["warframe", "memory", "find", "marker"], Some(&path)));
    assert_eq!(default["searched_bytes"], 0);
    let readable = document(run(
        &[
            "warframe", "memory", "find", "marker", "--scope", "readable",
        ],
        Some(&path),
    ));
    assert_eq!(readable["matches"][0]["address"], 0x1002);
    let raw = run(&["warframe", "memory", "read", "0x1002", "6"], Some(&path));
    assert!(raw.status.success());
    assert_eq!(raw.stdout, b"marker");
    let bulk = fixture.capture("bulk", 0x1000, "rw-p", "[heap]", &vec![0; 8 * 1024 * 1024]);
    let scanned = document(run(&["warframe", "memory", "scan", "00"], Some(&bulk)));
    assert_eq!(scanned["searched_bytes"], 4096);
    assert_eq!(scanned["candidate_bytes"], 8 * 1024 * 1024);
    assert_eq!(scanned["coverage"]["match_limited"], true);
}

#[test]
fn selected_capture_roundtrips_and_never_overwrites() {
    let fixture = Fixture::new();
    let source = fixture.capture("source", 0x1000, "rw-p", "[heap]", b"0123456789abcdef");
    let output = fixture.0.join("selected");
    let args = [
        "warframe",
        "memory",
        "capture",
        output.to_str().unwrap(),
        "--range",
        "0x1004:8",
    ];
    let captured = document(run(&args, Some(&source)));
    assert_eq!(captured["bytes"], 8);
    assert_eq!(
        run(
            &["warframe", "memory", "read", "0x1004", "8"],
            Some(&output)
        )
        .stdout,
        b"456789ab"
    );
    assert!(!run(&args, Some(&source)).status.success());
}

#[test]
fn movies_recompute_instead_of_reusing_saved_snapshot() {
    let fixture = Fixture::new();
    let path = fixture.capture("old", 0x1000, "rw-p", "[heap]", &vec![0; 512]);
    let report = document(run(&["warframe", "ui", "movies"], Some(&path)));
    assert!(report["snapshot"]["movies"].as_array().unwrap().is_empty());
    assert!(report["metrics"].is_object());
}

#[test]
fn cache_search_retains_matches_before_error_and_streams_them() {
    let fixture = Fixture::new();
    let mut toc = vec![0; 8];
    let data = b"xxneedle-needle";
    for (offset, size, name) in [
        (0_i64, data.len() as i32, "First.bin"),
        (data.len() as i64, 100, "Broken.bin"),
    ] {
        let mut entry = [0; 96];
        entry[..8].copy_from_slice(&offset.to_le_bytes());
        entry[16..20].copy_from_slice(&size.to_le_bytes());
        entry[20..24].copy_from_slice(&size.to_le_bytes());
        entry[32..32 + name.len()].copy_from_slice(name.as_bytes());
        toc.extend_from_slice(&entry);
    }
    fs::write(fixture.0.join("H.Probe.toc"), toc).unwrap();
    fs::write(fixture.0.join("H.Probe.cache"), data).unwrap();
    let output = run(
        &[
            "warframe",
            "cache",
            "find",
            fixture.0.to_str().unwrap(),
            "Probe",
            "needle",
            "--stream",
        ],
        None,
    );
    assert!(!output.status.success());
    let records: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records[1]["data"]["offset"], 2);
    assert_eq!(records[2]["data"]["offset"], 9);
    assert_eq!(records[3]["data"]["kind"], "error");
    assert_eq!(records.last().unwrap()["data"]["errors"], 1);
    assert_eq!(records.last().unwrap()["data"]["matches"], 2);
}

#[test]
fn rejected_daemon_request_exits_nonzero_and_preserves_reply() {
    let output = daemon_output(&["daemon", "get", "invalid"], |mut reader, mut stream| {
        reply(&mut stream, hello_response(&mut reader, "warframe"));
        let request = receive(&mut reader);
        assert_eq!(request["op"], "get");
        assert_eq!(request["game"], "warframe");
        reply(
            &mut stream,
            json!({"id": 2, "ok": false, "error": "unsupported_dataset"}),
        );
    });
    assert!(!output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["response"]["error"],
        "unsupported_dataset"
    );
}

#[test]
fn poe2_daemon_get_and_subscribe_use_session_namespace() {
    let report = document(daemon_output(
        &["daemon", "--game", "poe2", "get", "session"],
        |mut reader, mut stream| {
            reply(&mut stream, hello_response(&mut reader, "poe2"));
            assert_eq!(
                receive(&mut reader),
                json!({
                    "op": "get", "id": 2, "game": "poe2", "dataset": "session",
                })
            );
            std::thread::sleep(Duration::from_millis(10));
            reply(
                &mut stream,
                json!({
                    "id": 2, "ok": true, "dataset": "session", "data": {"revision": 7},
                }),
            );
        },
    ));
    assert_eq!(report["handshake"]["game"], "poe2");
    assert_eq!(
        report["handshake"]["required_interfaces"],
        json!({"session": 1})
    );
    assert_eq!(report["response"]["data"]["revision"], 7);

    let output = daemon_output(
        &[
            "daemon",
            "subscribe",
            "session",
            "--game",
            "poe2",
            "--seconds",
            "1",
            "--limit",
            "1",
        ],
        |mut reader, mut stream| {
            reply(&mut stream, hello_response(&mut reader, "poe2"));
            let request = receive(&mut reader);
            assert_eq!(request["op"], "subscribe");
            assert_eq!(request["game"], "poe2");
            assert_eq!(request["dataset"], "session");
            reply(
                &mut stream,
                json!({"id": 2, "ok": true, "data": {"revision": 7}}),
            );
            std::thread::sleep(Duration::from_millis(10));
            reply(
                &mut stream,
                json!({
                    "event": "session", "game": "poe2", "subscription": 2, "data": {"revision": 8},
                }),
            );
            assert_eq!(
                receive(&mut reader),
                json!({
                    "op": "unsubscribe", "id": 3, "game": "poe2", "subscription": 2,
                })
            );
        },
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["data"]["kind"], "initial");
    assert_eq!(records[0]["data"]["message"]["data"]["revision"], 7);
    assert_eq!(records[1]["data"]["kind"], "update");
    assert_eq!(records[1]["data"]["message"]["game"], "poe2");
    assert_eq!(records[1]["data"]["message"]["data"]["revision"], 8);
    assert_eq!(records[2]["record"], "summary");
    assert_eq!(records[2]["data"]["handshake"]["game"], "poe2");
    assert_eq!(records[2]["data"]["events"], 1);
}

#[test]
fn daemon_handshake_reports_selected_game_and_old_envelope_stops_requests() {
    for game in ["warframe", "poe2"] {
        let report = document(daemon_output(
            &["daemon", "handshake", "--game", game],
            move |mut reader, mut stream| {
                reply(&mut stream, hello_response(&mut reader, game));
            },
        ));
        assert_eq!(report["game"], game);
        assert_eq!(report["response"]["game"], game);
    }
    let output = daemon_output(
        &["daemon", "--game", "poe2", "get", "session"],
        |mut reader, mut stream| {
            let mut response = hello_response(&mut reader, "poe2");
            response["envelope"] = json!(1);
            reply(&mut stream, response);
            let mut line = String::new();
            assert_eq!(reader.read_line(&mut line).unwrap(), 0);
        },
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("daemon contract mismatch"));
    assert!(output.stdout.is_empty());
}
