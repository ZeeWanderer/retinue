use crate::{decode_value, encode_value, invalid, prost::Message, wire};
use serde_json::{Value, json};
use std::io;

pub fn typed_request<T: Message + prost::Name>(id: u64, request: &T) -> wire::Packet {
    wire::Packet {
        body: Some(wire::packet::Body::Request(wire::Request {
            id,
            type_name: T::full_name(),
            payload: request.encode_to_vec().into(),
        })),
    }
}

pub fn request(game: &str, mut value: Value) -> io::Result<wire::Packet> {
    if game == "poe2" {
        return crate::poe2_codec::encode_request(value);
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("request must be an object"))?;
    let id = object.remove("id").map_or(Ok(0), |v| {
        v.as_u64().ok_or_else(|| invalid("invalid request ID"))
    })?;
    let name = match game {
        "warframe" => "retinue.warframe.native.v1.WarframeRequest",
        _ => return Err(invalid("unsupported native game")),
    };
    if object.get("limit").is_some_and(|v| v == "all") {
        object.remove("limit");
        object.insert("all".into(), Value::Bool(true));
    }
    if let Some(patch) = object.get_mut("patch").and_then(Value::as_object_mut)
        && patch.get("instance_id") == Some(&Value::Null)
    {
        patch.remove("instance_id");
        patch.insert("clear_instance".into(), Value::Bool(true));
    }
    Ok(wire::Packet {
        body: Some(wire::packet::Body::Request(wire::Request {
            id,
            type_name: name.into(),
            payload: encode_value(name, value)?.into(),
        })),
    })
}

pub fn frame(packet: &wire::Packet, limit: usize) -> io::Result<Vec<u8>> {
    let size = packet.encoded_len();
    if size == 0 || size > limit || size > u32::MAX as usize {
        return Err(invalid("native frame exceeds limit"));
    }
    let mut bytes = Vec::with_capacity(size + 4);
    bytes.extend_from_slice(&(size as u32).to_be_bytes());
    packet.encode(&mut bytes).map_err(invalid)?;
    Ok(bytes)
}

pub fn request_value(request: wire::Request) -> io::Result<Value> {
    let mut value = decode_value(&request.type_name, &request.payload)?;
    if !value.is_object() {
        return Err(invalid("request payload must be an object"));
    }
    value["id"] = request.id.into();
    Ok(value)
}

pub fn value(packet: wire::Packet) -> io::Result<Value> {
    use wire::packet::Body;
    match packet.body.ok_or_else(|| invalid("empty native packet"))? {
        Body::Reply(reply) => Ok(json!({"id":reply.id,"ok":true,"dataset":reply.dataset,
            "data":decode_value(&reply.type_name, &reply.payload)?})),
        Body::Ack(ack) => {
            let mut value = json!({"id":ack.id,"ok":true});
            if let Some(revision) = ack.revision {
                value["revision"] = revision.into();
            }
            Ok(value)
        }
        Body::Failure(failure) => {
            let error = failure
                .error
                .ok_or_else(|| invalid("missing daemon error"))?;
            Ok(
                json!({"id":failure.id,"ok":false,"error":serde_json::from_str::<Value>(&error.json).map_err(invalid)?}),
            )
        }
        Body::Event(event) => {
            let mut value = json!({"event":event.event});
            let field = if event.event == "companion_diagnostics" {
                "request"
            } else {
                "data"
            };
            value[field] = decode_value(&event.type_name, &event.payload)?;
            if let Some(v) = event.subscription {
                value["subscription"] = v.into();
            }
            if let Some(v) = event.source {
                value["source"] = v.into();
            }
            if let Some(v) = event.action {
                value["action"] = v.into();
            }
            if let Some(v) = event.request_id {
                value["request_id"] = v.into();
            }
            if let Some(v) = event.dataset {
                value["dataset"] = v.into();
            }
            if let Some(v) = event.game {
                value["game"] = v.into();
            }
            Ok(value)
        }
        Body::Request(_) => Err(invalid("unexpected request from daemon")),
    }
}

pub fn control(value: &Value) -> io::Result<wire::Packet> {
    let id = value["id"]
        .as_u64()
        .ok_or_else(|| invalid("missing reply ID"))?;
    let body = if value["ok"] == false {
        wire::packet::Body::Failure(wire::Failure {
            id,
            error: Some(wire::Document {
                json: value["error"].to_string(),
            }),
        })
    } else if value["ok"] == true && value.get("data").is_none() {
        wire::packet::Body::Ack(wire::Ack {
            id,
            revision: value["revision"].as_u64(),
        })
    } else {
        return Err(invalid("typed reply schema is required"));
    };
    Ok(wire::Packet { body: Some(body) })
}
