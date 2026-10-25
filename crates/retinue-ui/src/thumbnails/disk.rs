use super::{Key, Request, parse_digest};
use crate::painter::RasterImage;
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const DIRECTORY: &str = "native-v1";

pub struct DiskCache {
    root: PathBuf,
    epoch: Option<u64>,
    origins: HashMap<[u8; 32], [u8; 32]>,
    stored: HashSet<([u8; 32], Key)>,
}

impl DiskCache {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            epoch: None,
            origins: HashMap::new(),
            stored: HashSet::new(),
        }
    }

    pub fn begin_batch(&mut self, requests: &[Request]) -> io::Result<()> {
        let previous_epoch = self.epoch.take();
        if !requests.iter().any(|r| r.origin.is_some()) {
            return Ok(());
        }
        let _lock = lock(&self.root)?;
        let epoch = read_epoch(&self.root)?;
        if previous_epoch != Some(epoch) {
            self.stored.clear();
        }
        self.stored
            .retain(|(origin, key)| self.origins.get(origin) == Some(&key.digest));
        for request in requests {
            let Some(origin) = request.origin else {
                continue;
            };
            if self.origins.get(&origin) == Some(&request.key.digest) {
                continue;
            }
            let directory = self.root.join(hex(&origin));
            create_owned(&directory)?;
            create_owned(&directory.join(hex(&request.key.digest)))?;
            let marker = directory.join("current");
            if !read_limited(&marker, 32).is_ok_and(|digest| digest == request.key.digest) {
                atomic_write(&marker, &request.key.digest)?;
            }
            self.origins.insert(origin, request.key.digest);
        }
        self.epoch = Some(epoch);
        Ok(())
    }

    fn path(&self, request: &Request) -> Option<PathBuf> {
        let origin = request.origin?;
        let key = request.key;
        let crop = key.crop.map_or_else(
            || "full".to_owned(),
            |[x, y, w, h]| format!("{x}-{y}-{w}-{h}"),
        );
        let tint = key.tint.map_or_else(String::new, |[r, g, b, a]| {
            format!("-tint-{r:02x}{g:02x}{b:02x}{a:02x}")
        });
        Some(
            self.root
                .join(hex(&origin))
                .join(hex(&key.digest))
                .join(format!(
                    "lanczos-v1-{}x{}-{crop}{tint}.png",
                    key.width, key.height
                )),
        )
    }

    fn current(&self, request: &Request) -> io::Result<bool> {
        let Some(epoch) = self.epoch else {
            return Ok(false);
        };
        let Some(origin) = request.origin else {
            return Ok(false);
        };
        let directory = self.root.join(hex(&origin));
        reject_link(&directory)?;
        reject_link(&directory.join(hex(&request.key.digest)))?;
        if read_epoch(&self.root)? != epoch {
            return Ok(false);
        }
        Ok(read_limited(&directory.join("current"), 32).is_ok_and(|v| v == request.key.digest))
    }

    pub fn load(&mut self, request: &Request) -> io::Result<Option<RasterImage>> {
        if self.epoch.is_none() || request.origin.is_none() {
            return Ok(None);
        }
        let path = self.path(request).unwrap();
        let bytes = {
            let _lock = lock(&self.root)?;
            if !self.current(request)? {
                return Ok(None);
            }
            reject_link(&path)?;
            match read_limited(&path, 32 * 1024 * 1024) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            }
        };
        let decoded = (|| {
            let mut reader =
                image::ImageReader::with_format(io::Cursor::new(bytes), image::ImageFormat::Png);
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(32 * 1024 * 1024);
            limits.max_image_width = Some(request.key.width);
            limits.max_image_height = Some(request.key.height);
            reader.limits(limits);
            let image = reader.decode().map_err(io::Error::other)?;
            if image.width() != request.key.width && image.height() != request.key.height {
                return Err(io::Error::other("invalid thumbnail dimensions"));
            }
            RasterImage::from_rgba(image.into_rgba8()).map_err(io::Error::other)
        })();
        match decoded {
            Ok(image) => {
                self.stored.insert((request.origin.unwrap(), request.key));
                Ok(Some(image))
            }
            Err(error) => {
                let _lock = lock(&self.root)?;
                if self.current(request)? {
                    let _ = fs::remove_file(path);
                }
                Err(io::Error::other(format!(
                    "Discarded corrupt thumbnail: {error}"
                )))
            }
        }
    }

    pub fn store(&mut self, request: &Request, image: &RasterImage) -> io::Result<bool> {
        let Some(origin) = request.origin else {
            return Ok(false);
        };
        if self.epoch.is_none() || self.stored.contains(&(origin, request.key)) {
            return Ok(false);
        }
        let path = self.path(request).unwrap();
        {
            let _lock = lock(&self.root)?;
            if !self.current(request)? {
                return Ok(false);
            }
        }
        let pixels = image.to_rgba().map_err(io::Error::other)?;
        let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        PngEncoder::new_with_quality(
            file.as_file_mut(),
            CompressionType::Fast,
            FilterType::Adaptive,
        )
        .write_image(
            &pixels,
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(io::Error::other)?;
        let _lock = lock(&self.root)?;
        if !self.current(request)? {
            return Ok(false);
        }
        file.persist(&path).map_err(|e| e.error)?;
        self.stored.insert((origin, request.key));
        let parent = self.root.join(hex(&origin));
        let digest = hex(&request.key.digest);
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let name = entry.file_name();
            if entry.file_type()?.is_dir()
                && name
                    .to_str()
                    .is_some_and(|s| s != digest && parse_digest(s).is_some())
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(true)
    }

    pub fn clear(root: &Path) -> io::Result<()> {
        if !root.exists() {
            return Ok(());
        }
        let _lock = lock(root)?;
        atomic_write(
            &root.join("epoch"),
            &read_epoch(root)?.wrapping_add(1).to_le_bytes(),
        )?;
        let mut directories = vec![root.to_owned()];
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    directories.push(entry.path());
                } else if kind.is_file() && entry.path().extension().is_some_and(|s| s == "png") {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn reject_link(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(io::Error::other("Unexpected link inside derivative cache"))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn create_owned(path: &Path) -> io::Result<()> {
    reject_link(path)?;
    fs::create_dir_all(path)
}

fn lock(root: &Path) -> io::Result<File> {
    fs::create_dir_all(root)?;
    let path = root.join("lock");
    reject_link(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.lock()?;
    Ok(file)
}

fn read_epoch(root: &Path) -> io::Result<u64> {
    match read_limited(&root.join("epoch"), 8) {
        Ok(bytes) => {
            Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
                io::Error::other("Invalid derivative cache generation")
            })?))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

fn read_limited(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    reject_link(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::other("Oversized derivative cache file"));
    }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    reject_link(path)?;
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    file.write_all(bytes)?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
