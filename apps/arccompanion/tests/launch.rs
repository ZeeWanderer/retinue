use std::fs::{self, File};
use std::io::Seek;
use std::os::unix::ffi::OsStringExt;
#[cfg(debug_assertions)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use retinue_client::child::ChildIdentity;
use rustix::process::{Pid, Signal, kill_process};

const BINARY: &str = env!("CARGO_BIN_EXE_arccompanion");
// Avoid inheriting another test's writable executable-fixture FDs across fork.
static SERIAL: Mutex<()> = Mutex::new(());

fn command(path: &std::path::Path) -> Command {
    let mut command = Command::new(path);
    command
        .env_remove("ARCCOMPANION_HANDOFF")
        .env_remove("ARCCOMPANION_DEV_RELOAD");
    command
}

fn output(mut command: Command) -> std::process::Output {
    command.env("ARCCOMPANION_DEV_RELOAD", "0");
    retinue_client::process::output(command, Duration::from_secs(5), || true).unwrap()
}

struct Launch {
    directory: tempfile::TempDir,
    companion: Child,
}

impl Launch {
    fn start(reload: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let installed = directory.path().join("arccompanion");
        fs::copy(BINARY, &installed).unwrap();
        let mut command = command(&installed);
        if !reload {
            command.env("ARCCOMPANION_DEV_RELOAD", "0");
        }
        command
            .env("ARCCOMPANION_TEST_DIR", directory.path())
            .args([
                "launch", "--", "sh", "-c",
                "printf '%s\\n' \"$$\" >> \"$ARCCOMPANION_TEST_DIR/starts\"; while [ ! -e \"$ARCCOMPANION_TEST_DIR/release\" ]; do sleep 0.05; done; exit 7",
            ])
            .stdout(Stdio::from(File::create(directory.path().join("stdout")).unwrap()))
            .stderr(Stdio::from(File::create(directory.path().join("stderr")).unwrap()));
        let companion = command.spawn().unwrap();
        let mut launch = Self {
            directory,
            companion,
        };
        launch.wait_for(|this| this.game_pid().is_some());
        if reload && cfg!(debug_assertions) {
            launch.wait_for_log("reload.watching");
        }
        launch
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.path().join(name)
    }

    fn log(&self) -> String {
        fs::read_to_string(self.path("stderr")).unwrap_or_default()
    }

    fn game_pid(&self) -> Option<u32> {
        fs::read_to_string(self.path("starts"))
            .ok()?
            .lines()
            .next()?
            .parse()
            .ok()
    }

    fn wait_for(&mut self, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if ready(self) {
                return;
            }
            assert!(
                self.companion.try_wait().unwrap().is_none(),
                "companion stopped: {}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("companion deadline: {}", self.log());
    }

    fn wait_for_log(&mut self, text: &str) {
        self.wait_for(|this| this.log().contains(text));
    }

    #[cfg(debug_assertions)]
    fn replace(&self, script: Option<&[u8]>) {
        let candidate = self.path("candidate");
        match script {
            Some(script) => {
                fs::write(&candidate, script).unwrap();
                fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700)).unwrap();
            }
            None => {
                fs::copy(BINARY, &candidate).unwrap();
            }
        }
        fs::rename(candidate, self.path("arccompanion")).unwrap();
    }

    fn signal(&self, signal: Signal) {
        kill_process(Pid::from_raw(self.companion.id() as i32).unwrap(), signal).unwrap();
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Some(status) = self.companion.try_wait().unwrap() {
                return status;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("companion did not exit: {}", self.log());
    }

    fn release(&self) {
        fs::write(self.path("release"), b"").unwrap();
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
}

impl Drop for Launch {
    fn drop(&mut self) {
        let _ = fs::write(self.path("release"), b"");
        // Only synthetic probe processes created by this fixture are terminated.
        if let Ok(pid) = fs::read_to_string(self.path("probe.pid"))
            && let Ok(pid) = pid.parse::<i32>()
            && let Some(pid) = Pid::from_raw(pid)
        {
            let _ = kill_process(pid, Signal::KILL);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.companion.try_wait().ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.companion.try_wait().ok().flatten().is_none() {
            let _ = self.companion.kill();
        }
        let _ = self.companion.wait();
        // Keep the release file until an orphaned synthetic game has consumed it.
        let deadline = Instant::now() + Duration::from_secs(2);
        if let Some(pid) = self.game_pid() {
            while Instant::now() < deadline {
                let running = fs::read_to_string(format!("/proc/{pid}/stat"))
                    .ok()
                    .and_then(|stat| {
                        stat.rsplit_once(')')
                            .map(|(_, fields)| fields.trim_start().starts_with('Z'))
                    })
                    == Some(false);
                if !running {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            if let Some(pid) = Pid::from_raw(pid as i32) {
                let _ = kill_process(pid, Signal::KILL);
            }
        }
    }
}

#[test]
fn launch_preserves_child_exit_code_signal_and_non_utf8_arguments() {
    let _serial = SERIAL.lock().unwrap();
    for (script, expected) in [("exit 7", 7), ("kill -TERM $$", 143)] {
        let mut launch = command(std::path::Path::new(BINARY));
        launch.args(["launch", "--", "sh", "-c", script]);
        assert_eq!(output(launch).status.code(), Some(expected));
    }
    let bytes = vec![b'x', 0xff, b'y'];
    let mut launch = command(std::path::Path::new(BINARY));
    launch
        .args(["launch", "--", "sh", "-c", "printf '%s' \"$1\"", "test"])
        .arg(std::ffi::OsString::from_vec(bytes.clone()));
    let result = output(launch);
    assert!(result.status.success());
    assert_eq!(result.stdout, bytes);
}

#[test]
fn status_and_paths_remain_scaffolds_and_reload_check_stays_hidden() {
    let _serial = SERIAL.lock().unwrap();
    let mut paths = command(std::path::Path::new(BINARY));
    paths.args(["paths", "--json"]);
    let result = output(paths);
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        serde_json::json!({"app":"arccompanion", "paths":[]})
    );
    let mut status = command(std::path::Path::new(BINARY));
    status.args(["status", "--json"]);
    let result = output(status);
    let status: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(status["observation"], "not_implemented");
    let mut help = command(std::path::Path::new(BINARY));
    help.arg("--help");
    let help = String::from_utf8(output(help).stdout).unwrap();
    assert!(!help.contains("--reload-check"));
    assert!(help.contains("ARCCOMPANION_DEV_RELOAD=0"));
}

#[test]
fn reload_probe_checks_owner_schema_and_build_before_starting_services() {
    let _serial = SERIAL.lock().unwrap();
    for (schema, owner) in [(1, std::process::id()), (0, std::process::id()), (1, 0)] {
        let mut input = tempfile::tempfile().unwrap();
        serde_json::to_writer(
            &mut input,
            &serde_json::json!({
                "schema":schema, "owner":owner, "installed":BINARY, "child":null, "state":{}
            }),
        )
        .unwrap();
        input.rewind().unwrap();
        let mut probe = command(std::path::Path::new(BINARY));
        probe.arg("--reload-check");
        let result = retinue_client::process::output_with_stdin(
            probe,
            Stdio::from(input),
            Duration::from_secs(5),
            || true,
        )
        .unwrap();
        let accepted = cfg!(debug_assertions) && schema == 1 && owner == std::process::id();
        assert_eq!(result.status.success(), accepted);
        assert_eq!(
            result.stdout,
            if accepted {
                b"arccompanion-reload/1\n".as_slice()
            } else {
                b""
            }
        );
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn interrupt_returns_signal_code_without_terminating_the_launched_game() {
    let _serial = SERIAL.lock().unwrap();
    for (signal, expected) in [(Signal::INT, 130), (Signal::TERM, 143)] {
        let mut launch = Launch::start(false);
        let pid = launch.game_pid().unwrap();
        launch.signal(signal);
        assert_eq!(launch.wait().code(), Some(expected), "{}", launch.log());
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        assert_ne!(
            stat.rsplit_once(')').unwrap().1.split_whitespace().next(),
            Some("Z")
        );
        launch.assert_one_game();
        launch.release();
    }
}

#[test]
#[cfg(debug_assertions)]
fn identical_staged_binary_reloads_same_pid_without_relaunching_the_game() {
    let _serial = SERIAL.lock().unwrap();
    let mut launch = Launch::start(true);
    let pid = launch.companion.id();
    let child = ChildIdentity::read(launch.game_pid().unwrap(), pid).unwrap();
    let original = fs::metadata(format!("/proc/{pid}/exe")).unwrap().ino();
    launch.replace(None);
    launch.wait_for_log("reload.resumed");
    assert_eq!(launch.companion.id(), pid);
    child.validate(pid).unwrap();
    assert_ne!(
        fs::metadata(format!("/proc/{pid}/exe")).unwrap().ino(),
        original
    );
    assert_eq!(
        fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
        launch.path("arccompanion")
    );
    for fd in fs::read_dir(format!("/proc/{pid}/fd")).unwrap().flatten() {
        if let Ok(target) = fs::read_link(fd.path()) {
            assert!(
                !target
                    .to_string_lossy()
                    .contains("memfd:arccompanion-reload")
            );
        }
    }
    launch.assert_one_game();
    launch.release();
    assert_eq!(launch.wait().code(), Some(7));
}

#[test]
#[cfg(debug_assertions)]
fn rejected_candidate_recovers_old_monitor_and_is_not_retried() {
    let _serial = SERIAL.lock().unwrap();
    let mut launch = Launch::start(true);
    let child = ChildIdentity::read(launch.game_pid().unwrap(), launch.companion.id()).unwrap();
    launch.replace(Some(b"#!/bin/sh\nprintf 'wrongcompanion-reload/1\\n'\n"));
    launch.wait_for_log("reload.failed");
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(launch.log().matches("reload.failed").count(), 1);
    child.validate(launch.companion.id()).unwrap();
    launch.assert_one_game();
    launch.release();
    assert_eq!(launch.wait().code(), Some(7));
}

#[test]
#[cfg(debug_assertions)]
fn signal_during_reload_probe_cancels_and_reaps_the_probe() {
    let _serial = SERIAL.lock().unwrap();
    let mut launch = Launch::start(true);
    launch.replace(Some(
        b"#!/bin/sh\nprintf '%s' \"$$\" > \"$ARCCOMPANION_TEST_DIR/probe.pid\"\nexec sleep 60\n",
    ));
    launch.wait_for(|this| {
        fs::read_to_string(this.path("probe.pid")).is_ok_and(|pid| pid.parse::<u32>().is_ok())
    });
    let probe = fs::read_to_string(launch.path("probe.pid")).unwrap();
    let started = Instant::now();
    launch.signal(Signal::TERM);
    assert_eq!(launch.wait().code(), Some(143), "{}", launch.log());
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!std::path::Path::new(&format!("/proc/{probe}")).exists());
    fs::remove_file(launch.path("probe.pid")).unwrap();
    assert!(!launch.log().contains("reload.resumed"));
    launch.assert_one_game();
    launch.release();
}

#[test]
#[cfg(debug_assertions)]
fn explicit_opt_out_keeps_old_wrapper_until_the_game_exits() {
    let _serial = SERIAL.lock().unwrap();
    let mut launch = Launch::start(false);
    launch.replace(None);
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!launch.log().contains("reload."));
    launch.assert_one_game();
    launch.release();
    assert_eq!(launch.wait().code(), Some(7));
}
