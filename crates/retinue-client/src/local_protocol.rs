use std::collections::BTreeMap;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/local_protocol.rs"));

pub fn interfaces() -> BTreeMap<&'static str, u32> {
    INTERFACES.iter().copied().collect()
}

pub fn companion_interfaces() -> BTreeMap<&'static str, u32> {
    BTreeMap::from([
        ("assets", INTERFACE_ASSETS),
        ("checks", INTERFACE_CHECKS),
        ("datasets", INTERFACE_DATASETS),
        ("diagnostics", INTERFACE_DIAGNOSTICS),
        ("game_metadata", INTERFACE_GAME_METADATA),
        ("market", INTERFACE_MARKET),
        ("player", INTERFACE_PLAYER),
        ("relics", INTERFACE_RELICS),
    ])
}

pub fn socket_path() -> PathBuf {
    crate::paths::Paths::from_env().daemon_socket().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_uses_shared_retinue_paths() {
        assert_eq!(
            socket_path(),
            crate::paths::Paths::from_env().daemon_socket()
        );
    }

    #[test]
    fn inspector_contract_covers_daemon_interfaces() {
        let names = interfaces().into_keys().collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "assets",
                "build_stats",
                "builds",
                "checks",
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
