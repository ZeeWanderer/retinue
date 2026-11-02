use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let common = root.join("apps/retinue_core/proto");
    let warframe = root.join("apps/wfcore/proto");
    let poe2 = root.join("apps/poe2core/proto");
    let sources = [
        common.join("retinue_wire.proto"),
        warframe.join("wf_views.proto"),
        warframe.join("wf_native.proto"),
        poe2.join("poe2_native.proto"),
    ];
    for file in &sources {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let descriptors =
        protox::compile(&sources, [common, warframe, poe2]).expect("compile native contracts");
    let mut config = prost_build::Config::new();
    config.bytes([".retinue.wire.v1"]);
    config.enable_type_names();
    let mut encoders = String::new();
    let mut decoders = String::new();
    for source in &sources {
        let name = source.file_name().unwrap().to_str().unwrap();
        let parsed = protox::file::File::from_source(name, &fs::read_to_string(source).unwrap())
            .expect("parse contract attributes");
        let file = parsed.file_descriptor_proto();
        let package = file.package();
        let resolved_file = descriptors.file.iter().find(|f| f.name() == name).unwrap();
        for message in &file.message_type {
            if package == "retinue.wire.v1"
                && matches!(
                    message.name(),
                    "Packet" | "Request" | "DataReply" | "Ack" | "Failure" | "Event"
                )
            {
                continue;
            }
            let full = format!("{package}.{}", message.name());
            let is_poe2 = package == "retinue.poe2.v1";
            let poe2_wrapper = is_poe2
                && (message.name().starts_with("Nullable")
                    || message.name().ends_with("List")
                    || matches!(message.name(), "Number" | "EventValue"));
            if full != "retinue.wire.v1.Document" && !poe2_wrapper {
                config.type_attribute(&full, "#[derive(serde::Serialize, serde::Deserialize)]");
                config.type_attribute(&full, "#[serde(default)]");
                if message.name() == "FissureFilterList" {
                    config.type_attribute(&full, "#[serde(transparent)]");
                }
                if is_poe2
                    || message.name().ends_with("Request")
                    || matches!(
                        message.name(),
                        "Patch"
                            | "GroupInput"
                            | "PlanOptions"
                            | "Order"
                            | "Cookie"
                            | "FissureSettings"
                            | "FissureFilter"
                    )
                {
                    config.type_attribute(&full, "#[serde(deny_unknown_fields)]");
                }
            }
            let resolved_message = resolved_file
                .message_type
                .iter()
                .find(|m| m.name() == message.name())
                .unwrap();
            for field in &message.field {
                if poe2_wrapper {
                    continue;
                }
                let resolved = resolved_message
                    .field
                    .iter()
                    .find(|f| f.number == field.number)
                    .unwrap();
                let path = format!("{full}.{}", field.name());
                if is_poe2 {
                    config.field_attribute(&path, format!("#[serde(rename = {:?})]", field.name()));
                } else if let Some(name) = &field.json_name {
                    config.field_attribute(&path, format!("#[serde(rename = {name:?})]"));
                }
                if resolved.proto3_optional.unwrap_or(false)
                    || (resolved.r#type() == prost_types::field_descriptor_proto::Type::Message
                        && resolved.label() != prost_types::field_descriptor_proto::Label::Repeated)
                {
                    config.field_attribute(
                        &path,
                        "#[serde(skip_serializing_if = \"Option::is_none\")]",
                    );
                    if is_poe2 {
                        config.field_attribute(
                            &path,
                            "#[serde(default, deserialize_with = \"crate::poe2_codec::present\")]",
                        );
                    }
                }
            }
            let rust_type = format!("crate::{}::{}", package.replace('.', "::"), message.name());
            encoders.push_str(&format!(
                "{full:?} => encode_model::<{rust_type}>(value),\n"
            ));
            decoders.push_str(&format!(
                "{full:?} => decode_model::<{rust_type}>(bytes),\n"
            ));
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("value_codec.rs"), format!(
        "pub fn encode_value(name: &str, value: serde_json::Value) -> std::io::Result<Vec<u8>> {{ match name {{ {encoders} _ => Err(invalid(\"unknown native schema\")) }} }}\n\
         pub fn decode_value(name: &str, bytes: &[u8]) -> std::io::Result<serde_json::Value> {{ match name {{ {decoders} _ => Err(invalid(\"unknown native schema\")) }} }}\n"
    )).expect("generate schema dispatch");
    config
        .compile_fds(descriptors)
        .expect("generate native contracts");
}
