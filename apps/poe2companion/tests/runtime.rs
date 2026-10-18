mod support;

use std::ffi::OsString;
use std::fs::{self, File};
use std::os::unix::ffi::OsStringExt;
#[cfg(debug_assertions)]
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use retinue_client::child::ChildIdentity;
use rustix::process::{Pid, Signal, kill_process};
use serde_json::json;
use tokio::net::UnixListener;

const BINARY: &str = env!("CARGO_BIN_EXE_poe2companion");
// Keep synthetic executable writes and process launches from racing across tests.
static SERIAL: Mutex<()> = Mutex::new(());

fn game(script: &str) -> Vec<OsString> {
    vec![
        "sh".into(),
        "-c".into(),
        format!("printf '%s\\n' \"$$\" >> \"$POE2_TEST_DIR/starts\"; {script}").into(),
        "synthetic-poe2-game".into(),
    ]
}

struct Launch {
    directory: tempfile::TempDir,
    companion: Child,
    input: Option<ChildStdin>,
}

impl Launch {
    fn start(
        reload: bool,
        game: Option<Vec<OsString>>,
        daemon: bool,
    ) -> (Self, Option<UnixListener>) {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("poe2companion");
        fs::copy(BINARY, &installed).unwrap();
        let socket = directory.path().join("daemon.sock");
        let listener = daemon.then(|| UnixListener::bind(&socket).unwrap());
        let helper = directory.path().join("retinue");
        fs::write(&helper, "#!/bin/sh\n[ \"$*\" = 'daemon ensure' ] || exit 9\nprintf x >> \"$POE2_TEST_DIR/ensures\"\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let mut command = Command::new(installed);
        command
            // Never discover or read a live game during lifecycle tests.
            .args(["--no-overlay", "--pid", "4294967295"])
            .env("POE2_TEST_DIR", directory.path())
            .env("RETINUE_DAEMON_SOCKET", socket)
            .env("RETINUE_COMMAND", helper)
            .env_remove("POE2COMPANION_RELOAD_FD")
            .env_remove("POE2COMPANION_DEV_RELOAD")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(
                File::create(directory.path().join("stdout")).unwrap(),
            ))
            .stderr(Stdio::from(
                File::create(directory.path().join("stderr")).unwrap(),
            ));
        if !reload {
            command.env("POE2COMPANION_DEV_RELOAD", "0");
        }
        match game {
            Some(game) => {
                command.args(["launch", "--"]).args(game);
            }
            None => {
                command.arg("watch");
            }
        }
        let mut companion = command.spawn().unwrap();
        let input = companion.stdin.take();
        (
            Self {
                directory,
                companion,
                input,
            },
            listener,
        )
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn log(&self) -> String {
        fs::read_to_string(self.path("stderr")).unwrap_or_default()
    }

    async fn game_pid(&self) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(pid) = fs::read_to_string(self.path("starts"))
                .ok()
                .and_then(|starts| starts.lines().next()?.parse().ok())
            {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "synthetic game did not start: {}",
                self.log()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn assert_one_game(&self) {
        assert_eq!(
            fs::read_to_string(self.path("starts"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    async fn wait_for_log(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if self.log().contains(text) {
                return;
            }
            assert!(
                self.companion.try_wait().unwrap().is_none(),
                "companion stopped: {}",
                self.log()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("companion log did not contain {text}: {}", self.log());
    }

    async fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Some(status) = self.companion.try_wait().unwrap() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("companion did not exit: {}", self.log());
    }

    fn release(&mut self) {
        // EOF releases the synthetic game even if the fixture fails or the wrapper exits first.
        self.input.take();
    }

    fn signal(&self, signal: Signal) {
        kill_process(Pid::from_raw(self.companion.id() as i32).unwrap(), signal).unwrap();
    }

    #[cfg(debug_assertions)]
    fn replace(&self) {
        let candidate = self.path("candidate");
        fs::copy(BINARY, &candidate).unwrap();
        fs::rename(candidate, self.path("poe2companion")).unwrap();
    }
}

fn running(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            stat.rsplit_once(')')
                .map(|(_, fields)| !fields.trim_start().starts_with('Z'))
        })
        .unwrap_or(false)
}

impl Drop for Launch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let log = self.log();
            let mut tail: Vec<_> = log.lines().rev().take(12).collect();
            tail.reverse();
            eprintln!("companion failure log:\n{}", tail.join("\n"));
        }
        self.release();
        if self.companion.try_wait().ok().flatten().is_none() {
            if let Some(pid) = Pid::from_raw(self.companion.id() as i32) {
                let _ = kill_process(pid, Signal::INT);
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.companion.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.companion.try_wait().ok().flatten().is_none() {
                let _ = self.companion.kill();
            }
        }
        let _ = self.companion.wait();
        if let Ok(starts) = fs::read_to_string(self.path("starts")) {
            for pid in starts.lines().filter_map(|line| line.parse::<u32>().ok()) {
                let deadline = Instant::now() + Duration::from_secs(2);
                while running(pid) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if running(pid)
                    && let Some(pid) = Pid::from_raw(pid as i32)
                {
                    // These PIDs come only from this fixture's synthetic launch command.
                    let _ = kill_process(pid, Signal::KILL);
                }
            }
        }
    }
}

#[tokio::test]
async fn launch_preserves_exit_code_and_non_utf8_arguments_after_final_ack() {
    let _serial = SERIAL.lock().unwrap();
    let bytes = vec![b'x', 0xff, b'y'];
    let mut command = game("printf '%s' \"$1\"; read marker; exit 7");
    command.push(OsString::from_vec(bytes.clone()));
    let (mut launch, listener) = Launch::start(false, Some(command), true);
    let (mut reader, mut writer) = support::accept(&listener.unwrap()).await;
    let request = support::read(&mut reader).await;
    assert_eq!(request["data"]["status"], "error");
    launch.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        launch.companion.try_wait().unwrap().is_none(),
        "drain did not wait for the acknowledgement"
    );
    support::acknowledge(&mut writer, &request["id"]).await;
    assert_eq!(launch.wait().await.code(), Some(7), "{}", launch.log());
    assert_eq!(fs::read(launch.path("stdout")).unwrap(), bytes);
    assert!(!launch.log().contains("retaining unacknowledged"));
    launch.assert_one_game();
}

#[tokio::test]
async fn launched_child_signal_exit_is_preserved() {
    let _serial = SERIAL.lock().unwrap();
    let (mut launch, listener) =
        Launch::start(false, Some(game("read marker; kill -TERM $$")), true);
    let (mut reader, mut writer) = support::accept(&listener.unwrap()).await;
    let request = support::read(&mut reader).await;
    support::acknowledge(&mut writer, &request["id"]).await;
    launch.release();
    assert_eq!(launch.wait().await.code(), Some(143), "{}", launch.log());
}

#[tokio::test]
async fn fatal_publish_failure_preserves_child_exit_and_remains_interruptible() {
    let _serial = SERIAL.lock().unwrap();
    for (signal, expected) in [
        (Some(Signal::INT), 130),
        (Some(Signal::TERM), 143),
        (None, 23),
    ] {
        let (mut launch, listener) = Launch::start(false, Some(game("read marker; exit 23")), true);
        let (mut reader, mut writer) = support::accept(&listener.unwrap()).await;
        support::read(&mut reader).await;
        support::acknowledge(&mut writer, &json!(999)).await;
        launch
            .wait_for_log("waiting for launched command or development reload")
            .await;
        let child = launch.game_pid().await;
        assert!(running(child));
        match signal {
            Some(signal) => launch.signal(signal),
            None => launch.release(),
        }
        assert_eq!(
            launch.wait().await.code(),
            Some(expected),
            "{}",
            launch.log()
        );
        if signal.is_some() {
            assert!(running(child), "interrupt terminated the launched game");
        }
        launch.assert_one_game();
        launch.release();
    }
}

#[tokio::test]
async fn watch_exits_on_permanent_publish_failure() {
    let _serial = SERIAL.lock().unwrap();
    let (mut watch, listener) = Launch::start(false, None, true);
    let (mut reader, mut writer) = support::accept(&listener.unwrap()).await;
    let request = support::read(&mut reader).await;
    support::write(
        &mut writer,
        json!({"id":request["id"],"ok":false,"error":"invalid_session_sample"}),
    )
    .await;
    assert_eq!(watch.wait().await.code(), Some(1));
    assert!(
        watch
            .log()
            .contains("daemon rejected native session observation")
    );
    assert!(!watch.path("ensures").exists());
}

#[tokio::test]
async fn child_exit_with_missing_daemon_has_bounded_drain_and_one_ensure() {
    let _serial = SERIAL.lock().unwrap();
    let started = Instant::now();
    let (mut launch, _) = Launch::start(false, Some(game("sleep 0.2; exit 19")), false);
    assert_eq!(launch.wait().await.code(), Some(19), "{}", launch.log());
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(fs::read(launch.path("ensures")).unwrap(), b"x");
    assert!(
        launch
            .log()
            .contains("retaining unacknowledged observation")
    );
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn same_pid_reload_replays_pending_sample_without_relaunching_game() {
    let _serial = SERIAL.lock().unwrap();
    let (mut launch, listener) = Launch::start(true, Some(game("read marker; exit 7")), true);
    let listener = listener.unwrap();
    let (mut reader, _writer) = support::accept(&listener).await;
    let pending = support::read(&mut reader).await;
    launch.wait_for_log("reload.watching").await;
    let owner = launch.companion.id();
    let child = ChildIdentity::read(launch.game_pid().await, owner).unwrap();
    let original = fs::metadata(launch.path("poe2companion")).unwrap().ino();
    launch.replace();
    assert_ne!(
        fs::metadata(launch.path("poe2companion")).unwrap().ino(),
        original
    );
    // Reload drains publication, probes the replacement and hashes the large debug executable.
    let (mut reader, mut writer) =
        support::accept_with_timeout(&listener, Duration::from_secs(30)).await;
    let replayed = support::read(&mut reader).await;
    assert_eq!(
        replayed, pending,
        "reload changed the outstanding sample or its request identity"
    );
    support::acknowledge(&mut writer, &replayed["id"]).await;
    launch.wait_for_log("reload.resumed").await;
    assert_eq!(launch.companion.id(), owner);
    child.validate(owner).unwrap();
    assert_eq!(
        fs::metadata(format!("/proc/{owner}/exe")).unwrap().ino(),
        fs::metadata(launch.path("poe2companion")).unwrap().ino()
    );
    launch.assert_one_game();
    launch.release();
    assert_eq!(launch.wait().await.code(), Some(7), "{}", launch.log());
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn interrupt_during_reload_drain_cancels_replacement_and_preserves_game() {
    let _serial = SERIAL.lock().unwrap();
    let (mut launch, listener) = Launch::start(true, Some(game("read marker; exit 7")), true);
    let (mut reader, _writer) = support::accept(&listener.unwrap()).await;
    support::read(&mut reader).await;
    launch.wait_for_log("reload.watching").await;
    launch.replace();
    launch.wait_for_log("reload.pending").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    launch.signal(Signal::INT);
    assert_eq!(launch.wait().await.code(), Some(130), "{}", launch.log());
    assert!(running(launch.game_pid().await));
    assert!(!launch.log().contains("reload.resumed"));
    launch.assert_one_game();
    launch.release();
}
