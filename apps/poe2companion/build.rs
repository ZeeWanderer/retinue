use std::{env, fs, path::PathBuf};

fn protocol_define(source: &str, name: &str) -> u32 {
    source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(&format!("-define({name},"))?
                .strip_suffix(").")?
                .trim()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("missing or invalid daemon protocol define {name}"))
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let version_file = manifest.join("../../VERSION");
    println!("cargo:rerun-if-changed={}", version_file.display());
    let version = fs::read_to_string(version_file).expect("read root VERSION");
    assert_eq!(
        version.trim(),
        env::var("CARGO_PKG_VERSION").unwrap(),
        "apps/poe2companion/Cargo.toml version must match root VERSION"
    );

    let protocol_file = manifest.join("../retinue_daemon/src/runtime/retinue_local_protocol.erl");
    println!("cargo:rerun-if-changed={}", protocol_file.display());
    let protocol = fs::read_to_string(protocol_file).expect("read daemon protocol");
    assert_eq!(
        protocol_define(&protocol, "ENVELOPE_VERSION"),
        2,
        "poe2companion supports envelope 2; review the client before changing its contract"
    );
    assert_eq!(
        protocol_define(&protocol, "INTERFACE_SESSION"),
        1,
        "poe2companion supports session 1; review the client before changing its contract"
    );
}
