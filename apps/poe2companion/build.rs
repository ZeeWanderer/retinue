use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[path = "build/support.rs"]
mod support;

fn chrome_digests(manifest: &Path, output: &Path) {
    let mut generated = String::from("fn source_digest(name: &str) -> [u8; 32] { match name {\n");
    for name in [
        "normal",
        "magic",
        "rare",
        "unique",
        "gem",
        "currency",
        "quest",
        "rare-double",
        "unique-double",
        "search-left-idle",
        "search-right-idle",
        "search-left-highlight",
        "search-right-highlight",
        "profile-radio",
        "profile-radio-selected",
    ] {
        let files = if name.starts_with("search-") || name.starts_with("profile-") {
            vec![format!("{name}.png")]
        } else {
            ["left", "center", "right"]
                .map(|cap| format!("{name}-{cap}.png"))
                .to_vec()
        };
        let mut hash = Sha256::new();
        for file in files {
            let path = manifest.join("assets/frames").join(file);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = fs::read(path).expect("read embedded chrome");
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        let digest: [u8; 32] = hash.finalize().into();
        generated.push_str(&format!("{name:?} => {digest:?},\n"));
    }
    generated.push_str("_ => panic!(\"unknown chrome source\"),\n}}\n");
    fs::write(output.join("chrome-digests.rs"), generated).expect("write chrome digests");
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    support::prepare(&manifest, &env::var("CARGO_PKG_VERSION").unwrap(), &out_dir);
    chrome_digests(&manifest, &out_dir);
}
