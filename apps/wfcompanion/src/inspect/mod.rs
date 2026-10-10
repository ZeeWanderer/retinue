pub mod cache;
pub mod capture;
pub mod combat;
pub mod daemon;
pub mod equipment;
pub mod events;
pub mod gep;
pub mod luau;
pub mod memory;
pub mod query;
pub mod watch;

pub fn unix_time_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub mod report;
