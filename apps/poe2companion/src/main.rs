use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use poe2companion::{Batch, Publisher, Tailer, Watcher, canonical_log, locate_log};
use retinue_client::paths::Paths;
use serde_json::json;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(
    name = "poe2companion",
    version,
    about = "Read-only Path of Exile 2 log observer"
)]
struct Args {
    #[arg(long, global = true, value_name = "PATH")]
    log: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Observe only records appended after startup.
    Watch,
    /// Print historical domain records as JSON lines; no daemon connection.
    Inspect,
    /// Observe logs while the supplied command runs, preserving its exit status.
    Launch {
        #[arg(last = true, required = true, num_args = 1..)]
        command: Vec<OsString>,
    },
    /// Report used directories without creating them.
    Paths {
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("poe2companion: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> io::Result<u8> {
    if let Some(Command::Paths { json }) = args.command {
        let paths = Paths::from_env();
        let runtime = paths
            .daemon_socket()
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let entries = [json!({"kind":"runtime", "path":runtime})];
        if json {
            println!("{}", json!({"app":"poe2companion", "paths":entries}));
        } else {
            for entry in entries {
                println!(
                    "{}\t{}",
                    entry["kind"].as_str().unwrap(),
                    entry["path"].as_str().unwrap()
                );
            }
        }
        return Ok(0);
    }
    if matches!(args.command, Some(Command::Inspect)) {
        let log = args.log.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "inspect requires --log PATH")
        })?;
        if !log.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "inspect log must exist and be a regular file",
            ));
        }
        let mut tailer = Tailer::open(&log, false)?;
        let stdout = io::stdout();
        let mut output = stdout.lock();
        loop {
            if let Some(batch) = tailer.next_batch()? {
                for event in batch.events {
                    serde_json::to_writer(&mut output, &event)?;
                    writeln!(output)?;
                }
            } else if !tailer.has_more() {
                break;
            }
        }
        return Ok(0);
    }
    let setup: io::Result<_> = (|| {
        let log = canonical_log(&match args.log {
            Some(path) => path,
            None => locate_log()?,
        })?;
        let watcher = Watcher::new(&log)?;
        let tailer = Tailer::open(&log, true)?;
        Ok((tailer, watcher))
    })();
    let child = if let Some(Command::Launch { command }) = args.command {
        Some(
            tokio::process::Command::new(&command[0])
                .args(&command[1..])
                .spawn()?,
        )
    } else {
        None
    };
    match setup {
        Ok((tailer, watcher)) => observe(tailer, watcher, child).await,
        Err(error) => wait_after_observer_failure(error, child, tokio::signal::ctrl_c()).await,
    }
}

async fn observe(
    mut tailer: Tailer,
    watcher: Watcher,
    mut child: Option<tokio::process::Child>,
) -> io::Result<u8> {
    let mode = if child.is_some() { "launch" } else { "watch" };
    let mut publisher = Publisher::new(Paths::from_env().daemon_socket().to_owned(), mode);
    let (send, mut receive) = mpsc::channel::<(u64, Batch)>(1);
    let (ack_send, mut acknowledgements) = mpsc::channel(1);
    let worker = tokio::spawn(async move {
        while let Some((id, batch)) = receive.recv().await {
            let result = publisher.publish(id, &batch).await.map(|()| id);
            let failed = result.is_err();
            if ack_send.send(result).await.is_err() || failed {
                break;
            }
        }
    });
    let mut pending: Option<(u64, u64)> = None;
    let mut id = 2u64;
    let mut dirty = true;
    let signal = tokio::signal::ctrl_c();
    tokio::pin!(signal);
    let result = loop {
        if pending.is_none() && dirty {
            match tailer.next_batch() {
                Ok(Some(batch)) => {
                    let cursor = batch.cursor;
                    if send.send((id, batch)).await.is_err() {
                        break Err(io::Error::other("publisher stopped"));
                    }
                    pending = Some((id, cursor));
                    let Some(next) = id.checked_add(1) else {
                        break Err(io::Error::other("request ID overflow"));
                    };
                    id = next;
                }
                Ok(None) => {
                    dirty = tailer.has_more();
                }
                Err(error) => break Err(error),
            }
        }
        tokio::select! {
            _ = tokio::task::yield_now(), if dirty && pending.is_none() => {}
            result = &mut signal => { break result.map(|()| 130); }
            status = async { child.as_mut().unwrap().wait().await }, if child.is_some() => {
                break status.map(|status| status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1)) as u8);
            }
            result = watcher.changed() => {
                if let Err(error) = result.and_then(|()| tailer.refresh()) { break Err(error); }
                dirty = true;
            }
            acknowledgement = acknowledgements.recv(), if pending.is_some() => {
                match acknowledgement {
                    Some(Ok(ack)) if Some(ack) == pending.map(|p| p.0) => { pending = None; dirty = true; }
                    Some(Err(error)) => break Err(error),
                    _ => break Err(io::Error::other("publisher acknowledgement missing or mismatched")),
                }
            }
        }
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        if let Some((expected, _)) = pending {
            if matches!(tokio::time::timeout_at(deadline, acknowledgements.recv()).await, Ok(Some(Ok(ack))) if ack == expected)
            {
                pending = None;
            } else {
                break;
            }
        } else if result.is_ok() {
            match tailer.next_batch() {
                Ok(Some(batch)) => {
                    let cursor = batch.cursor;
                    if send.send((id, batch)).await.is_err() {
                        break;
                    }
                    pending = Some((id, cursor));
                    id = id.saturating_add(1);
                }
                Ok(None) if tailer.has_more() => tokio::task::yield_now().await,
                Ok(None) => break,
                Err(error) => {
                    eprintln!("poe2companion: final log drain failed: {error}");
                    break;
                }
            }
        } else {
            break;
        }
    }
    if let Some((id, cursor)) = pending {
        eprintln!(
            "poe2companion: batch {id} through byte {cursor} is unacknowledged; stopping without a delivery guarantee"
        );
    }
    if tailer.has_more() {
        eprintln!("poe2companion: unread log data remains at shutdown");
    }
    drop(send);
    worker.abort();
    let _ = worker.await;
    match result {
        Err(error) => wait_after_observer_failure(error, child, &mut signal).await,
        result => result,
    }
}

async fn wait_after_observer_failure(
    error: io::Error,
    child: Option<tokio::process::Child>,
    signal: impl std::future::Future<Output = io::Result<()>>,
) -> io::Result<u8> {
    let Some(mut child) = child else {
        return Err(error);
    };
    // Telemetry failures must not terminate the user's game or replace its status.
    eprintln!("poe2companion: observer stopped: {error}; waiting for launched command");
    tokio::pin!(signal);
    tokio::select! {
        status = child.wait() => {
            let status = status?;
            Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(1)) as u8)
        }
        signal = &mut signal => signal.map(|()| 130),
    }
}
