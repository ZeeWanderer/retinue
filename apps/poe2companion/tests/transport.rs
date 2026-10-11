use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use poe2companion::{Batch, Connection};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

const DEATH: &str =
    "2026/05/30 19:16:35 8147076 3ef23347 [INFO Client 312] : ExampleCharacter has been slain.\r\n";

async fn read(reader: &mut BufReader<OwnedReadHalf>) -> Value {
    let mut line = String::new();
    let size = tokio::time::timeout(Duration::from_secs(4), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(size > 0);
    serde_json::from_str(&line).unwrap()
}

async fn write(writer: &mut OwnedWriteHalf, value: Value) {
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
}

async fn accept(listener: &UnixListener) -> (BufReader<OwnedReadHalf>, OwnedWriteHalf) {
    let (stream, _) = tokio::time::timeout(Duration::from_secs(4), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let hello = read(&mut reader).await;
    assert_eq!(hello["op"], "hello");
    assert_eq!(hello["id"], 1);
    assert_eq!(hello["client"], "poe2companion");
    assert_eq!(hello["game"], "poe2");
    assert_eq!(hello["interfaces"], json!({"session":1}));
    assert_eq!(hello["envelope"], 2);
    write(&mut writer, json!({"id":1,"ok":true,"compatible":true,"game":"poe2","envelope":2,"interfaces":{"session":1}})).await;
    (reader, writer)
}

async fn acknowledge(writer: &mut OwnedWriteHalf, id: &Value) {
    write(
        writer,
        json!({"id":id,"ok":true,"dataset":"session","data":{"revision":1}}),
    )
    .await;
}

fn append(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

#[tokio::test]
async fn reconnect_retries_identical_batch_without_consuming_a_second_batch() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("Client.txt");
    fs::write(&log, DEATH).unwrap();
    let socket = dir.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["watch", "--log"])
        .arg(&log)
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .env("RETINUE_COMMAND", "/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let (mut reader, mut writer) = accept(&listener).await;
    let initial = read(&mut reader).await;
    assert_eq!(initial["data"]["events"], json!([]));
    assert_eq!(initial["data"]["cursor"], DEATH.len());
    assert!(initial["data"].get("reset").is_none());
    acknowledge(&mut writer, &initial["id"]).await;

    append(&log, DEATH.as_bytes());
    let pending = read(&mut reader).await;
    assert_eq!(pending["op"], "publish");
    assert_eq!(pending["dataset"], "session");
    assert_eq!(
        pending["data"]["source"],
        fs::canonicalize(&log).unwrap().to_str().unwrap()
    );
    assert_eq!(pending["data"]["events"][0]["kind"], "death");
    assert_eq!(pending["data"]["events"][0]["client_pid"], 312);
    assert_eq!(pending["data"]["events"][0]["tick_ms"], 8147076);
    assert_eq!(pending["data"]["events"][0]["offset"], DEATH.len() * 2);
    append(&log, DEATH.as_bytes());
    let mut line = String::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
            .await
            .is_err()
    );
    drop(reader);
    drop(writer);

    let (mut reader, mut writer) = accept(&listener).await;
    let retry = read(&mut reader).await;
    assert_eq!(retry, pending);
    acknowledge(&mut writer, &retry["id"]).await;
    let next = read(&mut reader).await;
    assert_ne!(next["id"], retry["id"]);
    assert_eq!(next["data"]["events"].as_array().unwrap().len(), 1);
    assert_eq!(next["data"]["events"][0]["offset"], DEATH.len() * 3);
    assert_eq!(next["data"]["generation"], retry["data"]["generation"]);
    acknowledge(&mut writer, &next["id"]).await;
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap(),
        rustix::process::Signal::INT,
    )
    .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(4), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(130));
}

#[tokio::test]
async fn ensure_gate_recovers_across_two_outages() {
    exercise_ensure_recovery(false).await;
}

#[tokio::test]
async fn ensure_gate_retries_a_failed_helper() {
    exercise_ensure_recovery(true).await;
}

async fn exercise_ensure_recovery(fail_first: bool) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("Client.txt");
    fs::write(&log, b"").unwrap();
    let helper = dir.path().join("retinue");
    let count = dir.path().join("ensure-count");
    fs::write(
        &helper,
        concat!(
            "#!/bin/sh\n",
            "printf x >> \"$ENSURE_COUNT\"\n",
            "if [ \"$FAIL_FIRST_ENSURE\" = 1 ] && [ ! -e \"$ENSURE_COUNT.failed\" ]; then\n",
            "  : > \"$ENSURE_COUNT.failed\"\n",
            "  exit 17\n",
            "fi\n",
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["watch", "--log"])
        .arg(&log)
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .env("RETINUE_COMMAND", &helper)
        .env("ENSURE_COUNT", &count)
        .env("FAIL_FIRST_ENSURE", if fail_first { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let (mut reader, mut writer) = accept(&listener).await;
    let initial = read(&mut reader).await;
    acknowledge(&mut writer, &initial["id"]).await;
    let mut expected_ensures = 0;
    for epoch in 1..=2 {
        append(&log, DEATH.as_bytes());
        let pending = read(&mut reader).await;
        assert_eq!(pending["data"]["events"].as_array().unwrap().len(), 1);
        assert_eq!(pending["data"]["cursor"], DEATH.len() * epoch);
        let attempts = if fail_first && epoch == 1 { 2 } else { 1 };
        for _ in 0..attempts {
            drop(reader);
            drop(writer);
            (reader, writer) = accept(&listener).await;
            let retry = read(&mut reader).await;
            assert_eq!(retry, pending, "retry must retain request ID and payload");
            expected_ensures += 1;
            assert_eq!(fs::read(&count).unwrap(), vec![b'x'; expected_ensures]);
        }
        acknowledge(&mut writer, &pending["id"]).await;
    }
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap(),
        rustix::process::Signal::INT,
    )
    .unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(4), child.wait())
            .await
            .unwrap()
            .unwrap()
            .code(),
        Some(130)
    );
}

#[tokio::test]
async fn mismatched_acknowledgement_is_not_success() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accept(&listener).await;
        let request = read(&mut reader).await;
        assert_eq!(request["id"], 8);
        acknowledge(&mut writer, &json!(9)).await;
    });
    let mut connection = Connection::connect(&socket, "watch").await.unwrap();
    let batch = Batch {
        source: "/tmp/test/Client.txt".into(),
        generation: "1:2:0".into(),
        cursor: 5,
        events: vec![],
    };
    let error = connection.publish(8, &batch).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    server.await.unwrap();
}

#[tokio::test]
async fn incompatible_hello_and_oversized_frames_fail_closed() {
    for failure in ["interface", "missing_game", "oversized"] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            read(&mut BufReader::new(reader)).await;
            if failure == "oversized" {
                let _ = writer.write_all(&vec![b'x'; 130 * 1024]).await;
            } else {
                let mut reply = json!({"id":1,"ok":true,"compatible":true,"game":"poe2","envelope":2,"interfaces":{"session":1}});
                if failure == "interface" {
                    reply["interfaces"]["session"] = json!(2);
                } else {
                    reply.as_object_mut().unwrap().remove("game");
                }
                write(&mut writer, reply).await;
            }
        });
        let result = Connection::connect(&socket, "watch").await;
        let error = result.err().expect("invalid handshake accepted");
        assert_eq!(
            error.kind(),
            if failure == "oversized" {
                std::io::ErrorKind::InvalidData
            } else {
                std::io::ErrorKind::Unsupported
            }
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn launch_preserves_child_status_and_drains_final_records() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("Client.txt");
    fs::write(&log, b"").unwrap();
    let socket = dir.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let line = DEATH.to_owned();
    let log_for_server = log.clone();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = accept(&listener).await;
        let initial = read(&mut reader).await;
        acknowledge(&mut writer, &initial["id"]).await;
        append(&log_for_server, line.as_bytes());
        let final_batch = read(&mut reader).await;
        assert_eq!(final_batch["data"]["events"].as_array().unwrap().len(), 1);
        acknowledge(&mut writer, &final_batch["id"]).await;
    });
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["launch", "--log"])
        .arg(&log)
        .args(["--", "/bin/sh", "-c", "sleep 0.2; exit 7"])
        .env("RETINUE_DAEMON_SOCKET", &socket)
        .env("RETINUE_COMMAND", "/bin/true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.code(), Some(7));
    server.await.unwrap();
}

#[tokio::test]
async fn startup_failure_launches_child_and_preserves_exit_status() {
    for exit_code in [7, 19] {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("missing/logs/Client.txt");
        let marker = dir.path().join("child-ran");
        let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
            .args(["launch", "--log"])
            .arg(&log)
            .args([
                "--",
                "/bin/sh",
                "-c",
                "sleep 0.05; printf started > \"$1\"; exit \"$2\"",
                "test-child",
            ])
            .arg(&marker)
            .arg(exit_code.to_string())
            .env("RETINUE_DAEMON_SOCKET", dir.path().join("unused.sock"))
            .env("RETINUE_COMMAND", "/bin/false")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let output = tokio::time::timeout(Duration::from_secs(3), child.wait_with_output())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output.status.code(), Some(exit_code));
        assert_eq!(fs::read(marker).unwrap(), b"started");
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("waiting for launched command")
        );
        assert!(!log.parent().unwrap().exists());
    }
}

#[tokio::test]
async fn startup_failure_in_watch_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("missing/logs/Client.txt");
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["watch", "--log"])
        .arg(&log)
        .env("RETINUE_DAEMON_SOCKET", dir.path().join("unused.sock"))
        .env("RETINUE_COMMAND", "/bin/false")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(3), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains("waiting for launched command")
    );
    assert!(!log.parent().unwrap().exists());
}

#[tokio::test]
async fn unavailable_daemon_has_bounded_shutdown_and_ensures_once() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("Client.txt");
    fs::write(&log, b"").unwrap();
    let helper = dir.path().join("retinue");
    let count = dir.path().join("ensure-count");
    fs::write(&helper, "#!/bin/sh\nprintf x >> \"$ENSURE_COUNT\"\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
        .args(["launch", "--log"])
        .arg(&log)
        .args(["--", "/bin/sh", "-c", "sleep 0.1; exit 19"])
        .env("RETINUE_DAEMON_SOCKET", dir.path().join("missing.sock"))
        .env("RETINUE_COMMAND", &helper)
        .env("ENSURE_COUNT", &count)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.status.code(), Some(19));
    assert_eq!(fs::read(&count).unwrap(), b"x");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unacknowledged")
    );
}

#[tokio::test]
async fn observer_failure_preserves_child_exit_and_remains_interruptible() {
    for interrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("Client.txt");
        fs::write(&log, b"").unwrap();
        let marker = dir.path().join("child-exited");
        let socket = dir.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut reader, mut writer) = accept(&listener).await;
            let request = read(&mut reader).await;
            acknowledge(&mut writer, &json!(request["id"].as_u64().unwrap() + 1)).await;
        });
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_poe2companion"))
            .args(["launch", "--log"])
            .arg(&log)
            .args([
                "--",
                "/bin/sh",
                "-c",
                "sleep 0.4; printf done > \"$1\"; exit 23",
                "test-child",
            ])
            .arg(&marker)
            .env("RETINUE_DAEMON_SOCKET", &socket)
            .env("RETINUE_COMMAND", "/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut lines = BufReader::new(stderr).lines();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let line = lines
                    .next_line()
                    .await
                    .unwrap()
                    .expect("observer did not report failure");
                if line.contains("waiting for launched command") {
                    break;
                }
            }
        })
        .await
        .unwrap();
        if interrupt {
            rustix::process::kill_process(
                rustix::process::Pid::from_raw(child.id().unwrap() as i32).unwrap(),
                rustix::process::Signal::INT,
            )
            .unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.code(), Some(if interrupt { 130 } else { 23 }));
        // The synthetic child keeps the pipe open until it exits, even after Ctrl-C.
        tokio::time::timeout(Duration::from_secs(3), async {
            while lines.next_line().await.unwrap().is_some() {}
        })
        .await
        .unwrap();
        assert_eq!(fs::read(&marker).unwrap(), b"done");
        server.await.unwrap();
    }
}
