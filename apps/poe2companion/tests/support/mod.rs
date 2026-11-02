use std::time::Duration;

use retinue_protocol::{encode_value, native, prost::Message, wire};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

pub async fn read(reader: &mut BufReader<OwnedReadHalf>) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        if reader.fill_buf().await.unwrap().first() == Some(&b'{') {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).await.unwrap() > 0);
            return serde_json::from_str(&line).unwrap();
        }
        let size = reader.read_u32().await.unwrap() as usize;
        assert!((1..=128 * 1024).contains(&size));
        let mut bytes = vec![0; size];
        reader.read_exact(&mut bytes).await.unwrap();
        let Some(wire::packet::Body::Request(request)) =
            wire::Packet::decode(bytes.as_slice()).unwrap().body
        else {
            panic!("expected request")
        };
        native::request_value(request).unwrap()
    })
    .await
    .unwrap()
}

pub async fn write(writer: &mut OwnedWriteHalf, value: Value) {
    let bytes = if value.get("compatible").is_some() {
        format!("{value}\n").into_bytes()
    } else {
        let packet = if let Some(data) = value.get("data") {
            let name = if data.get("leagues").is_some() {
                "TradeLeagues"
            } else if data.get("estimate").is_some() {
                "TradePrediction"
            } else {
                "SessionPublished"
            };
            let type_name = format!("retinue.poe2.v1.{name}");
            wire::Packet {
                body: Some(wire::packet::Body::Reply(wire::DataReply {
                    id: value["id"].as_u64().unwrap(),
                    dataset: value["dataset"].as_str().unwrap().into(),
                    payload: encode_value(&type_name, data.clone()).unwrap().into(),
                    type_name,
                })),
            }
        } else {
            native::control(&value).unwrap()
        };
        native::frame(&packet, 128 * 1024).unwrap()
    };
    writer.write_all(&bytes).await.unwrap();
}

pub async fn accept(listener: &UnixListener) -> (BufReader<OwnedReadHalf>, OwnedWriteHalf) {
    accept_with_timeout(listener, Duration::from_secs(8)).await
}

pub async fn accept_with_timeout(
    listener: &UnixListener,
    timeout: Duration,
) -> (BufReader<OwnedReadHalf>, OwnedWriteHalf) {
    let (stream, _) = tokio::time::timeout(timeout, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let hello = read(&mut reader).await;
    assert_eq!(hello["op"], "hello");
    assert_eq!(hello["id"], 1);
    assert_eq!(hello["client"], "poe2companion");
    assert_eq!(hello["game"], "poe2");
    assert_eq!(hello["interfaces"], json!({"session":2,"native":1}));
    assert_eq!(hello["transport"], "protobuf");
    assert_eq!(hello["envelope"], 2);
    assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
    assert!(matches!(hello["mode"].as_str(), Some("watch" | "launch")));
    write(&mut writer, json!({"id":1,"ok":true,"compatible":true,"game":"poe2","envelope":2,"transport":"protobuf","interfaces":{"session":2,"native":1}})).await;
    (reader, writer)
}

pub async fn acknowledge(writer: &mut OwnedWriteHalf, id: &Value) {
    write(
        writer,
        json!({"id":id,"ok":true,"dataset":"session_published","data":{"revision":1}}),
    )
    .await;
}
