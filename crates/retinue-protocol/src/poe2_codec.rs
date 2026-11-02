//! Lossless Serde bridges for the game-owned PoE2 protobuf schema.
//!
//! The generator must not derive Serde for Nullable*, *List, Number, or
//! EventValue. Other PoE2 messages use deny_unknown_fields and `present` on
//! optional scalars and singular messages, with skip_serializing_if for None.

use crate::{invalid, poe2, wire};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser};
use serde_json::Value;
use std::{fmt, io};

/// A missing field defaults to None; an explicit null is decoded by T itself.
pub fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub fn encode_request(mut value: Value) -> io::Result<wire::Packet> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("PoE2 request must be an object"))?;
    let id = object.remove("id").map_or(Ok(0), |id| {
        id.as_u64().ok_or_else(|| invalid("invalid request ID"))
    })?;
    let op = object
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("missing PoE2 operation"))?;
    let (name, dataset) = match op {
        "get" => ("GetSessionRequest", Some("session")),
        "subscribe" => ("SubscribeSessionRequest", Some("session")),
        "unsubscribe" => ("UnsubscribeSessionRequest", None),
        "publish" if object.get("dataset").and_then(Value::as_str) == Some("checks") => {
            ("PublishChecksRequest", Some("checks"))
        }
        "publish" => ("PublishSessionRequest", Some("session")),
        "trade_item" => ("TradeItemRequest", None),
        "trade_search" => ("TradeSearchRequest", None),
        "trade_fetch" => ("TradeFetchRequest", None),
        "trade_restore" => ("TradeRestoreRequest", None),
        "trade_leagues" => ("TradeLeaguesRequest", None),
        "trade_predict" => ("TradePredictRequest", None),
        "currency_asset" => ("CurrencyAssetRequest", None),
        _ => return Err(invalid("unsupported PoE2 operation")),
    };
    if let Some(dataset) = dataset {
        if object
            .get("dataset")
            .is_some_and(|value| value.as_str() != Some(dataset))
        {
            return Err(invalid("invalid PoE2 request dataset"));
        }
        object.insert("dataset".into(), dataset.into());
    }
    let type_name = format!("retinue.poe2.v1.{name}");
    let payload = crate::encode_value(&type_name, value)?.into();
    Ok(wire::Packet {
        body: Some(wire::packet::Body::Request(wire::Request {
            id,
            type_name,
            payload,
        })),
    })
}

pub fn decode_value(type_name: &str, bytes: &[u8]) -> io::Result<Value> {
    // Generated schema dispatch uses the implementations below for nested data.
    crate::decode_value(type_name, bytes)
}

macro_rules! nullable {
    ($name:ident, $module:ident, $inner:ty) => {
        impl Serialize for poe2::$name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use poe2::$module::Kind;
                match &self.kind {
                    Some(Kind::Value(value)) => value.serialize(serializer),
                    Some(Kind::Null(true)) => serializer.serialize_none(),
                    _ => Err(ser::Error::custom("invalid PoE2 nullable branch")),
                }
            }
        }

        impl<'de> Deserialize<'de> for poe2::$name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                use poe2::$module::Kind;
                let kind = match Option::<$inner>::deserialize(deserializer)? {
                    Some(value) => Kind::Value(value),
                    None => Kind::Null(true),
                };
                Ok(Self { kind: Some(kind) })
            }
        }
    };
}

nullable!(NullableString, nullable_string, String);
nullable!(NullableBool, nullable_bool, bool);
nullable!(NullableUint32, nullable_uint32, u32);
nullable!(NullableUint64, nullable_uint64, u64);
nullable!(NullableSint32, nullable_sint32, i32);
nullable!(NullableSint64, nullable_sint64, i64);
nullable!(NullableProcess, nullable_process, poe2::Process);
nullable!(NullablePlayer, nullable_player, poe2::Player);
nullable!(NullableArea, nullable_area, poe2::Area);
nullable!(NullableIdentity, nullable_identity, poe2::Identity);
nullable!(NullableSocial, nullable_social, poe2::Social);
nullable!(NullableEntities, nullable_entities, poe2::Entities);
nullable!(NullableSnapshot, nullable_snapshot, poe2::Snapshot);
nullable!(NullableLevelRange, nullable_level_range, poe2::LevelRange);
nullable!(NullableTradeItem, nullable_trade_item, poe2::TradeItem);
nullable!(
    NullableTradeResults,
    nullable_trade_results,
    poe2::TradeResults
);
nullable!(NullableEstimate, nullable_estimate, poe2::Estimate);

macro_rules! list {
    ($name:ident, $inner:ty) => {
        impl Serialize for poe2::$name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.values.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for poe2::$name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(Self {
                    values: Vec::<$inner>::deserialize(deserializer)?,
                })
            }
        }
    };
}

list!(StringList, String);
list!(NumberList, poe2::Number);
list!(PartyMemberList, poe2::PartyMember);
list!(UniqueMonsterList, poe2::UniqueMonster);
list!(IssueList, poe2::Issue);
list!(SessionEventList, poe2::SessionEvent);
list!(FilterList, poe2::Filter);
list!(PropertyList, poe2::Property);
list!(MagnitudeList, poe2::Magnitude);
list!(ModifierList, poe2::Modifier);
list!(ConditionList, poe2::Condition);
list!(ChoiceList, poe2::Choice);
list!(LeagueList, poe2::League);
list!(RateWindowList, poe2::RateWindow);
list!(ListingList, poe2::Listing);

impl Serialize for poe2::Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use poe2::number::Kind;
        match &self.kind {
            Some(Kind::Integer(value)) => serializer.serialize_i64(*value),
            Some(Kind::Unsigned(value)) => serializer.serialize_u64(*value),
            Some(Kind::Real(value)) if value.is_finite() => serializer.serialize_f64(*value),
            Some(Kind::Null(true)) => serializer.serialize_none(),
            _ => Err(ser::Error::custom("invalid PoE2 number branch")),
        }
    }
}

impl<'de> Deserialize<'de> for poe2::Number {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NumberVisitor;
        impl de::Visitor<'_> for NumberVisitor {
            type Value = poe2::number::Kind;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an integer, finite real number, or null")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(poe2::number::Kind::Integer(value))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(match i64::try_from(value) {
                    Ok(value) => poe2::number::Kind::Integer(value),
                    Err(_) => poe2::number::Kind::Unsigned(value),
                })
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if value.is_finite() {
                    Ok(poe2::number::Kind::Real(value))
                } else {
                    Err(E::custom("non-finite PoE2 number"))
                }
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(poe2::number::Kind::Null(true))
            }
        }
        Ok(Self {
            kind: Some(deserializer.deserialize_any(NumberVisitor)?),
        })
    }
}

impl Serialize for poe2::EventValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use poe2::event_value::Kind;
        match &self.kind {
            Some(Kind::Null(true)) => serializer.serialize_none(),
            Some(Kind::Integer(value)) => value.serialize(serializer),
            Some(Kind::Boolean(value)) => value.serialize(serializer),
            Some(Kind::State(value)) => value.serialize(serializer),
            Some(Kind::Area(value)) => value.serialize(serializer),
            Some(Kind::Character(value)) => value.serialize(serializer),
            _ => Err(ser::Error::custom("invalid PoE2 event value branch")),
        }
    }
}

impl<'de> Deserialize<'de> for poe2::EventValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use poe2::event_value::Kind;
        let kind = match Value::deserialize(deserializer)? {
            Value::Null => Kind::Null(true),
            Value::Bool(value) => Kind::Boolean(value),
            Value::Number(value) => Kind::Integer(
                value
                    .as_i64()
                    .ok_or_else(|| de::Error::custom("invalid PoE2 event integer"))?,
            ),
            Value::Object(value) if value.contains_key("scene") => Kind::State(
                serde_json::from_value(Value::Object(value)).map_err(de::Error::custom)?,
            ),
            Value::Object(value) if value.contains_key("in_town") => {
                Kind::Area(serde_json::from_value(Value::Object(value)).map_err(de::Error::custom)?)
            }
            Value::Object(value) if value.contains_key("class") => Kind::Character(
                serde_json::from_value(Value::Object(value)).map_err(de::Error::custom)?,
            ),
            _ => return Err(de::Error::custom("invalid PoE2 event value")),
        };
        Ok(Self { kind: Some(kind) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prost::Message;
    use serde_json::json;

    fn request(value: Value) -> wire::Request {
        let packet = encode_request(value).unwrap();
        match packet.body.unwrap() {
            wire::packet::Body::Request(request) => request,
            _ => panic!("expected request"),
        }
    }

    fn roundtrip(name: &str, value: Value) {
        let name = format!("retinue.poe2.v1.{name}");
        let bytes = crate::encode_value(&name, value.clone()).unwrap();
        assert_eq!(decode_value(&name, &bytes).unwrap(), value);
    }

    #[test]
    fn request_mappings_and_outer_id() {
        for (op, expected) in [
            ("get", "GetSessionRequest"),
            ("subscribe", "SubscribeSessionRequest"),
            ("unsubscribe", "UnsubscribeSessionRequest"),
            ("publish", "PublishSessionRequest"),
            ("trade_item", "TradeItemRequest"),
            ("trade_search", "TradeSearchRequest"),
            ("trade_fetch", "TradeFetchRequest"),
            ("trade_restore", "TradeRestoreRequest"),
            ("trade_leagues", "TradeLeaguesRequest"),
            ("trade_predict", "TradePredictRequest"),
            ("currency_asset", "CurrencyAssetRequest"),
        ] {
            let packet = request(json!({"id":u64::MAX,"op":op,"game":"poe2"}));
            assert_eq!(packet.id, u64::MAX);
            assert_eq!(packet.type_name, format!("retinue.poe2.v1.{expected}"));
            let value = decode_value(&packet.type_name, &packet.payload).unwrap();
            assert!(value.get("id").is_none());
            assert_eq!(value["op"], op);
            assert_eq!(value["game"], "poe2");
        }
    }

    #[test]
    fn search_defaults_null_and_empty_are_distinct() {
        let packet = request(json!({"op":"trade_search","base":false,"rarity":null,
            "currency":"","conditions":[],"filters":[{"id":"stat","min":0,"max":1.0}],
            "item_level":{"min":1,"max":null}}));
        let model = poe2::TradeSearchRequest::decode(packet.payload.as_ref()).unwrap();
        assert!(model.id.is_none());
        assert!(model.conditions.is_some());
        assert_eq!(
            model.rarity.unwrap().kind,
            Some(poe2::nullable_string::Kind::Null(true))
        );
        let value = decode_value(&packet.type_name, &packet.payload).unwrap();
        assert_eq!(value["rarity"], Value::Null);
        assert!(value.get("status").is_none());
        assert_eq!(value["conditions"], json!([]));
        assert_eq!(value["item_level"], json!({"min":1,"max":null}));
        assert!(value["filters"][0]["min"].as_number().unwrap().is_i64());
        assert!(value["filters"][0]["max"].as_number().unwrap().is_f64());
    }

    #[test]
    fn erlang_presence_wire_vectors() {
        let name = "retinue.poe2.v1.TradeSearchRequest";
        for (bytes, expected) in [
            (vec![], json!({})),
            (vec![106, 0], json!({"conditions":[]})),
            (vec![98, 0], json!({"filters":[]})),
            (vec![50, 2, 16, 1], json!({"rarity":null})),
            (vec![50, 2, 10, 0], json!({"rarity":""})),
            (vec![88, 0], json!({"base":false})),
        ] {
            assert_eq!(decode_value(name, &bytes).unwrap(), expected);
            assert_eq!(crate::encode_value(name, expected).unwrap(), bytes);
        }
        let bytes = [16, 255, 255, 255, 255, 255, 255, 255, 255, 255, 1];
        assert_eq!(
            decode_value("retinue.poe2.v1.TradeLeaguesRequest", &bytes).unwrap(),
            json!({"id":u64::MAX})
        );
    }

    #[test]
    fn publication_and_account_data() {
        let data = json!({"stream":"stream","sequence":u64::MAX,"observed_at":9007199254740993u64,
            "status":"observed","error":null,
            "process":{"pid":u32::MAX,"started":u64::MAX,"executable":"/game/example.exe","image_base":u64::MAX},
            "snapshot":{"scene":"in_game","states":[],
                "player":{"name":"Example","class":null,"level":1,"experience":u32::MAX,"gold":i32::MIN,"life":0},
                "area":null,"panels":{"stash":null,"inventory":false},"issues":[],
                "social":{"local":{"character":"Example","account":"Example#1234","league":"Standard"},
                    "party":[{"character":"Other","account":"Other#5678","league":null,"class_id":0,"level":null}]},
                "entities":null}});
        let packet = request(json!({"op":"publish","dataset":"session","data":data}));
        assert_eq!(
            decode_value(&packet.type_name, &packet.payload).unwrap()["data"],
            data
        );
    }

    #[test]
    fn checks_document_preserves_raw_extensions() {
        let data = json!({"context":{"session":null},"checks":{"native_state":{
            "checked_at":9007199254740993u64,"state":"pass","error":null}},
            "diagnostics":[null,{"address":u64::MAX}]});
        let packet = request(json!({"op":"publish","dataset":"checks","data":data}));
        assert_eq!(packet.type_name, "retinue.poe2.v1.PublishChecksRequest");
        assert_eq!(
            decode_value(&packet.type_name, &packet.payload).unwrap()["data"],
            data
        );
    }

    #[test]
    fn session_event_variants() {
        for (kind, to) in [
            ("state", json!({"scene":"in_game","states":[]})),
            (
                "area",
                json!({"name":"Area","in_town":false,"change_key":1}),
            ),
            ("character", json!({"name":"Example","class":null})),
            ("xp", json!(u32::MAX)),
            ("level", json!(100)),
            ("gold", json!(i32::MIN)),
            ("panel", json!(false)),
            ("death", json!(0)),
            ("unique_monster_death", json!(0)),
        ] {
            roundtrip(
                "Session",
                json!({"events":[{"kind":kind,"from":null,"to":to}]}),
            );
        }
    }

    #[test]
    fn number_representation_and_boundaries() {
        for number in [
            json!(0),
            json!(0.0),
            json!(1.0),
            json!(-1.25),
            json!(i64::MIN),
            json!(i64::MAX),
            json!(u64::MAX),
            json!(9007199254740993u64),
        ] {
            roundtrip(
                "TradePrediction",
                json!({"estimate":{"amount":number,"currency":"exalted"}}),
            );
        }
        roundtrip(
            "TradePrediction",
            json!({"estimate":null,"reason":"unsupported_item"}),
        );
        roundtrip(
            "TradeRestoration",
            json!({"results":null,"auto_search_allowed":false}),
        );
        roundtrip("SessionPublished", json!({"revision":0}));
        roundtrip("SessionPublished", json!({"revision":1,"unchanged":false}));
    }

    #[test]
    fn trade_reply_shapes_and_enrichment() {
        let item = json!({"name":"","base":"Ring","rarity":"Rare","item_class":null,
            "item_level":77,"category":"accessory.ring","ranking_status":"ready",
            "base_required":false,"auto_search":false,"profiles":["allmods","fuzzy"],
            "default_profile":"allmods","properties":[{"text":"Quality: +20%"}],
            "modifiers":[{"key":"stat","text":"+10 to Life","section":"explicit","id":null,
                "min":10,"max":10.0,"supported":false,"resolution":"unknown","selected":false,
                "quick_selected":false,"exact_selected":false,"base_selected":false,
                "ranges":{"allmods":{"min":null},"fuzzy":{}},
                "magnitudes":[{"value":10,"min":8,"max":12}],
                "decorator":{"raw":"{Prefix Modifier}","group":0,"name":"Example",
                    "affix":"Prefix","tier":2,"tags":["Life"]},"display_template":"+# to Life",
                "display_values":[10,1.0],"negated":false,"ranking_score":2.5,
                "internal_ids":["base_maximum_life"],"tier_info":{"current":2,"count":3,"max":1}}],
            "conditions":[{"id":null,"text":"Unmodifiable","filters":{}}],
            "corrupted":false,"unidentified":false});
        let budget = json!({"observed_at_ms":9007199254740993u64,
            "windows":[{"usage":0.0,"expires_in_ms":1000}],"retry_after_ms":0});
        roundtrip(
            "TradeEvaluation",
            json!({"item":item,"leagues":[],"league":"",
            "search_options":{"status":[{"id":null,"text":"Any"}],"rarity":[],"currency":[],"indexed":[]},
            "prediction_available":false,"rate_budget":budget}),
        );
        roundtrip(
            "TradeLeagues",
            json!({"leagues":[{"id":"Standard","text":"Standard"}],
            "league":"Standard","rate_budget":budget}),
        );
        let listing = json!({"id":"listing","name":"","base":"Ring","item_level":77,
            "gem_level":null,"required_level":0,"gem_sockets":null,"item":item,"type":"secure",
            "quality":0,"indexed_at":-1,"amount":1,"currency":"exalted","account":"Example",
            "online":false,"indexed":"1969-12-31T23:59:59Z"});
        let results = json!({"search_id":"query","url":"https://example.invalid/query",
            "total":9007199254740993u64,"returned":1,"next_offset":null,
            "listings":[listing],"rate_budget":budget});
        roundtrip("TradeResults", results.clone());
        roundtrip(
            "TradePage",
            json!({"search_id":"query","offset":0,"next_offset":null,
            "listings":[listing],"rate_budget":budget}),
        );
        roundtrip(
            "TradeRestoration",
            json!({"results":results,"auto_search_allowed":false}),
        );
        roundtrip(
            "TradeError",
            json!({"reason":"trade_busy","retry_after_seconds":0,"rate_budget":budget}),
        );
        roundtrip(
            "CurrencyAsset",
            json!({"id":"exalted","text":"Exalted Orb","image":"/image/example.png",
            "tier":null,"digest":"a","media_type":"image/png","byte_size":0,"path":"/cache/example.png"}),
        );
    }

    #[test]
    fn malformed_requests_and_branches_fail() {
        for value in [
            json!([]),
            json!({}),
            json!({"op":"unknown"}),
            json!({"op":"get","dataset":"other"}),
            json!({"op":"get","id":null}),
            json!({"op":"trade_search","text":null}),
            json!({"op":"trade_search","conditions":null}),
            json!({"op":"trade_search","unexpected":true}),
            json!({"op":"publish","data":{"unexpected":true}}),
        ] {
            assert!(encode_request(value).is_err());
        }
        for bytes in [&[50, 0][..], &[50, 2, 16, 0], &[255]] {
            assert!(decode_value("retinue.poe2.v1.TradeSearchRequest", bytes).is_err());
        }
        assert!(
            serde_json::to_value(poe2::Number {
                kind: Some(poe2::number::Kind::Real(f64::INFINITY)),
            })
            .is_err()
        );
        assert!(decode_value("retinue.poe2.v1.Unknown", &[]).is_err());
    }
}
