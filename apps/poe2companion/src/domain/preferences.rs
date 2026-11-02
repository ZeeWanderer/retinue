use retinue_client::paths::{Base, Game, Paths, create_private_dir_all};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub(crate) league: String,
    pub(crate) scale: f32,
    pub(crate) status: String,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            league: String::new(),
            scale: 1.0,
            status: "securable".into(),
        }
    }
}
impl Preferences {
    fn path() -> PathBuf {
        Paths::from_env()
            .game_directory(Base::Config, Game::Poe2)
            .join("companion/evaluate.json")
    }
    pub(crate) fn load() -> Self {
        let mut preferences = std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .unwrap_or_default();
        if !preferences.scale.is_finite() {
            preferences.scale = 1.0;
        }
        preferences.scale = preferences.scale.clamp(0.75, 2.0);
        preferences
    }
    pub(crate) fn save(&self) -> Result<(), String> {
        let path = Self::path();
        create_private_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .and_then(|_| std::fs::rename(&tmp, &path))
            .map_err(|e| e.to_string())
    }
}
