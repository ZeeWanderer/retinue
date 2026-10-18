fn main() {
    let version = std::fs::read_to_string("../../VERSION").expect("read root VERSION");
    assert_eq!(version.trim(), std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rerun-if-changed=../../VERSION");
    println!("cargo:rustc-env=WFCLI_VERSION={}", version.trim());
}
