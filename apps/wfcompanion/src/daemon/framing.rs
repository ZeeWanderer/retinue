use std::io;

use bytes::{Buf, BytesMut};
use retinue_protocol::{prost::Message, wire};
use tokio_util::codec::{Decoder, LinesCodec};

pub(super) const MAX_SERVER_FRAME_BYTES: usize = 64 * 1024 * 1024;

pub(super) struct ServerFrames {
    lines: LinesCodec,
    protobuf: bool,
}

impl ServerFrames {
    pub(super) fn new() -> Self {
        Self {
            lines: LinesCodec::new_with_max_length(MAX_SERVER_FRAME_BYTES),
            protobuf: false,
        }
    }

    pub(super) fn enable_protobuf(&mut self) {
        self.protobuf = true;
    }
}

// Keep Decoder's strict EOF check; LinesCodec accepts an unterminated final line.
impl Decoder for ServerFrames {
    type Item = serde_json::Value;
    type Error = io::Error;

    fn decode(&mut self, bytes: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        if self.protobuf {
            if bytes.len() < 4 {
                return Ok(None);
            }
            let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            if length == 0 || length > MAX_SERVER_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid native frame length",
                ));
            }
            if bytes.len() < 4 + length {
                return Ok(None);
            }
            bytes.advance(4);
            let packet = wire::Packet::decode(bytes.split_to(length).freeze())?;
            return retinue_protocol::native::value(packet).map(Some);
        }
        self.lines
            .decode(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .map(|line| {
                let message: serde_json::Value = serde_json::from_str(&line)?;
                if !message.is_object() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "daemon frame is not an object",
                    ));
                }
                Ok(message)
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    use tokio_util::codec::FramedRead;

    fn native_frames() -> ServerFrames {
        let mut frames = ServerFrames::new();
        frames.enable_protobuf();
        frames
    }

    fn ack(id: u64) -> Vec<u8> {
        retinue_protocol::native::frame(
            &wire::Packet {
                body: Some(wire::packet::Body::Ack(wire::Ack {
                    id,
                    revision: Some(42),
                })),
            },
            MAX_SERVER_FRAME_BYTES,
        )
        .unwrap()
    }

    #[test]
    fn rejects_malformed_oversized_and_truncated_frames() {
        for frame in [b"{invalid}\n".as_slice(), b"[]\n", b"\xff\n", b"{\"id\":1}"] {
            let mut frames = ServerFrames::new();
            assert!(frames.decode_eof(&mut BytesMut::from(frame)).is_err());
        }
        let mut frames = ServerFrames {
            lines: LinesCodec::new_with_max_length(8),
            protobuf: false,
        };
        assert!(
            frames
                .decode(&mut BytesMut::from(&b"123456789"[..]))
                .is_err()
        );
        assert!(
            ServerFrames::new()
                .decode_eof(&mut BytesMut::new())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn limit_is_per_frame_and_includes_exact_boundary() {
        let frame = b"{\"id\":1}\n{\"id\":2}\n";
        let mut frames = ServerFrames {
            lines: LinesCodec::new_with_max_length(8),
            protobuf: false,
        };
        let mut bytes = BytesMut::from(&frame[..]);
        assert_eq!(frames.decode(&mut bytes).unwrap().unwrap()["id"], 1);
        assert_eq!(frames.decode(&mut bytes).unwrap().unwrap()["id"], 2);
        assert!(bytes.is_empty());
    }

    #[test]
    fn handshake_preserves_buffered_native_frames() {
        let hello = serde_json::json!({
            "id": 1, "ok": true, "compatible": true,
            "transport": "protobuf", "interfaces": {"native": 1}
        });
        let mut bytes = BytesMut::from(format!("{hello}\n").as_bytes());
        bytes.extend_from_slice(&ack(7));
        bytes.extend_from_slice(&ack(8));
        let mut frames = ServerFrames::new();
        assert_eq!(frames.decode(&mut bytes).unwrap().unwrap(), hello);
        frames.enable_protobuf();
        for id in [7, 8] {
            let value = frames.decode(&mut bytes).unwrap().unwrap();
            assert_eq!(value["id"], id);
            assert_eq!(value["ok"], true);
            assert_eq!(value["revision"], 42);
        }
        assert!(bytes.is_empty());
        assert!(frames.decode_eof(&mut bytes).unwrap().is_none());
    }

    #[test]
    fn native_fragments_retain_header_payload_and_following_frames() {
        let frame = ack(7);
        for split in 1..frame.len() {
            let mut frames = native_frames();
            let mut bytes = BytesMut::from(&frame[..split]);
            assert!(
                frames.decode(&mut bytes).unwrap().is_none(),
                "split {split}"
            );
            assert_eq!(bytes.as_ref(), &frame[..split]);
            bytes.extend_from_slice(&frame[split..]);
            bytes.extend_from_slice(&ack(8));
            assert_eq!(frames.decode(&mut bytes).unwrap().unwrap()["id"], 7);
            assert_eq!(frames.decode(&mut bytes).unwrap().unwrap()["id"], 8);
            assert!(bytes.is_empty());
        }
    }

    #[test]
    fn native_eof_rejects_every_incomplete_header_and_payload() {
        let frame = ack(7);
        for split in 1..frame.len() {
            assert!(
                native_frames()
                    .decode_eof(&mut BytesMut::from(&frame[..split]))
                    .is_err(),
                "split {split}"
            );
        }
        let mut frames = native_frames();
        let mut bytes = BytesMut::from(frame.as_slice());
        assert_eq!(frames.decode_eof(&mut bytes).unwrap().unwrap()["id"], 7);
        assert!(frames.decode_eof(&mut bytes).unwrap().is_none());

        let mut bytes = BytesMut::from(frame.as_slice());
        bytes.extend_from_slice(&frame[..frame.len() - 1]);
        assert_eq!(frames.decode_eof(&mut bytes).unwrap().unwrap()["id"], 7);
        assert!(frames.decode_eof(&mut bytes).is_err());
    }

    #[test]
    fn native_lengths_are_bounded_before_reading_the_payload() {
        for length in [0, MAX_SERVER_FRAME_BYTES as u32 + 1, u32::MAX] {
            let mut bytes = BytesMut::from(length.to_be_bytes().as_slice());
            assert_eq!(
                native_frames().decode(&mut bytes).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        for length in [1, MAX_SERVER_FRAME_BYTES as u32] {
            let mut bytes = BytesMut::from(length.to_be_bytes().as_slice());
            assert!(native_frames().decode(&mut bytes).unwrap().is_none());
            assert_eq!(bytes.len(), 4);
        }
    }

    #[test]
    fn native_frames_reject_malformed_payloads_and_client_requests() {
        for frame in [
            &[0, 0, 0, 1, 0xff][..],
            &[0, 0, 0, 1, 0][..],
            &[0, 0, 0, 2, 0x32, 0][..],
            b"{\"id\":7,\"ok\":true}\n",
        ] {
            assert!(native_frames().decode(&mut BytesMut::from(frame)).is_err());
        }
        let request = retinue_protocol::native::request(
            "warframe",
            serde_json::json!({"id": 7, "op": "get", "dataset": "player"}),
        )
        .unwrap();
        let frame = retinue_protocol::native::frame(&request, MAX_SERVER_FRAME_BYTES).unwrap();
        let error = native_frames()
            .decode(&mut BytesMut::from(frame.as_slice()))
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("unexpected request from daemon"));
    }

    #[tokio::test]
    async fn cancelled_receive_retains_fragments() {
        let (mut writer, reader) = tokio::io::duplex(128);
        let mut frames = FramedRead::new(reader, ServerFrames::new());
        writer.write_all(b"{\"id\":").await.unwrap();
        tokio::select! {
            biased;
            result = frames.next() => panic!("partial frame completed: {result:?}"),
            _ = std::future::ready(()) => {}
        }
        writer.write_all(b"7}\n{\"id\":8}\n").await.unwrap();
        writer.shutdown().await.unwrap();
        assert_eq!(frames.next().await.unwrap().unwrap()["id"], 7);
        assert_eq!(frames.next().await.unwrap().unwrap()["id"], 8);
        assert!(frames.next().await.is_none());
    }

    #[tokio::test]
    async fn cancelled_native_receive_retains_header_and_payload_fragments() {
        let frame = ack(7);
        for split in 1..frame.len() {
            let (mut writer, reader) = tokio::io::duplex(128);
            let mut frames = FramedRead::new(reader, native_frames());
            writer.write_all(&frame[..split]).await.unwrap();
            tokio::select! {
                biased;
                result = frames.next() => panic!("partial frame completed at {split}: {result:?}"),
                _ = std::future::ready(()) => {}
            }
            assert_eq!(frames.read_buffer().as_ref(), &frame[..split]);
            writer.write_all(&frame[split..]).await.unwrap();
            writer.write_all(&ack(8)).await.unwrap();
            writer.shutdown().await.unwrap();
            assert_eq!(frames.next().await.unwrap().unwrap()["id"], 7);
            assert_eq!(frames.next().await.unwrap().unwrap()["id"], 8);
            assert!(frames.next().await.is_none());
        }
    }
}
