use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::Duration,
};
use tokio::{process::Command, sync::oneshot};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub struct Login {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Login {
    pub fn start(
        send: impl FnOnce(Result<Value, String>) + Send + 'static,
    ) -> Result<Self, String> {
        let (stop, cancel) = oneshot::channel();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let thread = thread::Builder::new()
            .name("wfgui-login".into())
            .spawn(move || {
                send(runtime.block_on(run(cancel)));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}
impl Drop for Login {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn executable(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|p| p.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

fn browser(profile: &Path) -> Result<Command, String> {
    for name in [
        "brave-browser",
        "brave",
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "microsoft-edge-stable",
        "microsoft-edge",
    ] {
        if let Some(path) = executable(name) {
            return Ok(Command::new(path));
        }
    }
    if let Some(flatpak) = executable("flatpak") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        for id in [
            "com.brave.Browser",
            "com.google.Chrome",
            "org.chromium.Chromium",
            "com.microsoft.Edge",
        ] {
            let suffix = format!("app/{id}/current/active");
            if home.join(".local/share/flatpak").join(&suffix).exists()
                || Path::new("/var/lib/flatpak").join(&suffix).exists()
            {
                let mut command = Command::new(flatpak);
                command.args(["run", &format!("--filesystem={}", profile.display()), id]);
                return Ok(command);
            }
        }
    }
    Err("Sign-in requires a Chromium-based browser (Chromium, Chrome, Brave or Edge)".into())
}

async fn run(mut cancel: oneshot::Receiver<()>) -> Result<Value, String> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let profile = tempfile::Builder::new()
        .prefix("wfgui-sign-in-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let mut command = browser(profile.path())?;
    command.args([
        format!("--user-data-dir={}", profile.path().display()),
        "--remote-debugging-address=127.0.0.1".into(),
        "--remote-debugging-port=0".into(),
        "--remote-allow-origins=http://localhost".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-sync".into(),
        "--new-window".into(),
        "https://overframe.gg/account/".into(),
    ]);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not start sign-in browser: {e}"))?;
    let result = tokio::select! {
        _=&mut cancel=>Err("Sign-in cancelled".into()),
        _=child.wait()=>Err("Sign-in browser closed".into()),
        result=read_session(profile.path())=>result,
    };
    if tokio::time::timeout(Duration::from_millis(800), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
    result
}

fn debugger_url(text: &str) -> Result<String, String> {
    let mut lines = text.lines();
    let port = lines
        .next()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|p| *p > 0)
        .ok_or("Invalid browser control port")?;
    let path = lines
        .next()
        .filter(|p| p.starts_with("/devtools/browser/") && !p.chars().any(char::is_whitespace))
        .ok_or("Invalid browser control endpoint")?;
    Ok(format!("ws://127.0.0.1:{port}{path}"))
}

fn session_cookies(reply: &Value) -> Option<Value> {
    let cookies=reply["result"]["cookies"].as_array()?.iter().filter(|cookie|matches!(cookie["domain"].as_str(),Some("overframe.gg"|".overframe.gg"))).map(|cookie|json!({"name":cookie["name"],"value":cookie["value"],"domain":cookie["domain"],"expires":cookie["expires"]})).collect::<Vec<_>>();
    cookies
        .iter()
        .any(|c| c["name"] == "sessionid" && c["value"].as_str().is_some_and(|v| !v.is_empty()))
        .then(|| Value::Array(cookies))
}

async fn read_session(profile: &Path) -> Result<Value, String> {
    let endpoint = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(profile.join("DevToolsActivePort")) {
                if let Ok(url) = debugger_url(&text) {
                    break url;
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await
    .map_err(|_| "Could not connect to the sign-in browser")?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(1024 * 1024))
        .max_frame_size(Some(1024 * 1024))
        .read_buffer_size(4096);
    let (mut websocket, _) = connect_async_with_config(&endpoint, Some(config), false)
        .await
        .map_err(|_| "Could not open browser control connection")?;
    let mut sequence = 0u64;
    loop {
        sequence += 1;
        websocket
            .send(Message::Text(
                json!({"id":sequence,"method":"Storage.getCookies"})
                    .to_string()
                    .into(),
            ))
            .await
            .map_err(|_| "Sign-in browser disconnected")?;
        let reply = tokio::time::timeout(Duration::from_secs(8), async {
            while let Some(message) = websocket.next().await {
                let message = message.map_err(|_| "Sign-in browser disconnected")?;
                if let Message::Text(text) = message {
                    let reply: Value =
                        serde_json::from_str(&text).map_err(|_| "Invalid browser response")?;
                    if reply["id"] == sequence {
                        return Ok::<_, String>(reply);
                    }
                }
            }
            Err("Sign-in browser closed".into())
        })
        .await
        .map_err(|_| "Sign-in browser stopped responding")??;
        if let Some(cookies) = session_cookies(&reply) {
            let _ = websocket
                .send(Message::Text(
                    json!({"id":sequence+1,"method":"Browser.close"})
                        .to_string()
                        .into(),
                ))
                .await;
            return Ok(cookies);
        }
        if reply.get("error").is_some() {
            return Err("Browser could not read the sign-in session".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_overframe_session_is_imported() {
        let reply = json!({"result":{"cookies":[{"name":"sessionid","domain":".overframe.gg","value":"test"},{"name":"sessionid","domain":"overframe.gg.attacker.test","value":"secret"},{"name":"unrelated","domain":"example.com","value":"secret"}]}});
        let cookies = session_cookies(&reply).unwrap();
        assert_eq!(cookies.as_array().unwrap().len(), 1);
        assert!(session_cookies(&json!({"result":{"cookies":[{"name":"sessionid","domain":"example.com","value":"secret"}]}})).is_none());
    }
    #[test]
    fn debugger_endpoint_stays_on_loopback() {
        assert_eq!(
            debugger_url("4321\n/devtools/browser/test").unwrap(),
            "ws://127.0.0.1:4321/devtools/browser/test"
        );
        assert!(debugger_url("4321\nhttps://example.com").is_err());
        assert!(debugger_url("0\n/devtools/browser/test").is_err());
    }
}
