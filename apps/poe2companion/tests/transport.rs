mod support;

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use poe2companion::{Connection, Observation, Publisher, Sample};
use serde_json::json;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

fn sample(sequence: u64) -> Sample {
    Sample {
        stream: "native-test-stream".into(),
        sequence,
        observed_at: sequence.saturating_add(1234),
        observation: Observation::from_result(Err(format!("synthetic read failure {sequence}"))),
    }
}

#[tokio::test]
async fn prediction_requires_its_negotiated_optional_feature() {
    for enabled in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let hello = support::read(&mut reader).await;
            assert_eq!(hello["features"], json!(["trade.prediction"]));
            support::write(
                &mut writer,
                json!({"id":1,"ok":true,"compatible":true,
                "game":"poe2","envelope":2,"interfaces":{"session":2,"trade":5},
                "features": if enabled {vec!["trade.prediction"]} else {vec![]}}),
            )
            .await;
            if enabled {
                let request = support::read(&mut reader).await;
                assert_eq!(request["op"], "trade_predict");
                support::write(
                    &mut writer,
                    json!({"id":2,"ok":true,"dataset":"trade",
                    "data":{"estimate":{"amount":40,"currency":"exalted"}}}),
                )
                .await;
            } else {
                let mut tail = Vec::new();
                tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut tail)
                    .await
                    .unwrap();
                assert!(tail.is_empty());
            }
        });
        let reply = Connection::trade(
            &socket,
            json!({"op":"trade_predict","league":"A","text":"fixture"}),
        )
        .await
        .unwrap();
        assert_eq!(reply["data"]["estimate"].is_null(), !enabled);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn trade_negotiates_its_contract_and_rejects_wrong_datasets() {
    for (trade_version, dataset, expected) in [
        (5, "trade", None),
        (4, "trade", Some(ErrorKind::Unsupported)),
        (5, "session", Some(ErrorKind::InvalidData)),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let hello = support::read(&mut reader).await;
            assert_eq!(hello["interfaces"], json!({"session":2,"trade":5}));
            assert_eq!(hello["mode"], "overlay");
            support::write(
                &mut writer,
                json!({"id":1,"ok":true,"compatible":true,
                "game":"poe2","envelope":2,"interfaces":{"session":2,"trade":trade_version}}),
            )
            .await;
            if trade_version == 5 {
                let request = support::read(&mut reader).await;
                assert_eq!(request, json!({"id":2,"op":"trade_leagues"}));
                support::write(
                    &mut writer,
                    json!({"id":2,"ok":true,"dataset":dataset,
                    "data":{"leagues":[{"id":"Standard","text":"Standard"}]}}),
                )
                .await;
            }
        });
        let reply = Connection::trade(&socket, json!({"op":"trade_leagues"})).await;
        if let Some(expected) = expected {
            assert_eq!(reply.unwrap_err().kind(), expected);
        } else {
            assert_eq!(reply.unwrap()["data"]["leagues"][0]["id"], "Standard");
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn native_sample_is_published_under_session_two_and_acknowledged() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (mut reader, mut writer) = support::accept(&listener).await;
        let request = support::read(&mut reader).await;
        assert_eq!(
            request,
            json!({"op":"publish","id":8,"dataset":"session","data":sample(7)})
        );
        support::acknowledge(&mut writer, &request["id"]).await;
    });
    Connection::connect(&socket, "watch")
        .await
        .unwrap()
        .publish(&sample(7))
        .await
        .unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_acknowledgements_and_permanent_rejections_fail_closed() {
    let mut replies = vec![
        (
            json!({"id":9,"ok":true,"dataset":"session","data":{"revision":1}}),
            ErrorKind::InvalidData,
        ),
        (
            json!({"id":2,"ok":true,"dataset":"player","data":{"revision":1}}),
            ErrorKind::InvalidData,
        ),
        (
            json!({"id":2,"ok":true,"dataset":"session","data":{"revision":"1"}}),
            ErrorKind::InvalidData,
        ),
        (
            json!({"id":2,"ok":false,"error":"unavailable"}),
            ErrorKind::Other,
        ),
    ];
    for error in [
        "invalid_session_sample",
        "unsupported_poe2_request",
        "unauthorized",
        "observer_required",
        "session_sequence_conflict",
        "stale_session_sample",
    ] {
        replies.push((
            json!({"id":2,"ok":false,"error":error}),
            ErrorKind::InvalidInput,
        ));
    }
    for (reply, expected) in replies {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut reader, mut writer) = support::accept(&listener).await;
            support::read(&mut reader).await;
            support::write(&mut writer, reply).await;
        });
        let error = Connection::connect(&socket, "watch")
            .await
            .unwrap()
            .publish(&sample(1))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), expected);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn incompatible_hello_invalid_json_and_oversized_frames_fail_closed() {
    let hello = json!({"id":1,"ok":true,"compatible":true,"game":"poe2","envelope":2,"interfaces":{"session":2}});
    let mut cases = Vec::new();
    for (field, value) in [
        ("interfaces", json!({"session":1})),
        ("game", json!(null)),
        ("envelope", json!(1)),
    ] {
        let mut invalid = hello.clone();
        invalid[field] = value;
        cases.push((format!("{invalid}\n"), ErrorKind::Unsupported));
    }
    let mut wrong_id = hello;
    wrong_id["id"] = json!(2);
    cases.extend([
        (format!("{wrong_id}\n"), ErrorKind::InvalidData),
        (
            format!("{}\n", "x".repeat(130 * 1024)),
            ErrorKind::InvalidData,
        ),
        ("not-json\n".into(), ErrorKind::InvalidData),
    ]);
    for (reply, expected) in cases {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            support::read(&mut BufReader::new(reader)).await;
            // An oversized frame can close the peer before the entire write completes.
            let _ = writer.write_all(reply.as_bytes()).await;
        });
        let error = Connection::connect(&socket, "watch").await.err().unwrap();
        assert_eq!(error.kind(), expected);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn invalid_sequence_is_rejected_without_sending_a_request() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (mut reader, _writer) = support::accept(&listener).await;
        use tokio::io::AsyncBufReadExt;
        let mut line = String::new();
        assert_eq!(reader.read_line(&mut line).await.unwrap(), 0);
    });
    let mut connection = Connection::connect(&socket, "watch").await.unwrap();
    for sequence in [0, u64::MAX] {
        assert_eq!(
            connection
                .publish(&sample(sequence))
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::InvalidInput
        );
    }
    drop(connection);
    server.await.unwrap();
}

// Isolate environment overrides from the test harness and never run a real daemon ensure.
#[tokio::test]
async fn publisher_child() {
    let Ok(mode) = std::env::var("POE2_TEST_PUBLISHER") else {
        return;
    };
    let socket = std::env::var_os("POE2_TEST_SOCKET").unwrap().into();
    let stop = Arc::new(AtomicBool::new(false));
    let cancellation = (mode == "cancel").then(|| {
        let stop = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            stop.store(true, Ordering::Release);
        })
    });
    let mut publisher = Publisher::new(socket, "watch");
    let result = publisher.publish(&sample(1), &stop).await;
    match mode.as_str() {
        "retry" => {
            result.unwrap();
            publisher.publish(&sample(2), &stop).await.unwrap();
        }
        "permanent" => assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidInput),
        "cancel" => assert_eq!(result.unwrap_err().kind(), ErrorKind::Interrupted),
        _ => panic!("unexpected publisher mode {mode}"),
    }
    if let Some(cancellation) = cancellation {
        cancellation.await.unwrap();
    }
}

fn publisher_command(socket: &Path, helper: &Path, mode: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "publisher_child", "--nocapture"])
        .env("POE2_TEST_PUBLISHER", mode)
        .env("POE2_TEST_SOCKET", socket)
        .env("RETINUE_COMMAND", helper)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn publisher_finished(child: tokio::process::Child) {
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn script(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

async fn exercise_ensure_recovery(fail_first: bool) {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let count = directory.path().join("ensures");
    let helper = directory.path().join("retinue");
    script(
        &helper,
        "#!/bin/sh\n[ \"$*\" = 'daemon ensure' ] || exit 9\n[ -z \"${STEAM_RUNTIME+x}${STEAM_RUNTIME_LIBRARY_PATH+x}\" ] || exit 10\nprintf x >> \"$ENSURE_COUNT\"\nif [ \"$FAIL_FIRST\" = 1 ] && [ ! -e \"$ENSURE_COUNT.failed\" ]; then touch \"$ENSURE_COUNT.failed\"; exit 17; fi\n",
    );
    let child = publisher_command(&socket, &helper, "retry")
        .env("ENSURE_COUNT", &count)
        .env("FAIL_FIRST", if fail_first { "1" } else { "0" })
        .env("STEAM_RUNTIME", "test-runtime")
        .env("STEAM_RUNTIME_LIBRARY_PATH", "test-library")
        .spawn()
        .unwrap();
    for sequence in 1..=2 {
        let expected = sequence as usize + usize::from(fail_first);
        tokio::time::timeout(Duration::from_secs(5), async {
            while fs::read(&count).unwrap_or_default().len() < expected {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        if socket.exists() {
            fs::remove_file(&socket).unwrap();
        }
        let listener = UnixListener::bind(&socket).unwrap();
        let (mut reader, mut writer) = support::accept(&listener).await;
        let original = support::read(&mut reader).await;
        assert_eq!(
            original["data"],
            serde_json::to_value(sample(sequence)).unwrap()
        );
        for _ in 1..=2 {
            drop((reader, writer));
            (reader, writer) = support::accept(&listener).await;
            assert_eq!(support::read(&mut reader).await, original);
            assert_eq!(fs::read(&count).unwrap().len(), expected);
        }
        drop(listener);
        support::acknowledge(&mut writer, &original["id"]).await;
    }
    publisher_finished(child).await;
}

#[tokio::test]
async fn reconnect_preserves_sample_and_only_ensures_when_socket_is_unavailable() {
    exercise_ensure_recovery(false).await;
}

#[tokio::test]
async fn failed_ensure_is_retried_and_success_rearms_after_an_acknowledgement() {
    exercise_ensure_recovery(true).await;
}

#[tokio::test]
async fn permanent_rejection_does_not_ensure_or_reconnect() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("daemon.sock");
    let marker = directory.path().join("ensured");
    let helper = directory.path().join("retinue");
    script(&helper, "#!/bin/sh\ntouch \"$ENSURE_COUNT\"\n");
    let listener = UnixListener::bind(&socket).unwrap();
    let child = publisher_command(&socket, &helper, "permanent")
        .env("ENSURE_COUNT", &marker)
        .spawn()
        .unwrap();
    let (mut reader, mut writer) = support::accept(&listener).await;
    let request = support::read(&mut reader).await;
    support::write(
        &mut writer,
        json!({"id":request["id"],"ok":false,"error":"invalid_session_sample"}),
    )
    .await;
    publisher_finished(child).await;
    assert!(!marker.exists());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_interrupts_and_reaps_running_ensure() {
    let directory = tempfile::tempdir().unwrap();
    let helper = directory.path().join("retinue");
    let pid_file = directory.path().join("helper.pid");
    script(
        &helper,
        "#!/bin/sh\nprintf '%s' \"$$\" > \"$HELPER_PID\"\nexec sleep 60\n",
    );
    let child = publisher_command(&directory.path().join("missing.sock"), &helper, "cancel")
        .env("HELPER_PID", &pid_file)
        .spawn()
        .unwrap();
    publisher_finished(child).await;
    let pid: u32 = fs::read_to_string(pid_file).unwrap().parse().unwrap();
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "ensure child {pid} was not reaped"
    );
}
