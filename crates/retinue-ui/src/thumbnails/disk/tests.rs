use super::*;
use crate::thumbnails::{Source, Thumbnails};

fn icon() -> &'static [u8] {
    static ICON: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let image = image::RgbaImage::from_fn(80, 64, |x, y| {
            image::Rgba([100, x as u8, 175, (y * 4) as u8])
        });
        let mut bytes = io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    })
}

fn request(size: u32) -> Request {
    Request::embedded(icon(), size, None).persistent("test", "favorite")
}

#[test]
fn restart_reads_derivatives_without_reading_the_source_or_resampling() {
    let dir = tempfile::tempdir().unwrap();
    let mut cache = Thumbnails::new(1 << 20, 1 << 20).with_disk(dir.path().to_owned());
    let mut request = request(40);
    cache.begin_batch(&[request.clone()]);
    let original = cache.prepare(&request).unwrap();
    cache.persist(&request, &original);
    assert_eq!(cache.work.persistent_resamples, 1);
    assert_eq!(cache.work.disk_writes, 1);
    let pixels = original.to_rgba().unwrap();
    drop(cache);
    request.source = Source::File(dir.path().join("missing-source.png"));
    let mut cache = Thumbnails::new(1 << 20, 1 << 20).with_disk(dir.path().to_owned());
    cache.begin_batch(&[request.clone()]);
    let image = cache.prepare(&request).unwrap();
    assert!(image.to_rgba().unwrap() == pixels);
    assert_eq!(cache.work.disk_hits, 1);
    assert_eq!(cache.work.resamples, 0);
    assert!(cache.take_warnings().is_empty());
}

#[test]
fn keeps_old_revision_until_replacement_is_stored_and_rejects_stale_writer() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = DiskCache::new(dir.path().into());
    let a = request(40);
    let pixels = RasterImage::from_rgba(image::RgbaImage::new(40, 40)).unwrap();
    first.begin_batch(&[a.clone()]).unwrap();
    first.store(&a, &pixels).unwrap();
    let old = first.path(&a).unwrap();
    let mut b = a.clone();
    b.key.digest[0] ^= 1;
    let mut second = DiskCache::new(dir.path().into());
    second.begin_batch(&[b.clone()]).unwrap();
    assert!(old.exists());
    assert!(first.load(&a).unwrap().is_none());
    assert!(!first.store(&a, &pixels).unwrap());
    let mut pending = a.clone();
    pending.key.width = 80;
    pending.key.height = 80;
    assert!(!first.store(&pending, &pixels).unwrap());
    second.store(&b, &pixels).unwrap();
    assert!(!old.parent().unwrap().exists());
    assert!(second.path(&b).unwrap().exists());
}

#[test]
fn clear_invalidates_inflight_writers_until_next_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut cache = DiskCache::new(dir.path().into());
    let request = request(40);
    let pixels = RasterImage::from_rgba(image::RgbaImage::new(40, 40)).unwrap();
    cache.begin_batch(&[request.clone()]).unwrap();
    DiskCache::clear(dir.path()).unwrap();
    assert!(!cache.store(&request, &pixels).unwrap());
    cache.begin_batch(&[request.clone()]).unwrap();
    assert!(cache.store(&request, &pixels).unwrap());
    DiskCache::clear(dir.path()).unwrap();
    assert!(!cache.path(&request).unwrap().exists());
    assert!(cache.load(&request).unwrap().is_none());
}

#[test]
fn corrupt_cache_falls_back_to_source_and_repairs() {
    let dir = tempfile::tempdir().unwrap();
    let request = request(40);
    let mut disk = DiskCache::new(dir.path().into());
    disk.begin_batch(&[request.clone()]).unwrap();
    fs::write(disk.path(&request).unwrap(), "not a PNG").unwrap();
    let mut cache = Thumbnails::new(1 << 20, 1 << 20).with_disk(dir.path().into());
    cache.begin_batch(&[request.clone()]);
    let image = cache.prepare(&request).unwrap();
    cache.persist(&request, &image);
    assert_eq!(cache.work.resamples, 1);
    assert_eq!(cache.take_warnings().len(), 1);
    assert!(disk.load(&request).unwrap().is_some());
}

#[test]
fn physical_size_crop_and_origin_are_separate_keys() {
    let disk = DiskCache::new("unused".into());
    let a = request(40);
    let mut crop = a.clone();
    crop.key.crop = Some([1, 2, 3, 4]);
    assert_ne!(disk.path(&a), disk.path(&request(50)));
    assert_ne!(disk.path(&a), disk.path(&crop));
    assert_ne!(
        disk.path(&a),
        disk.path(&a.clone().persistent("test", "other"))
    );
    assert!(disk.path(&Request::embedded(icon(), 40, None)).is_none());
}

#[test]
fn unchanged_revision_does_not_rewrite_its_marker_on_restart() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let request = request(40);
    let mut cache = DiskCache::new(dir.path().into());
    cache.begin_batch(&[request.clone()]).unwrap();
    let marker = dir
        .path()
        .join(hex(&request.origin.unwrap()))
        .join("current");
    let inode = fs::metadata(&marker).unwrap().ino();
    let mut restarted = DiskCache::new(dir.path().into());
    restarted.begin_batch(&[request]).unwrap();
    assert_eq!(fs::metadata(marker).unwrap().ino(), inode);
}

#[test]
fn maintenance_never_follows_nested_links_and_unwritable_cache_is_optional() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("keep.png"), "keep").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("nested")).unwrap();
    DiskCache::clear(dir.path()).unwrap();
    assert!(outside.path().join("keep.png").exists());
    let root = dir.path().join("file");
    fs::write(&root, "not a directory").unwrap();
    let mut cache = Thumbnails::new(1 << 20, 1 << 20).with_disk(root);
    let request = request(40);
    cache.begin_batch(&[request.clone()]);
    assert!(cache.prepare(&request).is_ok());
    assert!(!cache.take_warnings().is_empty());
}
