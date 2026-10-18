use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

pub async fn read(reader: &mut BufReader<OwnedReadHalf>) -> Value {
    let mut line = String::new();
    let size = tokio::time::timeout(Duration::from_secs(8), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(size > 0, "daemon client disconnected before its request");
    serde_json::from_str(&line).unwrap()
}

pub async fn write(writer: &mut OwnedWriteHalf, value: Value) {
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
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
    assert_eq!(hello["interfaces"], json!({"session":2}));
    assert_eq!(hello["envelope"], 2);
    assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
    assert!(matches!(hello["mode"].as_str(), Some("watch" | "launch")));
    write(&mut writer, json!({"id":1,"ok":true,"compatible":true,"game":"poe2","envelope":2,"interfaces":{"session":2}})).await;
    (reader, writer)
}

pub async fn acknowledge(writer: &mut OwnedWriteHalf, id: &Value) {
    write(
        writer,
        json!({"id":id,"ok":true,"dataset":"session","data":{"revision":1}}),
    )
    .await;
}
