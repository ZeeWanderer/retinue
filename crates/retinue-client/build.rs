use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let protocol_file =
        manifest_dir.join("../../apps/retinue_daemon/src/runtime/retinue_local_protocol.erl");
    let protocol_source = fs::read_to_string(&protocol_file).expect("read Retinue local protocol");
    let interfaces = protocol_interfaces(&protocol_source);
    let mut generated_protocol = format!(
        "pub const ENVELOPE_VERSION: u32 = {};\n",
        protocol_define(&protocol_source, "ENVELOPE_VERSION")
    );
    for (constant, _, version) in &interfaces {
        generated_protocol.push_str(&format!("pub const {constant}: u32 = {version};\n"));
    }
    generated_protocol.push_str("pub const INTERFACES: &[(&str, u32)] = &[\n");
    for (_, name, version) in &interfaces {
        generated_protocol.push_str(&format!("    (\"{name}\", {version}),\n"));
    }
    generated_protocol.push_str("];\n");
    println!("cargo:rerun-if-changed={}", protocol_file.display());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out_dir.join("local_protocol.rs"), generated_protocol)
        .expect("write generated local protocol constants");
}

fn protocol_define(source: &str, name: &str) -> u32 {
    source
        .lines()
        .find_map(|line| {
            line.strip_prefix(&format!("-define({name},"))
                .and_then(|value| value.strip_suffix(")."))
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or_else(|| panic!("parse Retinue protocol define {name}"))
}

fn protocol_interfaces(source: &str) -> Vec<(String, String, u32)> {
    let warframe = source
        .split_once("\ninterfaces() ->")
        .and_then(|(_, body)| body.split_once("}."))
        .map(|(body, _)| body)
        .expect("Warframe interface map must be present");
    source
        .lines()
        .filter_map(|line| {
            let value = line.strip_prefix("-define(INTERFACE_")?;
            let (name, version) = value.strip_suffix(").")?.split_once(',')?;
            let constant = format!("INTERFACE_{}", name.trim());
            if !warframe.contains(&format!("?{constant}")) {
                return None;
            }
            let wire_name = name.trim().to_ascii_lowercase();
            let version = version.trim().parse().ok()?;
            Some((constant, wire_name, version))
        })
        .collect()
}
