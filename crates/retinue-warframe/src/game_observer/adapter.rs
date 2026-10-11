use serde::{Deserialize, Serialize};
use std::path::Path;

use super::ProcessIdentity;

#[derive(Clone, Copy, Debug)]
pub(crate) struct GameAdapter {
    pub(crate) id: &'static str,
    pub(crate) sha256: &'static str,
    pub(crate) scaleform: Option<ScaleformLayout>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ScaleformLayout {
    pub(crate) registry_vector_rva: u64,
    pub(crate) flash_instance_type_rva: u64,
    pub(crate) flash_instance_vtable_rva: u64,
    pub(crate) root_vtable_rva: u64,
    pub(crate) root_secondary_vtable_rva: u64,
    pub(crate) container_vtable_rva: u64,
    pub(crate) container_secondary_vtable_rva: u64,
    pub(crate) text_vtable_rva: u64,
    pub(crate) text_secondary_vtable_rva: u64,
}

const D01B: GameAdapter = GameAdapter {
    id: "d01b5cb5cff5",
    sha256: "d01b5cb5cff51afc5ffb7d3af051674aafa84000bee764780ff71d9d073cad93",
    scaleform: Some(ScaleformLayout {
        registry_vector_rva: 0x028a_5410,
        flash_instance_type_rva: 0x0294_e130,
        flash_instance_vtable_rva: 0x0222_00f8,
        root_vtable_rva: 0x0222_44b8,
        root_secondary_vtable_rva: 0x0222_4580,
        container_vtable_rva: 0x0222_5588,
        container_secondary_vtable_rva: 0x0222_5878,
        text_vtable_rva: 0x0222_77d8,
        text_secondary_vtable_rva: 0x0222_7ac0,
    }),
};

const ADAPTERS: &[GameAdapter] = &[D01B];

pub fn list() -> serde_json::Value {
    ADAPTERS.iter().map(|adapter| {
        serde_json::json!({"id": adapter.id, "executable_sha256": adapter.sha256, "domains": ["scaleform"]})
    }).collect()
}

pub fn inspect_executable(path: &Path) -> Result<serde_json::Value, String> {
    let (hash, bytes) = super::executable::read(path)?;
    Ok(serde_json::json!({
        "executable_sha256": hash,
        "account": discovery(super::account::layout::discover(&bytes)),
        "http": discovery(super::gep::layout::discover(&bytes)),
        "inventory": discovery(super::inventory::layout::discover(&bytes)),
        "metadata": discovery(super::metadata::inspect_image(&bytes)),
        "scaleform": discovery(super::ui::layout::discover(&bytes)),
        "scaleform_adapter": resolve(&hash).map(|adapter| adapter.id),
    }))
}

fn discovery<T: Serialize>(result: Result<T, String>) -> serde_json::Value {
    match result {
        Ok(bindings) => serde_json::json!({"status": "available", "bindings": bindings}),
        Err(reason) => serde_json::json!({"status": "unavailable", "reason": reason}),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AdapterSupport {
    Supported {
        id: &'static str,
        capabilities: &'static [&'static str],
    },
    Unsupported {
        reason: String,
    },
    NotProbed,
}

pub(crate) fn resolve(sha256: &str) -> Option<&'static GameAdapter> {
    ADAPTERS.iter().find(|adapter| sha256 == adapter.sha256)
}

pub(crate) fn resolve_key(key: &str) -> Option<&'static GameAdapter> {
    ADAPTERS
        .iter()
        .find(|adapter| key == adapter.id || key == adapter.sha256)
}

pub(crate) fn require_scaleform(identity: &ProcessIdentity) -> Result<ScaleformLayout, String> {
    let (hash, bytes) = super::executable::read(&identity.executable.path)?;
    if hash != identity.executable.sha256 {
        return Err("Warframe executable changed since process identification".into());
    }
    super::ui::layout::discover(&bytes)
}

pub(crate) fn replay_scaleform(
    identity: Option<&ProcessIdentity>,
    recorded: Option<ScaleformLayout>,
) -> Result<ScaleformLayout, String> {
    if let Some(layout) = recorded {
        layout.validate()?;
        return Ok(layout);
    }
    let identity = identity.ok_or("capture has no executable identity")?;
    if let Some(layout) = resolve(&identity.executable.sha256).and_then(|adapter| adapter.scaleform)
    {
        return Ok(layout);
    }
    require_scaleform(identity)
}

pub(crate) fn layout_support(result: &Result<ScaleformLayout, String>) -> AdapterSupport {
    match result {
        Ok(_) => AdapterSupport::Supported {
            id: "validated_bindings",
            capabilities: &["scaleform_ui_v1"],
        },
        Err(reason) => AdapterSupport::Unsupported {
            reason: reason.clone(),
        },
    }
}

impl ScaleformLayout {
    fn validate(self) -> Result<(), String> {
        let values = [
            self.registry_vector_rva,
            self.flash_instance_type_rva,
            self.flash_instance_vtable_rva,
            self.root_vtable_rva,
            self.root_secondary_vtable_rva,
            self.container_vtable_rva,
            self.container_secondary_vtable_rva,
            self.text_vtable_rva,
            self.text_secondary_vtable_rva,
        ];
        if values
            .iter()
            .any(|&rva| !(0x1000..0xffff_ff00).contains(&rva) || !rva.is_multiple_of(8))
        {
            return Err("invalid recorded Scaleform bindings".into());
        }
        Ok(())
    }
}

pub fn support(identity: Option<&ProcessIdentity>) -> AdapterSupport {
    match identity {
        Some(identity) => match resolve(&identity.executable.sha256) {
            Some(adapter) => AdapterSupport::Supported {
                id: adapter.id,
                capabilities: &["scaleform_ui_v1"],
            },
            None => AdapterSupport::NotProbed,
        },
        None => AdapterSupport::Unsupported {
            reason: "capture has no executable identity".to_owned(),
        },
    }
}

#[cfg(test)]
pub(crate) fn test_scaleform() -> ScaleformLayout {
    D01B.scaleform.unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_observer::ExecutableIdentity;
    use std::path::PathBuf;

    #[test]
    fn resolves_only_exact_executable_identity() {
        let identity = ProcessIdentity {
            pid: 1,
            executable: ExecutableIdentity {
                path: PathBuf::from("Warframe.x64.exe"),
                size: 1,
                modified_unix_ms: None,
                sha256: D01B.sha256.to_owned(),
            },
        };
        assert_eq!(resolve(&identity.executable.sha256).unwrap().id, D01B.id);

        let mut unknown = identity;
        unknown.executable.sha256 = "unknown".to_owned();
        assert!(resolve(&unknown.executable.sha256).is_none());
    }

    #[test]
    fn unknown_build_does_not_enable_unverified_scaleform() {
        let identity = ProcessIdentity {
            pid: 1,
            executable: ExecutableIdentity {
                path: PathBuf::from("Warframe.x64.exe"),
                size: 1,
                modified_unix_ms: None,
                sha256: "unknown".to_owned(),
            },
        };
        assert!(require_scaleform(&identity).is_err());
        assert!(matches!(
            support(Some(&identity)),
            AdapterSupport::NotProbed
        ));
        assert_eq!(list()[0]["domains"], serde_json::json!(["scaleform"]));
    }

    #[test]
    fn recorded_bindings_do_not_require_an_installed_executable() {
        let expected = test_scaleform();
        assert_eq!(replay_scaleform(None, Some(expected)).unwrap(), expected);
        let mut invalid = expected;
        invalid.text_vtable_rva = u64::MAX;
        assert!(
            replay_scaleform(None, Some(invalid))
                .unwrap_err()
                .contains("invalid recorded")
        );
    }
}
