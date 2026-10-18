//! Shared, game-independent support for Retinue native clients.

pub mod child;
pub mod health;
pub mod local_protocol;
pub mod paths;
pub mod process;
pub mod reload;
pub mod signals;

pub fn executable_path() -> Option<&'static std::path::Path> {
    static PATH: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    // Directory exchange changes /proc/self/exe; keep helper lookup at the startup prefix.
    PATH.get_or_init(|| std::env::current_exe().ok()).as_deref()
}
