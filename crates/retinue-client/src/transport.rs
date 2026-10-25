use std::io;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    UnixStream,
    unix::{OwnedReadHalf, OwnedWriteHalf},
};

/// Bounded frames; subscription clients dispatch replies and events by ID.
pub struct JsonChannel {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    limit: usize,
    pending: Vec<u8>,
}

impl JsonChannel {
    pub fn new(stream: UnixStream, limit: usize) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            reader: BufReader::new(reader),
            writer,
            limit,
            pending: Vec::new(),
        }
    }

    pub async fn request(&mut self, request: &Value) -> io::Result<Value> {
        let id = request
            .get("id")
            .filter(|id| !id.is_null())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "request needs an ID"))?;
        self.send(request).await?;
        let reply = self.receive().await?;
        if reply.get("id") != Some(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "daemon reply ID does not match outstanding request",
            ));
        }
        Ok(reply)
    }

    pub async fn send(&mut self, request: &Value) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(request)?;
        bytes.push(b'\n');
        if bytes.len() > self.limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "request exceeds frame limit",
            ));
        }
        self.writer.write_all(&bytes).await
    }

    /// Cancellation preserves a partial frame for the next receive.
    pub async fn receive(&mut self) -> io::Result<Value> {
        loop {
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "daemon disconnected",
                ));
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(available.len(), |index| index + 1);
            if self.pending.len() + count > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "daemon frame exceeds limit",
                ));
            }
            self.pending.extend_from_slice(&available[..count]);
            self.reader.consume(count);
            if newline.is_some() {
                let frame = std::mem::take(&mut self.pending);
                return serde_json::from_slice(&frame).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid daemon JSON frame")
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn cancelled_receive_retains_partial_event() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut channel = JsonChannel::new(client, 128);
        server.write_all(b"{\"event\":").await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), channel.receive())
                .await
                .is_err()
        );
        server.write_all(b"\"player\"}\n").await.unwrap();
        assert_eq!(channel.receive().await.unwrap(), json!({"event":"player"}));
    }

    #[tokio::test]
    async fn split_and_buffered_frames_preserve_boundaries() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut channel = JsonChannel::new(client, 128);
        let sender = tokio::spawn(async move {
            server.write_all(b"{\"id\":").await.unwrap();
            tokio::task::yield_now().await;
            server
                .write_all(b"1,\"ok\":true}\n{\"id\":2}\n")
                .await
                .unwrap();
            server
        });
        assert_eq!(channel.request(&json!({"id":1})).await.unwrap()["ok"], true);
        let _server = sender.await.unwrap();
        assert_eq!(
            channel.request(&json!({"id":2})).await.unwrap(),
            json!({"id":2})
        );
    }

    #[tokio::test]
    async fn rejects_oversized_malformed_mismatched_and_truncated_frames() {
        for (payload, kind) in [
            ("x".repeat(129), io::ErrorKind::InvalidData),
            ("not json\n".into(), io::ErrorKind::InvalidData),
            ("{\"id\":2}\n".into(), io::ErrorKind::InvalidData),
            ("{\"id\":1}".into(), io::ErrorKind::UnexpectedEof),
        ] {
            let (client, mut server) = UnixStream::pair().unwrap();
            server.write_all(payload.as_bytes()).await.unwrap();
            server.shutdown().await.unwrap();
            assert_eq!(
                JsonChannel::new(client, 128)
                    .request(&json!({"id":1}))
                    .await
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
    }

    #[tokio::test]
    async fn validates_requests_before_writing() {
        let (client, _server) = UnixStream::pair().unwrap();
        let mut channel = JsonChannel::new(client, 16);
        for request in [
            json!({}),
            json!({"id":null}),
            json!({"id":1,"data":"x".repeat(20)}),
        ] {
            assert_eq!(
                channel.request(&request).await.unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
