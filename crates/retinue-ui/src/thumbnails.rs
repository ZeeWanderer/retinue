use crate::{
    painter::RasterImage,
    resources::{CacheStats, Resource, ResourceCache},
};
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::PathBuf};

pub mod disk;
use disk::DiskCache;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Embedded(&'static [u8]),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Key {
    pub digest: [u8; 32],
    pub width: u32,
    pub height: u32,
    pub crop: Option<[u32; 4]>,
    pub tint: Option<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub key: Key,
    pub source: Source,
    pub origin: Option<[u8; 32]>,
}

impl Request {
    pub fn embedded(bytes: &'static [u8], size: u32, crop: Option<[u32; 4]>) -> Self {
        Self {
            key: Key {
                digest: Sha256::digest(bytes).into(),
                width: size,
                height: size,
                crop,
                tint: None,
            },
            source: Source::Embedded(bytes),
            origin: None,
        }
    }

    pub fn persistent(mut self, source: &str, name: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(source.as_bytes());
        digest.update([0]);
        digest.update(name.as_bytes());
        self.origin = Some(digest.finalize().into());
        self
    }
    pub fn tinted(mut self, rgba: [u8; 4]) -> Self {
        self.key.tint = Some(rgba);
        self
    }
}

pub fn parse_digest(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut digest = [0; 32];
    for (byte, pair) in digest.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *byte = ((pair[0] as char).to_digit(16)? * 16 + (pair[1] as char).to_digit(16)?) as u8;
    }
    Some(digest)
}

#[derive(Clone, Copy, Default, Debug)]
pub struct WorkStats {
    pub ram_hits: u64,
    pub disk_hits: u64,
    pub resamples: u64,
    pub persistent_resamples: u64,
    pub disk_writes: u64,
}

/// Call on a preparation worker. Source resolution remains outside the renderer.
pub struct Thumbnails {
    sources: ResourceCache<([u8; 32], Option<[u32; 4]>, Option<[u8; 4]>), RasterImage>,
    rendered: ResourceCache<Key, RasterImage>,
    disk: Option<DiskCache>,
    pub work: WorkStats,
    warnings: Vec<String>,
}

impl Thumbnails {
    pub fn new(source_budget: usize, rendered_budget: usize) -> Self {
        Self {
            sources: ResourceCache::new(source_budget),
            rendered: ResourceCache::new(rendered_budget),
            disk: None,
            work: WorkStats::default(),
            warnings: Vec::new(),
        }
    }

    pub fn with_disk(mut self, root: PathBuf) -> Self {
        self.disk = Some(DiskCache::new(root));
        self
    }

    pub fn begin_batch(&mut self, requests: &[Request]) {
        if let Some(disk) = &mut self.disk {
            if let Err(error) = disk.begin_batch(requests) {
                self.warn(error);
            }
        }
    }

    fn warn(&mut self, error: impl ToString) {
        let message = error.to_string();
        if self.warnings.len() < 16 && !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }

    pub fn cached(&mut self, key: &Key) -> Option<Resource<RasterImage>> {
        let image = self.rendered.get(key)?;
        self.work.ram_hits += 1;
        Some(image)
    }

    pub fn persist(&mut self, request: &Request, image: &RasterImage) {
        if let Some(disk) = &mut self.disk {
            match disk.store(request, image) {
                Ok(true) => self.work.disk_writes += 1,
                Ok(false) => (),
                Err(error) => self.warn(error),
            }
        }
    }

    pub fn clear(&mut self) {
        self.sources.clear();
        self.rendered.clear();
    }

    pub fn stats(&self) -> (CacheStats, CacheStats) {
        (self.sources.stats(), self.rendered.stats())
    }

    pub fn prepare(&mut self, request: &Request) -> Result<Resource<RasterImage>, String> {
        let key = request.key;
        if key.width == 0 || key.height == 0 || key.width > 2048 || key.height > 2048 {
            return Err("Thumbnail dimensions outside 1..2048".into());
        }
        if let Some(image) = self.cached(&key) {
            return Ok(image);
        }
        if let Some(disk) = &mut self.disk {
            match disk.load(request) {
                Ok(Some(image)) => {
                    self.work.disk_hits += 1;
                    return Ok(self.rendered.insert(key, image));
                }
                Ok(None) => (),
                Err(error) => self.warn(error),
            }
        }
        let source_key = (key.digest, key.crop, key.tint);
        let image = self.sources.try_get_or_insert_with(source_key, || {
            let bytes = match &request.source {
                Source::File(path) => {
                    use std::io::Read;
                    let mut bytes = Vec::new();
                    std::fs::File::open(path)
                        .map_err(|e| e.to_string())?
                        .take(16 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    if bytes.len() > 16 * 1024 * 1024 {
                        return Err("Source image exceeds encoded limit".into());
                    }
                    bytes
                }
                Source::Embedded(bytes) => bytes.to_vec(),
            };
            if <[u8; 32]>::from(Sha256::digest(&bytes)) != key.digest {
                return Err("Source image changed before decoding".into());
            }
            let mut reader = image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| e.to_string())?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(32 * 1024 * 1024);
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            reader.limits(limits);
            let mut image = reader.decode().map_err(|e| e.to_string())?;
            if let Some([x, y, w, h]) = key.crop {
                if w == 0
                    || h == 0
                    || x.saturating_add(w) > image.width()
                    || y.saturating_add(h) > image.height()
                {
                    return Err("Invalid source display bounds".into());
                }
                image = image.crop_imm(x, y, w, h);
            }
            let mut image = image.into_rgba8();
            if let Some([r, g, b, a]) = key.tint {
                for pixel in image.pixels_mut() {
                    *pixel =
                        image::Rgba([r, g, b, ((pixel[3] as u16 * a as u16 + 127) / 255) as u8]);
                }
            }
            RasterImage::from_rgba(image).map_err(|e| e.to_string())
        })?;
        let (w, h) = image.dimensions();
        let ratio = (key.width as f64 / w as f64).min(key.height as f64 / h as f64);
        let resized = image
            .resampled(
                (w as f64 * ratio).round().max(1.0) as u32,
                (h as f64 * ratio).round().max(1.0) as u32,
            )
            .map_err(|e| e.to_string())?;
        self.work.resamples += 1;
        self.work.persistent_resamples += u64::from(request.origin.is_some());
        Ok(self.rendered.insert(key, resized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_icons_over_budget_are_reused_without_resampling() {
        let mut encoded = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(40, 40, image::Rgba([30, 60, 90, 255]))
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = Box::leak(encoded.into_inner().into_boxed_slice());
        let requests: Vec<_> = (20..30)
            .map(|size| Request::embedded(bytes, size, None))
            .collect();
        let mut cache = Thumbnails::new(1, 1);
        let held: Vec<_> = requests
            .iter()
            .map(|request| cache.prepare(request).unwrap())
            .collect();
        let resamples = cache.work.resamples;
        for _ in 0..4 {
            for (request, image) in requests.iter().zip(&held) {
                assert!(std::ptr::eq(&**image, &*cache.prepare(request).unwrap()));
            }
        }
        assert_eq!(cache.work.resamples, resamples);
        drop(held);
        assert_eq!(cache.stats().1.live_bytes, 0);
    }
    #[test]
    fn tints_are_distinct_cached_derivatives_preserving_source_alpha() {
        let mut encoded = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 8, image::Rgba([20, 40, 60, 128]))
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = Box::leak(encoded.into_inner().into_boxed_slice());
        let original = Request::embedded(bytes, 8, None);
        let red = original.clone().tinted([255, 0, 0, 255]);
        let green = original.clone().tinted([0, 255, 0, 128]);
        assert_ne!(original.key, red.key);
        assert_ne!(red.key, green.key);
        let mut cache = Thumbnails::new(4096, 4096);
        let a = cache.prepare(&red).unwrap();
        let b = cache.prepare(&green).unwrap();
        assert_eq!(a.to_rgba().unwrap().get_pixel(0, 0).0, [255, 0, 0, 128]);
        assert_eq!(b.to_rgba().unwrap().get_pixel(0, 0).0, [0, 255, 0, 64]);
        assert!(!std::ptr::eq(&*a, &*b));
        assert!(std::ptr::eq(&*a, &*cache.prepare(&red).unwrap()));
        assert_eq!(cache.stats().1.entries, 2);
    }
    #[test]
    fn physical_size_and_content_identity_key_derivatives() {
        let mut cache = Thumbnails::new(1024 * 1024, 1024 * 1024);
        let mut encoded = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(100, 80, image::Rgba([180, 120, 30, 255]))
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let icon = Box::leak(encoded.into_inner().into_boxed_slice());
        let a = Request::embedded(icon, 40, None);
        let first = cache.prepare(&a).unwrap();
        let again = cache.prepare(&a).unwrap();
        assert!(std::ptr::eq(&*first, &*again));
        let b = Request::embedded(icon, 50, None);
        let larger = cache.prepare(&b).unwrap();
        assert_ne!(larger.dimensions(), first.dimensions());
        assert_eq!(cache.stats().1.entries, 2);
        let mut invalid = a.clone();
        invalid.key.digest[0] ^= 1;
        assert!(cache.prepare(&invalid).err().unwrap().contains("changed"));
        cache.clear();
        assert_eq!(cache.stats().1.entries, 0);
        assert!(cache.stats().1.live_bytes > 0);
        assert_eq!(first.dimensions().0.max(first.dimensions().1), 40);
    }
}
