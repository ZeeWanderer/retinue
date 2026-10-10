use std::collections::BTreeMap;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/local_protocol.rs"));

pub fn interfaces() -> BTreeMap<&'static str, u32> {
    INTERFACES.iter().copied().collect()
}

pub fn companion_interfaces() -> BTreeMap<&'static str, u32> {
    BTreeMap::from([
        ("assets", INTERFACE_ASSETS),
        ("datasets", INTERFACE_DATASETS),
        ("diagnostics", INTERFACE_DIAGNOSTICS),
        ("game_metadata", INTERFACE_GAME_METADATA),
        ("market", INTERFACE_MARKET),
        ("player", INTERFACE_PLAYER),
        ("relics", INTERFACE_RELICS),
    ])
}

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WFCLI_DAEMON_SOCKET") {
        return PathBuf::from(path);
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime).join("wfcli/wfdaemon.sock");
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."));
    cache.join("wfcli/wfdaemon.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_contract_covers_daemon_interfaces() {
        let names = interfaces().into_keys().collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "assets",
                "build_stats",
                "builds",
                "datasets",
                "diagnostics",
                "game_metadata",
                "market",
                "notifications",
                "overframe",
                "player",
                "relics",
                "worldstate",
            ]
        );
    }
}
