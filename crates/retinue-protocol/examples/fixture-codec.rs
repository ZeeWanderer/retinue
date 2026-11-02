use retinue_protocol::{native, prost::Message, wire};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn convert(value: Value) -> Result<Value, Box<dyn std::error::Error>> {
    if let Some(request) = value.get("encode") {
        let packet = native::request(
            value["game"].as_str().ok_or("missing game")?,
            request.clone(),
        )?;
        return Ok(json!({"bytes": native::frame(&packet, 1024 * 1024)?}));
    }
    let bytes: Vec<u8> = serde_json::from_value(value["decode"].clone())?;
    Ok(json!({"value": native::value(wire::Packet::decode(bytes.as_slice())?)?}))
}

fn main() -> io::Result<()> {
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let response = serde_json::from_str(&line?)
            .map_err(|e| e.to_string())
            .and_then(|value| convert(value).map_err(|e| e.to_string()))
            .unwrap_or_else(|error| json!({"error": error}));
        writeln!(output, "{response}")?;
        output.flush()?;
    }
    Ok(())
}
