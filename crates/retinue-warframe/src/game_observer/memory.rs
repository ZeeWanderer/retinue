use std::path::PathBuf;

pub use retinue_native::process::{
    CaptureBlock, ExecutableIdentity, ProcessIdentity, ProcessMemory, Region, ScanRange,
};

pub fn identify_process(pid: u32) -> Result<ProcessIdentity, String> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps"))
        .map_err(|error| format!("could not read Warframe maps: {error}"))?;
    let path = game_image(&retinue_native::process::parse_regions(&maps))
        .ok_or("Warframe executable is absent from process maps")?;
    retinue_native::process::identify_executable(pid, path)
}

fn game_image(regions: &[Region]) -> Option<PathBuf> {
    regions
        .iter()
        .find(|region| {
            region
                .path
                .to_ascii_lowercase()
                .ends_with("/warframe.x64.exe")
        })
        .map(|region| PathBuf::from(&region.path))
}

pub(crate) fn with_game_image(memory: ProcessMemory) -> ProcessMemory {
    let image = game_image(memory.regions());
    memory.with_image_path(image)
}

pub fn open(pid: u32) -> Result<ProcessMemory, String> {
    ProcessMemory::open(pid).map(with_game_image)
}

pub fn from_capture(
    pid: u32,
    maps: String,
    blocks: Vec<CaptureBlock>,
    bytes: Vec<u8>,
) -> Result<ProcessMemory, String> {
    ProcessMemory::from_capture(pid, maps, blocks, bytes).map(with_game_image)
}

#[cfg(test)]
pub fn from_test_bytes(bytes: &[u8], regions: Vec<Region>) -> ProcessMemory {
    with_game_image(ProcessMemory::from_test_bytes(bytes, regions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_pe_identity_is_game_owned() {
        let regions = retinue_native::process::parse_regions(
            "140000000-141000000 r-xp 0 00:00 1 /games/Steam\\040Library/Warframe.x64.exe\n",
        );
        assert_eq!(
            game_image(&regions),
            Some("/games/Steam Library/Warframe.x64.exe".into())
        );
    }
}
