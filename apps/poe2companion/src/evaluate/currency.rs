use super::{model::Model, worker::Reply};
use retinue_client::paths::Paths;
use retinue_ui::{
    image::{ImageReader, Limits, imageops::FilterType},
    painter::RasterImage,
    resources::{Resource, ResourceCache},
    schedule::Ping,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, watch};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    ids: Vec<String>,
    size: u32,
    generation: u64,
}

impl Request {
    pub fn for_model(model: &Model, scale: f32) -> Self {
        let ids: BTreeSet<_> = model
            .results
            .iter()
            .flat_map(|r| r.listings.iter().map(|l| l.currency.clone()))
            .chain(model.estimate.iter().map(|e| e.currency.clone()))
            .collect();
        Self {
            ids: ids.into_iter().take(16).collect(),
            size: (20.0 * scale).round() as u32,
            generation: model.generation,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Source {
    pub id: String,
    pub text: String,
    pub path: String,
    pub digest: String,
    pub tier: Option<String>,
}

#[derive(Clone)]
pub struct Icon {
    pub source: Source,
    pub size: u32,
    pub image: Resource<RasterImage>,
}

type Key = ([u8; 32], u32);
type Cache = ResourceCache<Key, RasterImage>;

fn prepare(source: Source, size: u32, cache: &mut Cache) -> Result<Icon, String> {
    if !(1..=128).contains(&size) || source.digest.len() != 64 {
        return Err("invalid currency image identity or size".into());
    }
    let key = (Sha256::digest(source.digest.as_bytes()).into(), size);
    let image = cache.try_get_or_insert_with(key, || {
        let mut bytes = Vec::new();
        std::fs::File::open(&source.path)
            .and_then(|f| f.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes))
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != source.digest
        {
            return Err("currency image changed or exceeds size limit".into());
        }
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(512);
        limits.max_image_height = Some(512);
        limits.max_alloc = Some(4 * 1024 * 1024);
        reader.limits(limits);
        let pixels = reader
            .decode()
            .map_err(|e| e.to_string())?
            .resize_exact(size, size, FilterType::Lanczos3)
            .into_rgba8();
        RasterImage::from_rgba(pixels).map_err(|e| e.to_string())
    })?;
    Ok(Icon {
        source,
        size,
        image,
    })
}

async fn load(id: String, size: u32, cache: Arc<Mutex<Cache>>) -> Result<Icon, String> {
    let reply = crate::Connection::trade(
        Paths::from_env().daemon_socket(),
        serde_json::json!({"op":"currency_asset", "currency":id}),
    )
    .await
    .map_err(|e| e.to_string())?;
    if reply["ok"] != true {
        return Err(format!("currency asset unavailable: {}", reply["error"]));
    }
    let source: Source =
        serde_json::from_value(reply["data"].clone()).map_err(|e| e.to_string())?;
    if source.id != id {
        return Err("currency identity mismatch".into());
    }
    tokio::task::spawn_blocking(move || prepare(source, size, &mut cache.lock().unwrap()))
        .await
        .map_err(|e| e.to_string())?
}

pub async fn run(mut requests: watch::Receiver<Request>, replies: mpsc::Sender<Reply>, wake: Ping) {
    let cache = Arc::new(Mutex::new(Cache::new(2 * 1024 * 1024)));
    while requests.changed().await.is_ok() {
        let request = requests.borrow_and_update().clone();
        let mut icons = BTreeMap::new();
        for id in &request.ids {
            let loaded = tokio::select! {
                changed = requests.changed() => {
                    if changed.is_ok() { requests.mark_changed(); }
                    break;
                },
                icon = load(id.clone(), request.size, cache.clone()) => icon,
            };
            match loaded {
                Ok(icon) => {
                    icons.insert(id.clone(), icon);
                }
                Err(error) => eprintln!("poe2companion: currency {id}: {error}"),
            }
            if replies
                .send(Reply::Currencies(request.clone(), icons.clone()))
                .await
                .is_err()
            {
                return;
            }
            wake.ping();
        }
    }
}

pub fn preview(path: &Path, size: u32) -> Result<BTreeMap<String, Icon>, String> {
    let sources: Vec<Source> =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if sources.len() > 64 {
        return Err("too many preview currency assets".into());
    }
    let mut cache = Cache::new(2 * 1024 * 1024);
    sources
        .into_iter()
        .map(|mut source| {
            source.path = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(&source.path)
                .to_string_lossy()
                .into_owned();
            prepare(source, size, &mut cache).map(|icon| (icon.source.id.clone(), icon))
        })
        .collect()
}

pub fn abbreviation(currency: &str) -> &str {
    match currency {
        "exalted" => "ex",
        "divine" => "div",
        "chaos" => "c",
        _ => currency,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_ui::image::{Rgba, RgbaImage};

    fn source(path: &Path, value: u8) -> Source {
        RgbaImage::from_pixel(64, 64, Rgba([value, 80, 30, 255]))
            .save(path)
            .unwrap();
        Source {
            id: "future-currency".into(),
            text: "Future Currency".into(),
            path: path.to_string_lossy().into_owned(),
            digest: format!("{:x}", Sha256::digest(std::fs::read(path).unwrap())),
            tier: None,
        }
    }

    #[test]
    fn derivatives_are_display_sized_and_keyed_by_content_not_currency_name() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("icon.png");
        let first = source(&path, 100);
        let mut cache = Cache::new(32768);
        for size in [20, 25, 30, 40] {
            let icon = prepare(first.clone(), size, &mut cache).unwrap();
            assert_eq!(icon.image.byte_len(), (size * size * 4) as usize);
        }
        std::fs::remove_file(&path).unwrap();
        assert!(
            prepare(first.clone(), 20, &mut cache).is_ok(),
            "warm paint resources need no file I/O"
        );
        let second = source(&path, 200);
        assert_ne!(first.digest, second.digest);
        let before = cache.stats().misses;
        prepare(second, 20, &mut cache).unwrap();
        assert_eq!(cache.stats().misses, before + 1);
        assert!(
            prepare(first, 21, &mut cache).is_err(),
            "stale identity must not decode replacement bytes"
        );
        assert!(cache.stats().cached_bytes <= 32768);
    }

    #[test]
    fn requests_deduplicate_currencies_and_keep_missing_images_out_of_price_state() {
        let mut model = Model::default();
        model.results = Some(super::super::model::Results {
            listings: (0..100)
                .map(|_| super::super::model::Listing {
                    currency: "new-coin".into(),
                    amount: 12.0,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let request = Request::for_model(&model, 1.25);
        assert_eq!(request.ids, ["new-coin"]);
        assert_eq!(request.size, 25);
        assert_eq!(model.results.as_ref().unwrap().listings[0].amount, 12.0);
        assert_eq!(abbreviation("divine"), "div");
        assert_eq!(abbreviation("new-coin"), "new-coin");
    }
}
