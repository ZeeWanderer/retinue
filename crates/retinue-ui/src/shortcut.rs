use ashpd::desktop::{
    CreateSessionOptions,
    global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut},
};
use futures_util::StreamExt;
use std::{collections::HashMap, thread, time::Duration};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Copy)]
pub struct Binding {
    pub app_id: &'static str,
    pub id: &'static str,
    pub description: &'static str,
    pub trigger: &'static str,
    pub on_release: bool,
}

#[derive(Clone)]
pub struct Controller(watch::Sender<bool>);
impl Controller {
    pub fn set_enabled(&self, enabled: bool) {
        self.0.send_if_modified(|value| {
            if *value == enabled {
                false
            } else {
                *value = enabled;
                true
            }
        });
    }
}

pub struct Service {
    controller: Controller,
    stop: Option<oneshot::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Service {
    pub fn spawn(
        binding: Binding,
        activated: impl Fn() + Send + 'static,
        report: impl Fn(&str, String) + Send + 'static,
    ) -> Result<Self, String> {
        let (enabled, receiver) = watch::channel(false);
        let (stop, mut stopping) = oneshot::channel();
        let worker = thread::Builder::new()
            .name(format!("{}-shortcut", binding.id))
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|runtime| {
                        runtime.block_on(async {
                            let setup = async {
                                let connection = ashpd::zbus::Connection::session()
                                    .await
                                    .map_err(|e| e.to_string())?;
                                let registry = ashpd::zbus::Proxy::new(
                                    &connection,
                                    "org.freedesktop.portal.Desktop",
                                    "/org/freedesktop/portal/desktop",
                                    "org.freedesktop.host.portal.Registry",
                                )
                                .await
                                .map_err(|e| e.to_string())?;
                                registry
                                    .call::<_, _, ()>(
                                        "Register",
                                        &(
                                            binding.app_id,
                                            HashMap::<String, ashpd::zvariant::OwnedValue>::new(),
                                        ),
                                    )
                                    .await
                                    .map_err(|e| format!("desktop identity: {e}"))?;
                                let portal = GlobalShortcuts::with_connection(connection)
                                    .await
                                    .map_err(|e| e.to_string())?;
                                let session = portal
                                    .create_session(CreateSessionOptions::default())
                                    .await
                                    .map_err(|e| e.to_string())?;
                                Ok::<_, String>((portal, session))
                            };
                            let (portal, session) = tokio::select! {
                                biased;
                                _ = &mut stopping => return Ok(()),
                                result = setup => result?,
                            };
                            let listen = async {
                                let mut events = Box::pin(
                                    portal
                                        .receive_activated()
                                        .await
                                        .map_err(|e| e.to_string())?,
                                );
                                let mut releases = Box::pin(
                                    portal.receive_deactivated().await.map_err(|e| e.to_string())?,
                                );
                                let shortcut = NewShortcut::new(binding.id, binding.description)
                                    .preferred_trigger(binding.trigger);
                                let response = portal
                                    .bind_shortcuts(
                                        &session,
                                        &[shortcut],
                                        None,
                                        BindShortcutsOptions::default(),
                                    )
                                    .await
                                    .and_then(|r| r.response())
                                    .map_err(|e| e.to_string())?;
                                if !response.shortcuts().iter().any(|s| s.id() == binding.id) {
                                    return Err(format!("{} was not granted", binding.trigger));
                                }
                                report("shortcut.active", format!("id={}", binding.id));
                                let mut armed = false;
                                loop {
                                    tokio::select! {
                                        event = events.next() => {
                                            let Some(event) = event else { break; };
                                            if event.shortcut_id() == binding.id && *receiver.borrow() {
                                                if binding.on_release { armed = true; }
                                                else { activated(); }
                                            }
                                        }
                                        event = releases.next() => {
                                            let Some(event) = event else { break; };
                                            if event.shortcut_id() == binding.id && armed {
                                                armed = false;
                                                if *receiver.borrow() { activated(); }
                                            }
                                        }
                                    }
                                }
                                Err("shortcut activation stream closed".to_owned())
                            };
                            let result = tokio::select! {
                                biased;
                                _ = &mut stopping => Ok(()),
                                result = listen => result,
                            };
                            let _ =
                                tokio::time::timeout(Duration::from_secs(2), session.close()).await;
                            report("shortcut.inactive", format!("id={}", binding.id));
                            result
                        })
                    });
                if let Err(error) = result {
                    report("shortcut.unavailable", error);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            controller: Controller(enabled),
            stop: Some(stop),
            worker: Some(worker),
        })
    }
    pub fn controller(&self) -> Controller {
        self.controller.clone()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_joins_with_live_controller() {
        let (enabled, _) = watch::channel(false);
        let (stop, stopping) = oneshot::channel();
        let worker = thread::spawn(move || {
            stopping.blocking_recv().unwrap();
        });
        let service = Service {
            controller: Controller(enabled),
            stop: Some(stop),
            worker: Some(worker),
        };
        let controller = service.controller();
        drop(service);
        controller.set_enabled(true);
    }
}
