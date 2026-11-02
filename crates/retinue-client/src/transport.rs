use std::io;

use bytes::Bytes;
use retinue_protocol::{
    native,
    prost::{Message, Name},
    wire,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{
    UnixStream,
    unix::{OwnedReadHalf, OwnedWriteHalf},
};

/// Bounded frames; subscription clients dispatch replies and events by ID.
pub struct Channel {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    limit: usize,
    pending: Vec<u8>,
    protobuf: bool,
    game: String,
}

#[derive(Debug)]
pub enum Packet {
    Json(Value),
    Data(wire::DataReply),
}

impl Packet {
    pub fn id(&self) -> Option<u64> {
        match self {
            Self::Json(value) => value["id"].as_u64(),
            Self::Data(value) => Some(value.id),
        }
    }

    pub fn json(self) -> io::Result<Value> {
        match self {
            Self::Json(value) => Ok(value),
            Self::Data(reply) => native::value(wire::Packet {
                body: Some(wire::packet::Body::Reply(reply)),
            }),
        }
    }

    pub fn decode<T: Message + Default + Name>(self) -> io::Result<T> {
        match self {
            Self::Data(reply) if reply.type_name == T::full_name() => {
                T::decode(reply.payload).map_err(|_| invalid("invalid typed reply payload"))
            }
            Self::Json(reply) if reply["ok"] == false => {
                Err(invalid(reply["error"].as_str().unwrap_or("request failed")))
            }
            _ => Err(invalid("daemon reply schema mismatch")),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl Channel {
    pub fn new(stream: UnixStream, limit: usize) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            reader: BufReader::new(reader),
            writer,
            limit,
            pending: Vec::new(),
            protobuf: false,
            game: String::new(),
        }
    }

    pub async fn request(&mut self, request: &Value) -> io::Result<Value> {
        self.request_packet(request).await?.json()
    }

    pub async fn request_packet(&mut self, request: &Value) -> io::Result<Packet> {
        let id = request.get("id").and_then(Value::as_u64).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "request needs an unsigned ID")
        })?;
        self.send(request).await?;
        let reply = self.receive_packet().await?;
        if reply.id() != Some(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "daemon reply ID does not match outstanding request",
            ));
        }
        Ok(reply)
    }

    pub async fn send(&mut self, request: &Value) -> io::Result<()> {
        let mut bytes;
        if self.protobuf {
            let packet = if request.get("op").is_some() {
                native::request(&self.game, request.clone())?
            } else {
                native::control(request)?
            };
            bytes = packet.encode_to_vec();
        } else {
            bytes = serde_json::to_vec(request)?;
            bytes.push(b'\n');
        }
        self.write_frame(&bytes).await
    }

    pub async fn send_data<T: Message + Name>(&mut self, id: u64, data: &T) -> io::Result<()> {
        if !self.protobuf {
            return Err(invalid("typed replies require protobuf transport"));
        }
        let reply = wire::DataReply {
            id,
            type_name: T::full_name(),
            payload: data.encode_to_vec().into(),
            dataset: String::new(),
        };
        self.send_packet(&wire::Packet {
            body: Some(wire::packet::Body::Reply(reply)),
        })
        .await
    }

    pub async fn send_packet(&mut self, packet: &wire::Packet) -> io::Result<()> {
        if !self.protobuf {
            return Err(invalid("native packets require protobuf transport"));
        }
        self.write_frame(&packet.encode_to_vec()).await
    }

    async fn write_frame(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > self.limit || bytes.len() > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "request exceeds frame limit",
            ));
        }
        if self.protobuf {
            self.writer
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .await?;
        }
        self.writer.write_all(bytes).await
    }

    pub fn enable_protobuf(&mut self, game: &str) -> io::Result<()> {
        if !self.pending.is_empty() || self.protobuf {
            return Err(invalid("invalid transport transition"));
        }
        self.protobuf = true;
        self.game = game.to_owned();
        Ok(())
    }

    /// Cancellation preserves a partial frame for the next receive.
    pub async fn receive(&mut self) -> io::Result<Value> {
        self.receive_packet().await?.json()
    }

    pub async fn receive_packet(&mut self) -> io::Result<Packet> {
        self.receive_frame(false).await
    }

    pub async fn receive_request(&mut self) -> io::Result<Value> {
        self.receive_frame(true).await?.json()
    }

    async fn receive_frame(&mut self, request: bool) -> io::Result<Packet> {
        loop {
            if self.protobuf && self.pending.len() >= 4 {
                let size = u32::from_be_bytes(self.pending[..4].try_into().unwrap()) as usize;
                if size == 0 || size > self.limit {
                    return Err(invalid("daemon frame exceeds limit"));
                }
                if self.pending.len() == size + 4 {
                    let bytes = Bytes::from(std::mem::take(&mut self.pending)).slice(4..);
                    let packet = wire::Packet::decode(bytes)
                        .map_err(|_| invalid("invalid daemon protobuf packet"))?;
                    return match packet.body {
                        Some(wire::packet::Body::Request(value)) if request => {
                            native::request_value(value).map(Packet::Json)
                        }
                        _ if request => Err(invalid("expected native request")),
                        Some(wire::packet::Body::Reply(reply)) => Ok(Packet::Data(reply)),
                        body => native::value(wire::Packet { body }).map(Packet::Json),
                    };
                }
            }
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "daemon disconnected",
                ));
            }
            let newline = (!self.protobuf)
                .then(|| available.iter().position(|byte| *byte == b'\n'))
                .flatten();
            let count = if self.protobuf {
                let end = if self.pending.len() < 4 {
                    4
                } else {
                    4 + u32::from_be_bytes(self.pending[..4].try_into().unwrap()) as usize
                };
                available.len().min(end - self.pending.len())
            } else {
                newline.map_or(available.len(), |index| index + 1)
            };
            if !self.protobuf && self.pending.len() + count > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "daemon frame exceeds limit",
                ));
            }
            self.pending.extend_from_slice(&available[..count]);
            self.reader.consume(count);
            if newline.is_some() {
                let frame = std::mem::take(&mut self.pending);
                return serde_json::from_slice(&frame)
                    .map(Packet::Json)
                    .map_err(|_| {
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
    async fn protobuf_switch_keeps_buffered_frames_and_partial_reads() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(client, 1024);
        let data = retinue_protocol::warframe::FoundrySnapshot {
            revision: 7,
            ..Default::default()
        };
        let reply = wire::Packet {
            body: Some(wire::packet::Body::Reply(wire::DataReply {
                dataset: "foundry".into(),
                id: 10,
                type_name: retinue_protocol::warframe::FoundrySnapshot::full_name(),
                payload: data.encode_to_vec().into(),
            })),
        }
        .encode_to_vec();
        let prefix = (reply.len() as u32).to_be_bytes();
        server
            .write_all(b"{\"id\":1,\"transport\":\"protobuf\"}\n")
            .await
            .unwrap();
        server.write_all(&prefix[..2]).await.unwrap();
        assert_eq!(channel.receive().await.unwrap()["id"], 1);
        channel.enable_protobuf("warframe").unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                channel.receive_packet()
            )
            .await
            .is_err()
        );
        server.write_all(&prefix[2..]).await.unwrap();
        server.write_all(&reply[..3]).await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                channel.receive_packet()
            )
            .await
            .is_err()
        );
        server.write_all(&reply[3..]).await.unwrap();
        let packet = channel.receive_packet().await.unwrap();
        assert_eq!(packet.id(), Some(10));
        assert_eq!(
            packet
                .decode::<retinue_protocol::warframe::FoundrySnapshot>()
                .unwrap(),
            data
        );
    }

    #[tokio::test]
    async fn protobuf_events_schema_errors_and_limits() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut client = Channel::new(a, 128);
        let mut server = Channel::new(b, 128);
        client.enable_protobuf("warframe").unwrap();
        server.enable_protobuf("warframe").unwrap();
        let event = wire::Packet {
            body: Some(wire::packet::Body::Event(wire::Event {
                event: "player".into(),
                type_name: retinue_protocol::warframe_native::PlayerSnapshot::full_name(),
                ..Default::default()
            })),
        };
        server
            .writer
            .write_all(&native::frame(&event, 128).unwrap())
            .await
            .unwrap();
        server
            .send_data(4, &retinue_protocol::warframe::Equipment::default())
            .await
            .unwrap();
        assert_eq!(client.receive().await.unwrap()["event"], "player");
        assert!(
            client
                .receive_packet()
                .await
                .unwrap()
                .decode::<retinue_protocol::warframe::FoundrySnapshot>()
                .is_err()
        );
        server
            .writer
            .write_all(&129u32.to_be_bytes())
            .await
            .unwrap();
        assert_eq!(
            client.receive_packet().await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let (a, mut b) = UnixStream::pair().unwrap();
        let mut client = Channel::new(a, 128);
        client.enable_protobuf("warframe").unwrap();
        b.write_all(&[0, 0, 0, 5, 0x12]).await.unwrap();
        drop(b);
        assert_eq!(
            client.receive_packet().await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[tokio::test]
    async fn cancelled_receive_retains_partial_event() {
        let (client, mut server) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(client, 128);
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
        let mut channel = Channel::new(client, 128);
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
                Channel::new(client, 128)
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
        let mut channel = Channel::new(client, 16);
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
