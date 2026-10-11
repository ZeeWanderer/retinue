use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{info, profile::Profile};
use crate::inspect::cache;

const MAX_SCRIPT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snapshot {
    pub schema: u8,
    pub extractor_version: String,
    pub name: String,
    pub captured_unix_ms: u128,
    pub source: String,
    pub profile: Profile,
    pub packages: Vec<String>,
    pub path_filter: Option<String>,
    pub scripts: BTreeMap<String, Script>,
    pub errors: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Script {
    pub sha256: String,
    pub bytes: usize,
    pub decode_error: Option<String>,
    pub unmapped: BTreeMap<u8, usize>,
}

pub struct CaptureOptions<'a> {
    pub workspace: &'a Path,
    pub name: &'a str,
    pub cache: &'a Path,
    pub executable: &'a Path,
    pub profile: Option<Profile>,
    pub packages: Vec<String>,
    pub path_filter: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Import {
    pub profile: Profile,
    /// Resource paths mapped to bytecode files relative to the import manifest.
    pub scripts: BTreeMap<String, PathBuf>,
}

pub fn capture(
    options: CaptureOptions<'_>,
    mut progress: impl FnMut(&str, usize),
) -> Result<Snapshot, String> {
    let _lock = lock(options.workspace)?;
    let target = snapshot_path(options.workspace, options.name)?;
    if target.exists() {
        return Err("snapshot already exists; choose a new name".into());
    }
    let executable_hash = hash_file(options.executable)?;
    let profile = match options.profile {
        Some(profile) => {
            profile.require_executable(&executable_hash)?;
            profile
        }
        None => Profile::builtin(&executable_hash)
            .unwrap_or_else(|_| Profile::unmapped(executable_hash.clone())),
    };
    profile.validate()?;
    let packages = if options.packages.is_empty() {
        cache::packages(options.cache)?
    } else {
        options.packages
    };
    let before = cache_stamp(options.cache)?;
    let mut snapshot = Snapshot {
        schema: 1,
        extractor_version: env!("CARGO_PKG_VERSION").into(),
        name: options.name.into(),
        captured_unix_ms: crate::inspect::unix_time_ms(),
        source: format!("cache:{}", options.cache.display()),
        profile,
        packages: packages.clone(),
        path_filter: options.path_filter.clone(),
        scripts: BTreeMap::new(),
        errors: BTreeMap::new(),
    };
    for package in packages {
        let result = cache::visit_resources(
            options.cache,
            &package,
            |entry| {
                entry.split == 'B'
                    && entry.path.ends_with(".lua")
                    && options
                        .path_filter
                        .as_ref()
                        .is_none_or(|filter| entry.path.contains(filter))
            },
            MAX_SCRIPT_BYTES,
            |entry, result| {
                match result {
                    Ok(raw) => {
                        if snapshot.scripts.contains_key(&entry.path) {
                            snapshot.errors.insert(
                                entry.path.clone(),
                                "duplicate script path across packages".into(),
                            );
                        } else {
                            let script = store_script(options.workspace, &raw, &snapshot.profile)?;
                            snapshot.scripts.insert(entry.path.clone(), script);
                        }
                    }
                    Err(error) => {
                        snapshot.errors.insert(entry.path.clone(), error);
                    }
                }
                progress(&entry.path, snapshot.scripts.len());
                Ok(())
            },
        );
        if let Err(error) = result {
            snapshot.errors.insert(format!("package:{package}"), error);
        }
    }
    if cache_stamp(options.cache)? != before || hash_file(options.executable)? != executable_hash {
        snapshot.errors.insert(
            "source".into(),
            "game files changed during capture; repeat with a new name".into(),
        );
    }
    write_new_json(&target, &snapshot)?;
    Ok(snapshot)
}

pub fn import(workspace: &Path, name: &str, file: &Path) -> Result<Snapshot, String> {
    let _lock = lock(workspace)?;
    let target = snapshot_path(workspace, name)?;
    if target.exists() {
        return Err("snapshot already exists; choose a new name".into());
    }
    let import: Import = read_json(file)?;
    import.profile.validate()?;
    let mut snapshot = Snapshot {
        schema: 1,
        extractor_version: env!("CARGO_PKG_VERSION").into(),
        name: name.into(),
        captured_unix_ms: crate::inspect::unix_time_ms(),
        source: format!("import:{}", file.display()),
        profile: import.profile,
        packages: Vec::new(),
        path_filter: None,
        scripts: BTreeMap::new(),
        errors: BTreeMap::new(),
    };
    for (resource, path) in import.scripts {
        let path = file.parent().unwrap_or(Path::new(".")).join(path);
        let result = read_bounded(&path)
            .and_then(|bytes| store_script(workspace, &bytes, &snapshot.profile));
        match result {
            Ok(script) => {
                snapshot.scripts.insert(resource, script);
            }
            Err(error) => {
                snapshot.errors.insert(resource, error);
            }
        }
    }
    write_new_json(&target, &snapshot)?;
    Ok(snapshot)
}

pub fn load(workspace: &Path, name: &str) -> Result<Snapshot, String> {
    let snapshot: Snapshot = read_json(&snapshot_path(workspace, name)?)?;
    if snapshot.schema != 1 {
        return Err("unsupported script snapshot schema".into());
    }
    if snapshot.name != name {
        return Err("snapshot name does not match its manifest filename".into());
    }
    snapshot.profile.validate()?;
    Ok(snapshot)
}

pub fn list(workspace: &Path) -> Result<Vec<serde_json::Value>, String> {
    let mut result = Vec::new();
    for entry in fs::read_dir(workspace.join("snapshots")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("invalid snapshot filename")?;
            let snapshot = load(workspace, name)?;
            result.push(summary(&snapshot));
        }
    }
    result.sort_by_key(|entry| entry["name"].as_str().unwrap_or_default().to_owned());
    Ok(result)
}

pub fn summary(snapshot: &Snapshot) -> serde_json::Value {
    serde_json::json!({ "name": snapshot.name, "executable_sha256": snapshot.profile.executable_sha256,
        "scripts": snapshot.scripts.len(), "extraction_errors": snapshot.errors,
        "undecoded": snapshot.scripts.values().filter(|s| s.decode_error.is_some() || !s.unmapped.is_empty()).count(),
        "profile_status": snapshot.profile.status, "source": snapshot.source })
}

pub fn read_script(
    workspace: &Path,
    snapshot: &Snapshot,
    resource: &str,
) -> Result<Vec<u8>, String> {
    let script = snapshot
        .scripts
        .get(resource)
        .ok_or_else(|| format!("script not captured: {resource}"))?;
    let path = blob_path(workspace, &script.sha256)?;
    let raw = read_bounded(&path)?;
    if raw.len() != script.bytes || hash(&raw) != script.sha256 {
        return Err(format!("corrupt script blob: {resource}"));
    }
    Ok(raw)
}

fn store_script(workspace: &Path, bytes: &[u8], profile: &Profile) -> Result<Script, String> {
    if bytes.len() > MAX_SCRIPT_BYTES {
        return Err("script exceeds byte limit".into());
    }
    let sha256 = hash(bytes);
    let path = blob_path(workspace, &sha256)?;
    if path.exists() {
        if hash_file(&path)? != sha256 {
            return Err(format!("corrupt stored blob {sha256}"));
        }
    } else {
        write_new(&path, bytes)?;
    }
    let mut script = Script {
        sha256,
        bytes: bytes.len(),
        decode_error: None,
        unmapped: BTreeMap::new(),
    };
    match info(bytes, profile) {
        Ok(info) => {
            for proto in info.prototypes {
                if let Some(pc) = proto.uncertain_from_pc {
                    *script.unmapped.entry(proto.words[pc] as u8).or_default() += 1;
                }
            }
        }
        Err(error) => script.decode_error = Some(error),
    }
    Ok(script)
}

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_reader(File::open(path).map_err(|e| format!("{}: {e}", path.display()))?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn write_new_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    data.push(b'\n');
    write_new(path, &data)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let (temporary, mut file) = loop {
        let temporary = parent.join(format!(
            ".snapshot-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    // Publish complete files without replacing an existing immutable snapshot.
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .and_then(|_| fs::hard_link(&temporary, path));
    let _ = fs::remove_file(temporary);
    result.map_err(|e| format!("{}: {e}", path.display()))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(MAX_SCRIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SCRIPT_BYTES {
        return Err("script exceeds byte limit".into());
    }
    Ok(bytes)
}

struct WorkspaceLock(File);

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        // A concurrent fork can retain the file description until exec.
        let _ = self.0.unlock();
    }
}

fn lock(workspace: &Path) -> Result<WorkspaceLock, String> {
    fs::create_dir_all(workspace).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(workspace.join(".lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|e| format!("script workspace is busy: {e}"))?;
    Ok(WorkspaceLock(file))
}

fn snapshot_path(workspace: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err("snapshot name must contain only letters, digits, '-', '_' or '.'".into());
    }
    Ok(workspace.join("snapshots").join(format!("{name}.json")))
}

fn blob_path(workspace: &Path, hash: &str) -> Result<PathBuf, String> {
    if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid script blob hash".into());
    }
    Ok(workspace.join("blobs").join(hash))
}

fn cache_stamp(path: &Path) -> Result<BTreeMap<PathBuf, (u64, std::time::SystemTime)>, String> {
    fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .map(|entry| {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            Ok((
                entry.path(),
                (
                    metadata.len(),
                    metadata.modified().map_err(|e| e.to_string())?,
                ),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace(PathBuf);

    impl Workspace {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "wfinspect-corpus-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn atomic_publication_never_overwrites_existing_files() {
        let dir = Workspace::new();
        let path = dir.0.join("result.json");
        write_new(&path, b"complete").unwrap();
        assert!(write_new(&path, b"replacement").is_err());
        assert_eq!(fs::read(path).unwrap(), b"complete");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn imports_deduplicate_preserve_failures_and_detect_corruption() {
        let dir = Workspace::new();
        let profile = Profile::builtin("d01b5cb5cff5").unwrap();
        let raw = super::super::tests::fixture(0x29, 0x12345678);
        fs::write(dir.0.join("raw.bc"), &raw).unwrap();
        fs::write(dir.0.join("bad.bc"), b"unsupported layout").unwrap();
        let file = dir.0.join("import.json");
        write_new_json(
            &file,
            &Import {
                profile,
                scripts: BTreeMap::from([
                    ("/a.lua".into(), "raw.bc".into()),
                    ("/b.lua".into(), "raw.bc".into()),
                    ("/bad.lua".into(), "bad.bc".into()),
                    ("/missing.lua".into(), "missing.bc".into()),
                ]),
            },
        )
        .unwrap();
        let a = import(&dir.0, "a", &file).unwrap();
        let b = import(&dir.0, "b", &file).unwrap();
        assert_eq!(a.scripts.len(), 3);
        assert_eq!(a.errors.len(), 1);
        assert!(a.scripts["/bad.lua"].decode_error.is_some());
        assert_eq!(fs::read_dir(dir.0.join("blobs")).unwrap().count(), 2);
        assert!(import(&dir.0, "a", &file).is_err());
        assert_eq!(read_script(&dir.0, &b, "/a.lua").unwrap(), raw);
        assert_eq!(
            load(&dir.0, "a").unwrap().scripts["/a.lua"].sha256,
            hash(&raw)
        );
        assert_eq!(list(&dir.0).unwrap().len(), 2);
        let coverage = super::super::tracking::coverage(&dir.0, &a);
        assert_eq!(coverage["scripts"], 3);
        assert_eq!(coverage["fully_decoded"], 2);
        assert_eq!(coverage["atoms"]["0x12345678"]["count"], 2);
        assert_eq!(coverage["opcodes"]["0x29"]["name"], "RETURN");
        fs::write(blob_path(&dir.0, &hash(&raw)).unwrap(), b"damaged").unwrap();
        assert!(read_script(&dir.0, &a, "/a.lua").is_err());
        assert!(store_script(&dir.0, &raw, &a.profile).is_err());
    }

    #[test]
    fn rejects_renamed_manifests_and_unsupported_schemas() {
        let dir = Workspace::new();
        let file = dir.0.join("import.json");
        write_new_json(
            &file,
            &Import {
                profile: Profile::builtin("d01b5cb5cff5").unwrap(),
                scripts: BTreeMap::new(),
            },
        )
        .unwrap();
        let mut snapshot = import(&dir.0, "original", &file).unwrap();
        write_new_json(&snapshot_path(&dir.0, "renamed").unwrap(), &snapshot).unwrap();
        assert!(load(&dir.0, "renamed").unwrap_err().contains("filename"));
        assert!(list(&dir.0).is_err());
        snapshot.name = "future".into();
        snapshot.schema = 2;
        write_new_json(&snapshot_path(&dir.0, "future").unwrap(), &snapshot).unwrap();
        assert!(load(&dir.0, "future").unwrap_err().contains("schema"));
    }

    #[test]
    fn unknown_opcode_preserves_raw_and_reports_only_known_boundaries() {
        let dir = Workspace::new();
        let profile = Profile::builtin("d01b5cb5cff5").unwrap();
        let raw = super::super::tests::fixture(0xfe, 0);
        let stored = store_script(&dir.0, &raw, &profile).unwrap();
        assert!(stored.decode_error.is_none());
        assert_eq!(stored.unmapped, BTreeMap::from([(0xfe, 1)]));
        assert_eq!(
            fs::read(blob_path(&dir.0, &stored.sha256).unwrap()).unwrap(),
            raw
        );
        let _guard = lock(&dir.0).unwrap();
        assert!(lock(&dir.0).is_err());
    }

    #[test]
    fn lock_release_does_not_wait_for_inherited_descriptor() {
        let dir = Workspace::new();
        let guard = lock(&dir.0).unwrap();
        let inherited = guard.0.try_clone().unwrap();
        assert!(lock(&dir.0).is_err());
        drop(guard);
        let _next = lock(&dir.0).unwrap();
        drop(inherited);
        assert!(lock(&dir.0).is_err());
    }

    #[test]
    fn rejects_paths_outside_workspace() {
        assert!(snapshot_path(Path::new("."), "../elsewhere").is_err());
        assert!(snapshot_path(Path::new("."), "..").is_err());
        assert!(blob_path(Path::new("."), &format!("../{}", "x".repeat(61))).is_err());
    }
}
