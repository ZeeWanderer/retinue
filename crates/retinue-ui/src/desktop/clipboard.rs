use std::sync::mpsc::{self, SyncSender};

pub enum Operation {
    Read,
    Write(String),
}

pub struct Clipboard<T> {
    requests: SyncSender<(T, Operation)>,
}

impl<T: Send + 'static> Clipboard<T> {
    pub fn new(
        reply: impl Fn(T, Result<Option<String>, String>) + Send + 'static,
    ) -> Result<Self, String> {
        let (requests, receiver) = mpsc::sync_channel::<(T, Operation)>(4);
        std::thread::Builder::new()
            .name("desktop-clipboard".into())
            .spawn(move || {
                // Retaining the owner keeps copied text available to other Linux clients.
                let mut clipboard = None;
                while let Ok((token, operation)) = receiver.recv() {
                    let result = (|| {
                        if clipboard.is_none() {
                            clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
                        }
                        let clipboard = clipboard.as_mut().unwrap();
                        match operation {
                            Operation::Read => clipboard.get_text().map(Some),
                            Operation::Write(text) => clipboard.set_text(text).map(|()| None),
                        }
                        .map_err(|e| e.to_string())
                    })();
                    reply(token, result);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { requests })
    }

    pub fn submit(&self, token: T, operation: Operation) -> Result<(), String> {
        self.requests
            .try_send((token, operation))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "Clipboard is busy; try again".into(),
                mpsc::TrySendError::Disconnected(_) => "Clipboard worker stopped".into(),
            })
    }
}
