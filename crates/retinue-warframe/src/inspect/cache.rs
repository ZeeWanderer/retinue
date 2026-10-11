// Cache structure cross-checked against MIT-licensed lotus-lib-rs; implementation is first-party.

use std::collections::HashMap;
use std::env;
use std::ffi::{OsString, c_void};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use libloading::Library;
use memchr::memmem;
use serde::Serialize;

const TOC_HEADER_SIZE: usize = 8;
const TOC_ENTRY_SIZE: usize = 96;
const MAX_BLOCK_SIZE: usize = 0x40000;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

type OodleCallback = Option<
    unsafe extern "C" fn(*mut c_void, *const u8, isize, *const u8, isize, isize, isize) -> u32,
>;
type OodleDecompress = unsafe extern "C" fn(
    *const c_void,
    isize,
    *mut c_void,
    isize,
    i32,
    i32,
    i32,
    *mut c_void,
    isize,
    OodleCallback,
    *mut c_void,
    *mut c_void,
    isize,
    i32,
) -> isize;

#[derive(Clone, Debug, Serialize)]
pub struct CachePath {
    pub split: char,
    pub path: String,
    pub compressed_size: usize,
    pub size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExtractedResource {
    pub split: char,
    pub path: String,
    pub output: PathBuf,
    pub size: usize,
}

#[derive(Clone, Debug)]
pub struct LoadedResource {
    pub split: char,
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OodleDecoderStatus {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
struct Entry {
    split: char,
    path: String,
    cache: PathBuf,
    offset: u64,
    compressed_size: usize,
    size: usize,
}

pub fn paths(cache_dir: &Path, package: &str) -> Result<Vec<CachePath>, String> {
    Ok(entries(cache_dir, package)?
        .into_iter()
        .map(|entry| CachePath {
            split: entry.split,
            path: entry.path,
            compressed_size: entry.compressed_size,
            size: entry.size,
        })
        .collect())
}

pub fn visit_resources(
    cache_dir: &Path,
    package: &str,
    select: impl Fn(&CachePath) -> bool,
    max_bytes: usize,
    mut visit: impl FnMut(CachePath, Result<Vec<u8>, String>) -> Result<(), String>,
) -> Result<(), String> {
    let mut decoder = Decoder::default();
    for entry in entries(cache_dir, package)? {
        let resource = CachePath {
            split: entry.split,
            path: entry.path.clone(),
            compressed_size: entry.compressed_size,
            size: entry.size,
        };
        if !select(&resource) {
            continue;
        }
        let data = if entry.size > max_bytes {
            Err(format!("resource exceeds {max_bytes} byte limit"))
        } else {
            decoder.read(&entry)
        };
        visit(resource, data)?;
    }
    Ok(())
}

pub fn extract(
    cache_dir: &Path,
    package: &str,
    resource: &str,
    output_prefix: &Path,
) -> Result<Vec<ExtractedResource>, String> {
    let selected = read_resource(cache_dir, package, resource)?;
    let mut result = Vec::new();
    for resource in selected {
        let output = split_output_path(output_prefix, resource.split);
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        fs::write(&output, &resource.data)
            .map_err(|error| format!("could not write {}: {error}", output.display()))?;
        result.push(ExtractedResource {
            split: resource.split,
            path: resource.path,
            output,
            size: resource.data.len(),
        });
    }
    Ok(result)
}

pub fn read_resource(
    cache_dir: &Path,
    package: &str,
    resource: &str,
) -> Result<Vec<LoadedResource>, String> {
    let resource = normalize_resource_path(resource);
    let selected = entries(cache_dir, package)?
        .into_iter()
        .filter(|entry| entry.path == resource)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(format!("resource not found in {package}: {resource}"));
    }

    let mut decoder = Decoder::default();
    let mut result = Vec::new();
    for entry in selected {
        let data = decoder.read(&entry)?;
        result.push(LoadedResource {
            split: entry.split,
            path: entry.path,
            data,
        });
    }
    Ok(result)
}

pub fn read_resource_split(
    cache_dir: &Path,
    package: &str,
    resource: &str,
    split: char,
) -> Result<LoadedResource, String> {
    if !matches!(split, 'H' | 'B' | 'F') {
        return Err(format!("invalid cache split: {split}"));
    }
    let resource = normalize_resource_path(resource);
    let entry = entries(cache_dir, package)?
        .into_iter()
        .find(|entry| entry.split == split && entry.path == resource)
        .ok_or_else(|| format!("resource not found in {split}.{package}: {resource}"))?;
    let data = Decoder::default().read(&entry)?;
    Ok(LoadedResource {
        split: entry.split,
        path: entry.path,
        data,
    })
}

#[derive(Debug, Serialize)]
pub struct SearchOptions {
    pub split: Option<char>,
    pub path: Option<String>,
    pub continue_on_error: bool,
    pub max_matches: usize,
    pub max_resource_bytes: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            split: None,
            path: None,
            continue_on_error: false,
            max_matches: 10000,
            max_resource_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SearchRecord {
    Match {
        resource: CachePath,
        offset: usize,
    },
    Error {
        split: char,
        path: String,
        error: String,
    },
}

#[derive(Default, Debug, Serialize)]
pub struct SearchSummary {
    pub resources: usize,
    pub searched_bytes: usize,
    pub matches: usize,
    pub errors: usize,
    pub truncated: bool,
}

pub fn search(
    cache_dir: &Path,
    package: &str,
    needle: &[u8],
    options: &SearchOptions,
    mut emit: impl FnMut(&SearchRecord) -> Result<(), String>,
) -> Result<SearchSummary, String> {
    if needle.is_empty() || options.max_matches == 0 || options.max_resource_bytes == 0 {
        return Err("search requires nonempty bytes and nonzero limits".into());
    }
    let mut decoder = Decoder::default();
    let mut summary = SearchSummary::default();
    for entry in entries(cache_dir, package)?.into_iter().filter(|entry| {
        options.split.is_none_or(|split| split == entry.split)
            && options
                .path
                .as_ref()
                .is_none_or(|path| entry.path.contains(path))
    }) {
        let data = if entry.size.max(entry.compressed_size) > options.max_resource_bytes {
            Err("resource exceeds --max-resource-bytes".into())
        } else {
            decoder.read(&entry)
        };
        summary.resources += 1;
        let data = match data {
            Ok(data) => data,
            Err(error) => {
                emit(&SearchRecord::Error {
                    split: entry.split,
                    path: entry.path,
                    error,
                })?;
                summary.errors += 1;
                summary.truncated = true;
                if options.continue_on_error {
                    continue;
                } else {
                    break;
                }
            }
        };
        for offset in memmem::find_iter(&data, needle) {
            emit(&SearchRecord::Match {
                resource: CachePath {
                    split: entry.split,
                    path: entry.path.clone(),
                    compressed_size: entry.compressed_size,
                    size: entry.size,
                },
                offset,
            })?;
            summary.matches += 1;
            if summary.matches == options.max_matches {
                summary.searched_bytes += offset + needle.len();
                summary.truncated = true;
                return Ok(summary);
            }
        }
        summary.searched_bytes += data.len();
    }
    Ok(summary)
}

pub fn packages(directory: &Path) -> Result<Vec<String>, String> {
    let mut packages = std::collections::BTreeSet::new();
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some((split, rest)) = name.split_once('.')
            && matches!(split, "H" | "B" | "F")
            && let Some(package) = rest.strip_suffix(".toc")
        {
            packages.insert(package.to_owned());
        }
    }
    Ok(packages.into_iter().collect())
}

pub fn locate() -> Result<PathBuf, String> {
    if let Some(directory) = env::var_os("WFINSPECT_CACHE_DIR") {
        return fs::canonicalize(directory).map_err(|e| e.to_string());
    }
    let steam = steamlocate::locate().map_err(|e| e.to_string())?;
    let (app, library) = steam
        .find_app(230410)
        .map_err(|e| e.to_string())?
        .ok_or("Warframe installation not found in Steam libraries")?;
    fs::canonicalize(library.resolve_app_dir(&app).join("Cache.Windows"))
        .map_err(|e| format!("Warframe cache directory unavailable: {e}"))
}

pub fn oodle_decoder_status() -> OodleDecoderStatus {
    match Oodle::load() {
        Ok(decoder) => OodleDecoderStatus {
            available: true,
            backend: Some(decoder.backend().to_owned()),
            source: Some(decoder.source().to_path_buf()),
            error: None,
        },
        Err(error) => OodleDecoderStatus {
            available: false,
            backend: None,
            source: None,
            error: Some(error),
        },
    }
}

fn entries(cache_dir: &Path, package: &str) -> Result<Vec<Entry>, String> {
    validate_package(package)?;
    let mut result = Vec::new();
    for split in ['H', 'B', 'F'] {
        let stem = format!("{split}.{package}");
        let toc = cache_dir.join(format!("{stem}.toc"));
        if !toc.is_file() {
            continue;
        }
        let cache = cache_dir.join(format!("{stem}.cache"));
        let bytes =
            fs::read(&toc).map_err(|error| format!("could not read {}: {error}", toc.display()))?;
        result.extend(
            parse_toc(&bytes, split, cache)
                .map_err(|error| format!("could not parse {}: {error}", toc.display()))?,
        );
    }
    if result.is_empty() {
        return Err(format!(
            "package {package} not found under {}",
            cache_dir.display()
        ));
    }
    Ok(result)
}

fn validate_package(package: &str) -> Result<(), String> {
    if package.is_empty()
        || !package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!("invalid package name: {package}"));
    }
    Ok(())
}

fn parse_toc(bytes: &[u8], split: char, cache: PathBuf) -> Result<Vec<Entry>, String> {
    let Some(entries) = bytes.get(TOC_HEADER_SIZE..) else {
        return Err("TOC is shorter than its header".to_owned());
    };
    if entries.len() % TOC_ENTRY_SIZE != 0 {
        return Err("TOC has a partial entry".to_owned());
    }

    let mut directories = vec!["/".to_owned()];
    let mut files = Vec::new();
    let mut file_indexes = HashMap::new();
    for raw in entries.chunks_exact(TOC_ENTRY_SIZE) {
        let offset = read_i64(raw, 0)?;
        let compressed_size = read_i32(raw, 16)?;
        let size = read_i32(raw, 20)?;
        let parent = read_i32(raw, 28)?;
        let parent = usize::try_from(parent).map_err(|_| "negative parent index".to_owned())?;
        let parent = directories
            .get(parent)
            .ok_or_else(|| "parent directory index is out of range".to_owned())?;
        let name_bytes = &raw[32..96];
        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name_bytes.len());
        let name = std::str::from_utf8(&name_bytes[..name_end])
            .map_err(|error| format!("invalid UTF-8 name: {error}"))?;
        if name.is_empty() {
            continue;
        }
        if name.contains(['/', '\\']) {
            return Err(format!("invalid TOC entry name: {name:?}"));
        }
        let path = join_resource_path(parent, name);
        if offset == -1 {
            directories.push(path);
            continue;
        }
        let entry = Entry {
            split,
            path: path.clone(),
            cache: cache.clone(),
            offset: u64::try_from(offset).map_err(|_| "negative cache offset".to_owned())?,
            compressed_size: usize::try_from(compressed_size)
                .map_err(|_| "negative compressed size".to_owned())?,
            size: usize::try_from(size).map_err(|_| "negative resource size".to_owned())?,
        };
        if let Some(index) = file_indexes.get(&path).copied() {
            files[index] = entry;
        } else {
            file_indexes.insert(path, files.len());
            files.push(entry);
        }
    }
    Ok(files)
}

fn join_resource_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn normalize_resource_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

fn split_output_path(prefix: &Path, split: char) -> PathBuf {
    let mut output = OsString::from(prefix.as_os_str());
    output.push(format!(".{}.raw", split.to_ascii_lowercase()));
    PathBuf::from(output)
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated i32".to_owned())?
        .try_into()
        .map_err(|_| "truncated i32".to_owned())?;
    Ok(i32::from_le_bytes(raw))
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, String> {
    let raw: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| "truncated i64".to_owned())?
        .try_into()
        .map_err(|_| "truncated i64".to_owned())?;
    Ok(i64::from_le_bytes(raw))
}

#[derive(Default)]
struct Decoder {
    oodle: Option<Oodle>,
}

impl Decoder {
    fn read(&mut self, entry: &Entry) -> Result<Vec<u8>, String> {
        let mut cache = File::open(&entry.cache)
            .map_err(|error| format!("could not open {}: {error}", entry.cache.display()))?;
        cache
            .seek(SeekFrom::Start(entry.offset))
            .map_err(|error| format!("could not seek {}: {error}", entry.cache.display()))?;
        if entry.compressed_size == entry.size {
            return read_exact(&mut cache, entry.size);
        }

        let mut output = Vec::with_capacity(entry.size);
        while output.len() < entry.size {
            let block_start = cache
                .stream_position()
                .map_err(|error| format!("could not inspect cache position: {error}"))?;
            let mut header = [0; 8];
            cache
                .read_exact(&mut header)
                .map_err(|error| format!("could not read cache block header: {error}"))?;
            let block_shape = block_lengths(header);
            let (compressed_size, size) = if let Some(shape) = block_shape {
                shape
            } else {
                cache
                    .seek(SeekFrom::Start(block_start))
                    .map_err(|error| format!("could not rewind cache block: {error}"))?;
                (entry.compressed_size, entry.size)
            };
            if compressed_size > MAX_BLOCK_SIZE {
                return Err(format!("cache block exceeds {MAX_BLOCK_SIZE} bytes"));
            }
            if output.len().saturating_add(size) > entry.size {
                return Err(format!(
                    "resource {} expands past declared size",
                    entry.path
                ));
            }
            let input = read_exact(&mut cache, compressed_size)?;
            let mut block = vec![0; size];
            if input.first() == Some(&0x8c) {
                self.oodle()?.decompress(&input, &mut block)?;
            } else if compressed_size == size {
                block.copy_from_slice(&input);
            } else {
                lzf_decompress(&input, &mut block)?;
            }
            output.extend_from_slice(&block);
            if block_shape.is_none() {
                break;
            }
        }
        if output.len() != entry.size {
            return Err(format!(
                "resource {} decoded to {} bytes, expected {}",
                entry.path,
                output.len(),
                entry.size
            ));
        }
        Ok(output)
    }

    fn oodle(&mut self) -> Result<&Oodle, String> {
        if self.oodle.is_none() {
            self.oodle = Some(Oodle::load()?);
        }
        Ok(self.oodle.as_ref().expect("Oodle decoder initialized"))
    }
}

fn read_exact(reader: &mut File, size: usize) -> Result<Vec<u8>, String> {
    let mut data = vec![0; size];
    reader
        .read_exact(&mut data)
        .map_err(|error| format!("could not read cache data: {error}"))?;
    Ok(data)
}

fn block_lengths(header: [u8; 8]) -> Option<(usize, usize)> {
    if header[0] != 0x80 || header[7] & 0x0f != 1 {
        return None;
    }
    let compressed = (u32::from_be_bytes(header[..4].try_into().ok()?) >> 2) & 0x00ff_ffff;
    let decompressed = (u32::from_be_bytes(header[4..].try_into().ok()?) >> 5) & 0x00ff_ffff;
    Some((compressed as usize, decompressed as usize))
}

fn lzf_decompress(input: &[u8], output: &mut [u8]) -> Result<(), String> {
    let mut source = 0;
    let mut target = 0;
    while source < input.len() {
        let control = input[source] as usize;
        source += 1;
        if control < 32 {
            let length = control + 1;
            if source + length > input.len() || target + length > output.len() {
                return Err("invalid LZF literal run".to_owned());
            }
            output[target..target + length].copy_from_slice(&input[source..source + length]);
            source += length;
            target += length;
            continue;
        }

        let mut length = control >> 5;
        if length == 7 {
            let extra = *input
                .get(source)
                .ok_or_else(|| "truncated LZF match length".to_owned())?;
            source += 1;
            length += extra as usize;
        }
        let low = *input
            .get(source)
            .ok_or_else(|| "truncated LZF match offset".to_owned())? as usize;
        source += 1;
        let distance = ((control & 0x1f) << 8) + low + 1;
        let match_start = target
            .checked_sub(distance)
            .ok_or_else(|| "invalid LZF match offset".to_owned())?;
        let length = length + 2;
        if target + length > output.len() {
            return Err("LZF match exceeds output".to_owned());
        }
        for index in 0..length {
            output[target + index] = output[match_start + index];
        }
        target += length;
    }
    if target != output.len() {
        return Err(format!(
            "LZF decoded to {target} bytes, expected {}",
            output.len()
        ));
    }
    Ok(())
}

enum Oodle {
    Library(OodleLibrary),
    Command(OodleCommand),
}

struct OodleLibrary {
    path: PathBuf,
    _library: Library,
    decompress: OodleDecompress,
}

impl Oodle {
    fn load() -> Result<Self, String> {
        if let Some(path) = env::var_os("WFINSPECT_OODLE_LIBRARY") {
            return OodleLibrary::load_path(Path::new(&path))
                .map(Self::Library)
                .map_err(|error| {
                    format!(
                        "could not load WFINSPECT_OODLE_LIBRARY={}: {error}",
                        Path::new(&path).display()
                    )
                });
        }
        if let Some(path) = env::var_os("WFINSPECT_OODLE_COMMAND") {
            return OodleCommand::load_path(PathBuf::from(path))
                .map(Self::Command)
                .map_err(|error| format!("could not use WFINSPECT_OODLE_COMMAND: {error}"));
        }
        let mut errors = Vec::new();
        if let Some(executable) = retinue_client::executable_path()
            && let Some(prefix) = executable.parent().and_then(Path::parent)
        {
            let bundled = prefix.join("libexec/unoodle");
            if bundled.exists() {
                return OodleCommand::load_path(bundled).map(Self::Command);
            }
        }
        for name in ["liboo2corelinux64.so.9", "liboo2corelinux64.so"] {
            match OodleLibrary::load_path(Path::new(name)) {
                Ok(oodle) => return Ok(Self::Library(oodle)),
                Err(error) => errors.push(format!("{name}: {error}")),
            }
        }
        match OodleCommand::from_path() {
            Ok(oodle) => return Ok(Self::Command(oodle)),
            Err(error) => errors.push(error),
        }
        Err(format!(
            "Oodle decoder unavailable; set WFINSPECT_OODLE_LIBRARY to a licensed Linux library or WFINSPECT_OODLE_COMMAND to unoodle ({})",
            errors.join("; ")
        ))
    }

    fn backend(&self) -> &'static str {
        match self {
            Self::Library(_) => "official-library",
            Self::Command(_) => "external-unoodle",
        }
    }

    fn source(&self) -> &Path {
        match self {
            Self::Library(decoder) => &decoder.path,
            Self::Command(decoder) => &decoder.path,
        }
    }

    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<(), String> {
        match self {
            Self::Library(decoder) => decoder.decompress(input, output),
            Self::Command(decoder) => decoder.decompress(input, output),
        }
    }
}

impl OodleLibrary {
    fn load_path(path: &Path) -> Result<Self, String> {
        // SAFETY: Library remains owned by Oodle while copied function pointer is used.
        let library = unsafe { Library::new(path) }.map_err(|error| error.to_string())?;
        // SAFETY: Oodle's public C ABI defines this symbol and signature.
        let decompress = unsafe {
            *library
                .get::<OodleDecompress>(b"OodleLZ_Decompress\0")
                .map_err(|error| error.to_string())?
        };
        Ok(Self {
            path: path.to_path_buf(),
            _library: library,
            decompress,
        })
    }

    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<(), String> {
        // SAFETY: buffers remain valid for call duration; optional Oodle work buffers are null.
        let decoded = unsafe {
            (self.decompress)(
                input.as_ptr().cast(),
                input.len() as isize,
                output.as_mut_ptr().cast(),
                output.len() as isize,
                1,
                0,
                0,
                std::ptr::null_mut(),
                0,
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                3,
            )
        };
        if decoded != output.len() as isize {
            return Err(format!(
                "Oodle decoded {decoded} bytes, expected {}",
                output.len()
            ));
        }
        Ok(())
    }
}

struct OodleCommand {
    path: PathBuf,
}

impl OodleCommand {
    fn load_path(path: PathBuf) -> Result<Self, String> {
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!("{} is not executable", path.display()));
        }
        Ok(Self { path })
    }

    fn from_path() -> Result<Self, String> {
        let path =
            executable_in_path("unoodle").ok_or_else(|| "unoodle: not found in PATH".to_owned())?;
        Self::load_path(path)
    }

    fn decompress(&self, input: &[u8], output: &mut [u8]) -> Result<(), String> {
        let files = TempFiles::create(input)?;
        let result = Command::new(&self.path)
            .arg(&files.input)
            .arg("--length")
            .arg(output.len().to_string())
            .arg("--header")
            .arg("0")
            .arg("--output")
            .arg(&files.output)
            .output()
            .map_err(|error| format!("could not run {}: {error}", self.path.display()))?;
        if !result.status.success() {
            return Err(format!(
                "{} failed: {}",
                self.path.display(),
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        let decoded = fs::read(&files.output)
            .map_err(|error| format!("could not read {}: {error}", files.output.display()))?;
        if decoded.len() != output.len() {
            return Err(format!(
                "unoodle decoded {} bytes, expected {}",
                decoded.len(),
                output.len()
            ));
        }
        output.copy_from_slice(&decoded);
        Ok(())
    }
}

fn executable_in_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

struct TempFiles {
    input: PathBuf,
    output: PathBuf,
}

impl TempFiles {
    fn create(input: &[u8]) -> Result<Self, String> {
        for _ in 0..32 {
            let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let stem = format!("wfinspect-oodle-{}-{id}", std::process::id());
            let input_path = env::temp_dir().join(format!("{stem}.input"));
            let output = env::temp_dir().join(format!("{stem}.output"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&input_path)
            {
                Ok(mut file) => {
                    file.write_all(input).map_err(|error| {
                        format!("could not write {}: {error}", input_path.display())
                    })?;
                    return Ok(Self {
                        input: input_path,
                        output,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "could not create {}: {error}",
                        input_path.display()
                    ));
                }
            }
        }
        Err("could not allocate temporary Oodle input".to_owned())
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.input);
        let _ = fs::remove_file(&self.output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toc_paths_and_file_metadata() {
        let mut toc = vec![0; TOC_HEADER_SIZE];
        toc.extend(entry(-1, 1, 0, 0, 0, "Lotus"));
        toc.extend(entry(-1, 1, 0, 0, 1, "Scripts"));
        toc.extend(entry(32, 0, 3, 6, 2, "Probe.lua"));
        toc.extend(entry(64, 1, 4, 8, 2, "Probe.lua"));
        toc.extend(entry(128, 0, 1, 1, 2, "Removed.lua"));
        toc.extend(entry(0, 0, 0, 0, 0, ""));
        let files = parse_toc(&toc, 'B', PathBuf::from("B.Font.cache")).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "/Lotus/Scripts/Probe.lua");
        assert_eq!(files[0].offset, 64);
        assert_eq!(files[0].compressed_size, 4);
        assert_eq!(files[0].size, 8);
        assert_eq!(files[1].path, "/Lotus/Scripts/Removed.lua");
    }

    #[test]
    fn decodes_lzf_literals_and_overlapping_matches() {
        let mut literal = [0; 3];
        lzf_decompress(&[2, b'a', b'b', b'c'], &mut literal).unwrap();
        assert_eq!(&literal, b"abc");

        let mut repeated = [0; 6];
        lzf_decompress(&[2, b'a', b'b', b'c', 0x20, 0x02], &mut repeated).unwrap();
        assert_eq!(&repeated, b"abcabc");
    }

    #[test]
    fn rejects_package_path_injection() {
        assert!(validate_package("Font").is_ok());
        assert!(validate_package("../Font").is_err());
        assert!(validate_package("Font/Other").is_err());
    }

    #[test]
    fn reads_only_requested_cache_split() {
        let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory =
            env::temp_dir().join(format!("wfinspect-cache-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let mut toc = vec![0; TOC_HEADER_SIZE];
        toc.extend(entry(0, 0, 4, 4, 0, "Probe.lua"));
        fs::write(directory.join("B.Font.toc"), toc).unwrap();
        fs::write(directory.join("B.Font.cache"), b"test").unwrap();

        let resource = read_resource_split(&directory, "Font", "/Probe.lua", 'B').unwrap();
        assert_eq!(resource.split, 'B');
        assert_eq!(resource.data, b"test");
        assert!(read_resource_split(&directory, "Font", "/Probe.lua", 'X').is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn external_oodle_command_reads_exact_output() {
        let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let helper =
            env::temp_dir().join(format!("wfinspect-oodle-test-{}-{id}", std::process::id()));
        fs::write(&helper, "#!/bin/sh\ncp \"$1\" \"$7\"\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();

        let decoder = OodleCommand::load_path(helper.clone()).unwrap();
        let mut output = [0; 4];
        decoder.decompress(b"test", &mut output).unwrap();

        assert_eq!(&output, b"test");
        fs::remove_file(helper).unwrap();
    }

    fn entry(
        offset: i64,
        timestamp: i64,
        compressed_size: i32,
        size: i32,
        parent: i32,
        name: &str,
    ) -> Vec<u8> {
        let mut result = vec![0; TOC_ENTRY_SIZE];
        result[0..8].copy_from_slice(&offset.to_le_bytes());
        result[8..16].copy_from_slice(&timestamp.to_le_bytes());
        result[16..20].copy_from_slice(&compressed_size.to_le_bytes());
        result[20..24].copy_from_slice(&size.to_le_bytes());
        result[28..32].copy_from_slice(&parent.to_le_bytes());
        result[32..32 + name.len()].copy_from_slice(name.as_bytes());
        result
    }
}
