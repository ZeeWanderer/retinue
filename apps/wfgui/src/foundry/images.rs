use retinue_ui::{
    painter::RasterImage,
    preparation::Preparation,
    resources::{CacheStats, Resource},
    thumbnails::{Key, Request, Thumbnails, WorkStats},
};
use std::path::PathBuf;

pub struct Batch {
    pub images: Vec<(Key, Resource<RasterImage>)>,
    pub errors: Vec<(Key, String)>,
    pub stats: (CacheStats, CacheStats),
    pub work: WorkStats,
    pub warnings: Vec<String>,
    pub done: bool,
}

pub struct Input {
    pub generation: u64,
    pub requests: Vec<Request>,
}

pub fn cache(root: Option<PathBuf>) -> Thumbnails {
    let cache = Thumbnails::new(24 * 1024 * 1024, 32 * 1024 * 1024);
    match root {
        Some(root) => cache.with_disk(root),
        None => cache,
    }
}

pub fn spawn(
    root: Option<PathBuf>,
    notify: impl Fn() + Send + 'static,
) -> Result<Preparation<Input, Batch>, String> {
    let mut cache = cache(root);
    let mut generation = 0;
    Preparation::spawn_streaming(
        "wfgui-images",
        move |input: Input, token, publish| {
            if generation != input.generation {
                cache.clear();
                generation = input.generation;
            }
            let mut ready = Vec::new();
            let mut misses = Vec::new();
            let mut writes = Vec::new();
            for request in &input.requests {
                if let Some(image) = cache.cached(&request.key) {
                    ready.push((request.key, image.clone()));
                    writes.push((request, image));
                } else {
                    misses.push(request);
                }
            }
            if !ready.is_empty() {
                publish(Batch {
                    images: ready,
                    errors: Vec::new(),
                    stats: cache.stats(),
                    work: cache.work,
                    warnings: Vec::new(),
                    done: false,
                });
            }
            if !token.is_current() {
                return;
            }
            cache.begin_batch(&input.requests);
            for request in misses {
                if !token.is_current() {
                    return;
                }
                let (images, errors) = match cache.prepare(request) {
                    Ok(image) => {
                        writes.push((request, image.clone()));
                        (vec![(request.key, image)], Vec::new())
                    }
                    Err(error) => (Vec::new(), vec![(request.key, error)]),
                };
                publish(Batch {
                    images,
                    errors,
                    stats: cache.stats(),
                    work: cache.work,
                    warnings: cache.take_warnings(),
                    done: false,
                });
            }
            for (request, image) in writes {
                if !token.is_current() {
                    return;
                }
                cache.persist(request, &image);
            }
            publish(Batch {
                images: Vec::new(),
                errors: Vec::new(),
                stats: cache.stats(),
                work: cache.work,
                warnings: cache.take_warnings(),
                done: true,
            });
        },
        |previous: &mut Batch, mut next| {
            previous.images.append(&mut next.images);
            previous.errors.append(&mut next.errors);
            for warning in next.warnings {
                if previous.warnings.len() < 16 && !previous.warnings.contains(&warning) {
                    previous.warnings.push(warning);
                }
            }
            previous.stats = next.stats;
            previous.work = next.work;
            previous.done = next.done;
        },
        notify,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, sync::mpsc, time::Duration};

    #[test]
    fn cached_images_arrive_even_while_disk_work_is_blocked() {
        let root = tempfile::tempdir().unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        retinue_ui::image::RgbaImage::new(80, 80)
            .write_to(&mut bytes, retinue_ui::image::ImageFormat::Png)
            .unwrap();
        let bytes = Box::leak(bytes.into_inner().into_boxed_slice());
        let fast = Request::embedded(bytes, 16, None);
        let slow = Request::embedded(bytes, 32, None).persistent("test", "disk");
        let (send, wake) = mpsc::channel();
        let mut worker = spawn(Some(root.path().into()), move || {
            send.send(()).unwrap();
        })
        .unwrap();
        worker
            .request(Input {
                generation: 0,
                requests: vec![fast.clone()],
            })
            .unwrap();
        loop {
            wake.recv_timeout(Duration::from_secs(5)).unwrap();
            if worker.poll().unwrap().is_some_and(|batch| batch.done) {
                break;
            }
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.path().join("lock"))
            .unwrap();
        lock.lock().unwrap();
        worker
            .request(Input {
                generation: 0,
                requests: vec![fast.clone(), slow.clone()],
            })
            .unwrap();
        wake.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = worker.poll().unwrap().unwrap();
        assert!(!first.done);
        assert_eq!(
            first.images.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            vec![fast.key]
        );
        drop(lock);
        let mut loaded = false;
        loop {
            wake.recv_timeout(Duration::from_secs(5)).unwrap();
            if let Some(batch) = worker.poll().unwrap() {
                loaded |= batch.images.iter().any(|(key, _)| *key == slow.key);
                if batch.done {
                    assert!(loaded);
                    break;
                }
            }
        }
    }
}
