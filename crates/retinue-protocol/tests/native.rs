use retinue_protocol::{native, prost::Message, warframe_native::WarframeRequest, wire};
use serde_json::json;

#[test]
fn mutations_preserve_absence_zero_false_and_clear() {
    let packet = native::request(
        "warframe",
        json!({"id":u64::MAX,
        "op":"build_group_update","group_id":"test","revision":0,
        "patch":{"instance_id":null,"quantity":0,"visible":false}}),
    )
    .unwrap();
    let Some(wire::packet::Body::Request(envelope)) = packet.body else {
        panic!("not a request")
    };
    assert_eq!(envelope.id, u64::MAX);
    let request = WarframeRequest::decode(envelope.payload).unwrap();
    assert_eq!(request.revision, Some(0));
    let patch = request.patch.unwrap();
    assert_eq!(patch.clear_instance, Some(true));
    assert_eq!(patch.visible, Some(false));
    assert_eq!(patch.quantity, Some(0));
    assert_eq!(patch.platinum, None);
}

#[test]
fn requests_reject_unknown_fields_and_invalid_numbers() {
    for value in [
        json!({"id":1,"op":"get","unexpected":true}),
        json!({"id":-1,"op":"get"}),
        json!({"id":1,"op":"market_order_update","patch":{"quantitty":2}}),
        json!({"id":1,"op":"market_order_update","patch":{"quantity":-1}}),
    ] {
        assert!(native::request("warframe", value).is_err());
    }
    assert!(native::request("unknown", json!({"op":"get"})).is_err());
}

#[test]
fn notification_mode_updates_do_not_clear_filters() {
    for (fissures, present) in [
        (json!({"mode":"gui"}), false),
        (json!({"mode":"gui","filters":[]}), true),
    ] {
        let packet = native::request(
            "warframe",
            json!({"op":"notification_settings_set","fissures":fissures}),
        )
        .unwrap();
        let Some(wire::packet::Body::Request(envelope)) = packet.body else {
            panic!("not a request")
        };
        let request = WarframeRequest::decode(envelope.payload.clone()).unwrap();
        assert_eq!(request.fissures.unwrap().filters.is_some(), present);
        assert_eq!(
            native::request_value(envelope).unwrap()["fissures"],
            fissures
        );
    }
}

#[test]
fn documents_preserve_unknown_capture_fields_and_large_integers() {
    let data =
        json!({"new_game_field":{"id":u64::MAX,"fraction":0.12345678901234567},"empty":null});
    let packet = native::request(
        "warframe",
        json!({"id":5,"op":"publish",
        "dataset":"game_metadata","data":data}),
    )
    .unwrap();
    let Some(wire::packet::Body::Request(request)) = packet.body else {
        panic!("not a request")
    };
    assert_eq!(native::request_value(request).unwrap()["data"], data);
}

#[test]
fn framing_and_direction_are_explicit() {
    let request = native::request("warframe", json!({"id":1,"op":"get"})).unwrap();
    let bytes = native::frame(&request, 1024).unwrap();
    assert_eq!(
        u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize,
        bytes.len() - 4
    );
    assert!(native::frame(&request, 1).is_err());
    assert!(native::value(request).is_err());
    assert!(native::value(wire::Packet::default()).is_err());
    assert!(native::control(&json!({"id":1,"ok":true,"data":{"anything":true}})).is_err());
}
