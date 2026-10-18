//! Bounded subprocess output with cancellation, termination and reaping.

use std::io;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};

/// Run outside any async runtime; this helper owns and drops its Tokio runtime.
pub fn output(
    command: Command,
    timeout: Duration,
    active: impl Fn() -> bool,
) -> Result<Output, String> {
    output_with_stdin(command, Stdio::null(), timeout, active)
}

/// Run with supplied stdin, bounding stdout, stderr, time and child lifetime.
/// Like `output`, this must not run inside an async runtime.
pub fn output_with_stdin(
    command: Command,
    input: Stdio,
    timeout: Duration,
    active: impl Fn() -> bool,
) -> Result<Output, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("process runtime: {error}"))?;
    runtime.block_on(async {
        if !active() { return Err("process cancelled".into()); }
        let mut child = tokio::process::Command::from(command)
            .kill_on_drop(true).stdin(input).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|error| error.to_string())?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let result = {
            let work = async {
                let (stdout, stderr, status) = tokio::try_join!(
                    read_output(stdout, 64 * 1024), read_output(stderr, 16 * 1024), child.wait()
                )?;
                Ok(Output { stdout, stderr, status })
            };
            let cancelled = async {
                loop {
                    if !active() { break; }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            };
            tokio::select! {
                result = work => result,
                _ = tokio::time::sleep(timeout) => Err(io::Error::new(io::ErrorKind::TimedOut, "process deadline exceeded")),
                _ = cancelled => Err(io::Error::new(io::ErrorKind::ConnectionAborted, "process cancelled")),
            }
        };
        if result.is_err() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result.map_err(|error| error.to_string())
    })
}

async fn read_output(reader: impl AsyncRead + Unpin, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).await?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("process output limit exceeded"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn reads_both_streams_and_preserves_exit_status() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf result; printf problem >&2; exit 7"]);
        let result = output(command, Duration::from_secs(3), || true).unwrap();
        assert_eq!(result.stdout, b"result");
        assert_eq!(result.stderr, b"problem");
        assert_eq!(result.status.code(), Some(7));
    }

    #[test]
    fn deadline_and_cancellation_reap_children() {
        for cancelled in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let pid_file = dir.path().join("probe.pid");
            let started = Instant::now();
            let mut command = Command::new("sh");
            command
                .args(["-c", "printf '%s' \"$$\" > \"$1\"; exec sleep 60", "probe"])
                .arg(&pid_file);
            let deadline = if cancelled {
                Duration::from_secs(3)
            } else {
                Duration::from_millis(250)
            };
            let result = output(command, deadline, || {
                !cancelled || started.elapsed() < Duration::from_millis(250)
            });
            assert!(
                result
                    .unwrap_err()
                    .contains(if cancelled { "cancelled" } else { "deadline" })
            );
            assert!(started.elapsed() < Duration::from_secs(3));
            let pid = std::fs::read_to_string(pid_file)
                .unwrap()
                .parse::<i32>()
                .unwrap();
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
            assert_eq!(
                rustix::process::waitpid(
                    rustix::process::Pid::from_raw(pid),
                    rustix::process::WaitOptions::NOHANG
                )
                .unwrap_err(),
                rustix::io::Errno::CHILD
            );
        }
    }

    #[test]
    fn noisy_child_cannot_fill_memory_or_block_exit() {
        let started = Instant::now();
        let error = output(Command::new("yes"), Duration::from_secs(3), || true).unwrap_err();
        assert!(error.contains("output limit"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
