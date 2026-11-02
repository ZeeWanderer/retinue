pub use prost;
pub mod native;
pub mod poe2_codec;

pub mod retinue {
    pub mod poe2 {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/retinue.poe2.v1.rs"));
        }
    }
    pub mod wire {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/retinue.wire.v1.rs"));
        }
    }
    pub mod warframe {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/retinue.warframe.v1.rs"));
        }
        pub mod native {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/retinue.warframe.native.v1.rs"));
            }
        }
    }
}

pub use retinue::poe2::v1 as poe2;
pub use retinue::{
    warframe::native::v1 as warframe_native, warframe::v1 as warframe, wire::v1 as wire,
};

fn invalid(message: impl ToString) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string())
}

fn encode_model<T: prost::Message + serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> std::io::Result<Vec<u8>> {
    let model: T = serde_json::from_value(value).map_err(invalid)?;
    Ok(model.encode_to_vec())
}

fn decode_model<T: prost::Message + Default + serde::Serialize>(
    bytes: &[u8],
) -> std::io::Result<serde_json::Value> {
    serde_json::to_value(T::decode(bytes).map_err(invalid)?).map_err(invalid)
}

impl serde::Serialize for wire::Document {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value =
            serde_json::from_str(&self.json).map_err(serde::ser::Error::custom)?;
        value.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for wire::Document {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self {
            json: value.to_string(),
        })
    }
}

include!(concat!(env!("OUT_DIR"), "/value_codec.rs"));
