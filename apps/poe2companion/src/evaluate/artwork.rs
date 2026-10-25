use retinue_ui::{
    Rect,
    image::{self, RgbaImage, imageops},
    painter::{Painter, RasterImage},
    resources::{Resource, ResourceCache},
};
use std::collections::BTreeMap;

include!(concat!(env!("OUT_DIR"), "/chrome-digests.rs"));

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum Recipe {
    HeaderV1,
    LanczosV1,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct ImageKey {
    source: [u8; 32],
    width: u32,
    height: u32,
    recipe: Recipe,
}

struct Frame {
    caps: [RgbaImage; 3],
    divider: Option<RasterImage>,
    digest: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_symbols_keep_reference_footprints_when_scaled() {
        let artwork = Artwork::new().unwrap();
        for scale in [1.0_f32, 1.25, 2.0] {
            for (name, width, height, ink_width, ink_height) in [
                ("estimate-help", 12.0, 12.0, 12.0, 12.0),
                ("estimate-approximation", 9.0, 16.0, 8.11, 6.29),
                ("search-error", 16.0, 16.0, 16.0, 16.0),
            ] {
                let rect = Rect {
                    x: 0,
                    y: 0,
                    width: (width * scale).round() as u32,
                    height: (height * scale).round() as u32,
                };
                let mut pixels = vec![0; (rect.width * rect.height * 4) as usize];
                let mut painter = Painter::new(&mut pixels, rect.width, rect.height).unwrap();
                artwork.icon(&mut painter, rect, name, false);
                painter.finish().unwrap();
                let points: Vec<_> = pixels
                    .chunks_exact(4)
                    .enumerate()
                    .filter(|(_, pixel)| pixel[3] >= 64)
                    .map(|(index, _)| (index as u32 % rect.width, index as u32 / rect.width))
                    .collect();
                let width = points.iter().map(|p| p.0).max().unwrap()
                    - points.iter().map(|p| p.0).min().unwrap()
                    + 1;
                let height = points.iter().map(|p| p.1).max().unwrap()
                    - points.iter().map(|p| p.1).min().unwrap()
                    + 1;
                assert!(
                    (width as f32 - ink_width * scale).abs() <= 1.0,
                    "{name} width at {scale}: {width}"
                );
                assert!(
                    (height as f32 - ink_height * scale).abs() <= 1.0,
                    "{name} height at {scale}: {height}"
                );
            }
        }
    }

    #[test]
    fn profile_radios_preserve_the_frame_and_fill_only_the_selected_center() {
        let mut artwork = Artwork::new().unwrap();
        for size in [16, 20, 32] {
            let rect = Rect {
                x: 2,
                y: 2,
                width: size,
                height: size,
            };
            let extent = size + 4;
            let mut pixels = vec![0; (extent * extent * 4) as usize];
            for selected in [false, true] {
                let mut painter = Painter::new(&mut pixels, extent, extent).unwrap();
                painter.clear();
                artwork.prepare_controls((24, 20), (size, size)).unwrap();
                artwork.profile_radio(&mut painter, rect, selected);
                painter.finish().unwrap();
                let center = ((rect.y + size / 2) * extent + rect.x + size / 2) as usize * 4;
                if selected {
                    assert!(pixels[center + 3] >= 240);
                } else {
                    assert_eq!(pixels[center + 3], 0);
                }
                assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
                let images = artwork.profile_scaled.as_ref().unwrap();
                assert!(
                    images
                        .iter()
                        .all(|image| image.byte_len() == (size * size * 4) as usize)
                );
                assert!(
                    pixels[..extent as usize * 4]
                        .iter()
                        .all(|value| *value == 0)
                );
            }
            let misses = artwork.cache_stats().misses;
            artwork.prepare_controls((24, 20), (size, size)).unwrap();
            let mut painter = Painter::new(&mut pixels, extent, extent).unwrap();
            artwork.profile_radio(&mut painter, rect, true);
            assert_eq!(artwork.cache_stats().misses, misses);
            painter.finish().unwrap();
        }
    }

    #[test]
    fn search_ornaments_cache_display_sized_rasters_for_both_states() {
        let mut artwork = Artwork::new().unwrap();
        for size in [(24, 20), (30, 25), (48, 40)] {
            let rects = [0, 60].map(|x| Rect {
                x,
                y: 0,
                width: size.0,
                height: size.1,
            });
            let mut pixels = vec![0; 120 * 40 * 4];
            let mut painter = Painter::new(&mut pixels, 120, 40).unwrap();
            artwork.prepare_controls(size, (16, 16)).unwrap();
            artwork.search_ornaments(&mut painter, rects, false);
            let images = artwork.search_scaled.as_ref().unwrap();
            assert!(
                images
                    .iter()
                    .all(|image| image.byte_len() == (size.0 * size.1 * 4) as usize)
            );
            let misses = artwork.cache_stats().misses;
            artwork.search_ornaments(&mut painter, rects, true);
            assert_eq!(artwork.cache_stats().misses, misses);
            painter.finish().unwrap();
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
        }
    }

    #[test]
    fn headers_resample_at_display_size_and_reuse_the_last_composition() {
        let mut artwork = Artwork::new().unwrap();
        assert_eq!(artwork.frames.len(), 9);
        for (rarity, double) in [
            ("Normal", false),
            ("Magic", false),
            ("Rare", true),
            ("Unique", true),
            ("Gem", false),
            ("Currency", false),
            ("Quest", false),
        ] {
            for scale in [1.0, 1.25, 1.5] {
                let rect = Rect {
                    x: 0,
                    y: 0,
                    width: (400.0 * scale) as u32,
                    height: ((if double { 54.0 } else { 34.0 }) * scale) as u32,
                };
                let mut pixels = vec![0; rect.width as usize * rect.height as usize * 4];
                let mut painter = Painter::new(&mut pixels, rect.width, rect.height).unwrap();
                artwork.begin_frame();
                artwork.prepare_header(rect, rarity, double).unwrap();
                artwork.header(&mut painter, rect, rarity, double);
                let bytes = artwork.headers[0].1.byte_len();
                assert_eq!(bytes, rect.width as usize * rect.height as usize * 4);
                painter.finish().unwrap();
                assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
                let misses = artwork.cache_stats().misses;
                artwork.begin_frame();
                artwork.prepare_header(rect, rarity, double).unwrap();
                let mut painter = Painter::new(&mut pixels, rect.width, rect.height).unwrap();
                artwork.header(&mut painter, rect, rarity, double);
                painter.finish().unwrap();
                assert_eq!(artwork.cache_stats().misses, misses);
            }
        }
    }

    #[test]
    fn item_and_listing_headers_reuse_both_compositions() {
        let mut artwork = Artwork::new().unwrap();
        let mut pixels = vec![0; 400 * 54 * 4];
        let mut painter = Painter::new(&mut pixels, 400, 54).unwrap();
        let rect = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 54,
        };
        for double in [true, false] {
            artwork
                .prepare_header(
                    Rect {
                        height: if double { 54 } else { 34 },
                        ..rect
                    },
                    "Rare",
                    double,
                )
                .unwrap();
        }
        let misses = artwork.cache_stats().misses;
        for double in [true, false, true, false] {
            artwork.header(
                &mut painter,
                Rect {
                    height: if double { 54 } else { 34 },
                    ..rect
                },
                "Rare",
                double,
            );
            assert_eq!(artwork.cache_stats().misses, misses);
        }
        painter.finish().unwrap();
    }

    #[test]
    fn physical_size_source_revision_and_recipe_are_distinct_keys() {
        let mut artwork = Artwork::new().unwrap();
        let rect = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 54,
        };
        artwork.prepare_header(rect, "Rare", true).unwrap();
        let original = artwork.headers[0].0;
        artwork.frames.get_mut("rare-double").unwrap().digest[0] ^= 1;
        artwork.prepare_header(rect, "Rare", true).unwrap();
        assert_ne!(original.source, artwork.headers[1].0.source);
        artwork
            .prepare_header(
                Rect {
                    width: 500,
                    height: 68,
                    ..rect
                },
                "Rare",
                true,
            )
            .unwrap();
        assert_eq!(artwork.cache_stats().entries, 3);
        assert!(artwork.cache.get(&original).is_some());
        assert!(
            artwork
                .cache
                .get(&ImageKey {
                    recipe: Recipe::LanczosV1,
                    ..original
                })
                .is_none()
        );
    }
}

pub struct Artwork {
    frames: BTreeMap<&'static str, Frame>,
    icons: BTreeMap<&'static str, [RasterImage; 2]>,
    cache: ResourceCache<ImageKey, RasterImage>,
    headers: Vec<(ImageKey, Resource<RasterImage>)>,
    search_caps: [RgbaImage; 4],
    search_scaled: Option<[Resource<RasterImage>; 4]>,
    profile_icons: [RgbaImage; 2],
    profile_scaled: Option<[Resource<RasterImage>; 2]>,
}

impl Artwork {
    pub fn new() -> Result<Self, String> {
        let mut frames = BTreeMap::new();
        macro_rules! load {
            ($name:literal, $divider:expr) => {{
                let decode = |bytes: &[u8]| {
                    image::load_from_memory(bytes)
                        .map(|image| image.into_rgba8())
                        .map_err(|error| error.to_string())
                };
                frames.insert(
                    $name,
                    Frame {
                        digest: source_digest($name),
                        caps: [
                            decode(include_bytes!(concat!(
                                "../../assets/frames/",
                                $name,
                                "-left.png"
                            )))?,
                            decode(include_bytes!(concat!(
                                "../../assets/frames/",
                                $name,
                                "-center.png"
                            )))?,
                            decode(include_bytes!(concat!(
                                "../../assets/frames/",
                                $name,
                                "-right.png"
                            )))?,
                        ],
                        divider: $divider
                            .map(decode)
                            .transpose()?
                            .map(RasterImage::from_rgba)
                            .transpose()
                            .map_err(|e| e.to_string())?,
                    },
                );
            }};
        }
        macro_rules! rarity {
            ($name:literal) => {
                load!(
                    $name,
                    Some(
                        include_bytes!(concat!("../../assets/frames/", $name, "-divider.png"))
                            .as_slice()
                    )
                );
            };
        }
        rarity!("normal");
        rarity!("magic");
        rarity!("rare");
        rarity!("unique");
        rarity!("gem");
        rarity!("currency");
        rarity!("quest");
        load!("rare-double", None::<&[u8]>);
        load!("unique-double", None::<&[u8]>);
        let mut icons = BTreeMap::new();
        for (name, bytes) in [
            (
                "market",
                include_bytes!("../../assets/frames/market.png").as_slice(),
            ),
            (
                "settings",
                include_bytes!("../../assets/frames/settings.png").as_slice(),
            ),
            (
                "close",
                include_bytes!("../../assets/frames/close.png").as_slice(),
            ),
            (
                "listing-eye",
                include_bytes!("../../assets/frames/listing-eye.png").as_slice(),
            ),
            (
                "refresh",
                include_bytes!("../../assets/frames/refresh.png").as_slice(),
            ),
            (
                "estimate-help",
                include_bytes!("../../assets/frames/estimate-help.png").as_slice(),
            ),
            (
                "estimate-approximation",
                include_bytes!("../../assets/frames/estimate-approximation.png").as_slice(),
            ),
            (
                "search-error",
                include_bytes!("../../assets/frames/search-error.png").as_slice(),
            ),
        ] {
            let source = image::load_from_memory(bytes)
                .map_err(|e| e.to_string())?
                .into_rgba8();
            let idle = match name {
                "listing-eye" | "refresh" => [159, 157, 138],
                "estimate-approximation" => [203, 201, 191],
                "search-error" => [232, 13, 13],
                _ => [115, 113, 94],
            };
            let [idle_image, active_image] = [idle, [230, 230, 223]].map(|rgb| {
                let mut image = source.clone();
                for pixel in image.pixels_mut() {
                    pixel.0[..3].copy_from_slice(&rgb);
                }
                RasterImage::from_rgba(image)
            });
            icons.insert(
                name,
                [
                    idle_image.map_err(|e| e.to_string())?,
                    active_image.map_err(|e| e.to_string())?,
                ],
            );
        }
        let decode = |bytes: &[u8]| {
            image::load_from_memory(bytes)
                .map(|image| image.into_rgba8())
                .map_err(|error| error.to_string())
        };
        Ok(Self {
            frames,
            icons,
            cache: ResourceCache::new(8 * 1024 * 1024),
            headers: Vec::with_capacity(2),
            search_caps: [
                decode(include_bytes!("../../assets/frames/search-left-idle.png"))?,
                decode(include_bytes!("../../assets/frames/search-right-idle.png"))?,
                decode(include_bytes!(
                    "../../assets/frames/search-left-highlight.png"
                ))?,
                decode(include_bytes!(
                    "../../assets/frames/search-right-highlight.png"
                ))?,
            ],
            search_scaled: None,
            profile_icons: [
                decode(include_bytes!("../../assets/frames/profile-radio.png"))?,
                decode(include_bytes!(
                    "../../assets/frames/profile-radio-selected.png"
                ))?,
            ],
            profile_scaled: None,
        })
    }

    #[cfg(test)]
    pub fn cache_stats(&self) -> retinue_ui::resources::CacheStats {
        self.cache.stats()
    }

    pub fn begin_frame(&mut self) {
        self.headers.clear();
    }

    fn frame_name(rarity: &str, double: bool) -> String {
        let mut name = rarity.to_ascii_lowercase();
        if double && matches!(name.as_str(), "rare" | "unique") {
            name.push_str("-double");
        }
        name
    }

    pub fn prepare_header(&mut self, rect: Rect, rarity: &str, double: bool) -> Result<(), String> {
        let frame = self
            .frames
            .get(Self::frame_name(rarity, double).as_str())
            .unwrap_or(&self.frames["normal"]);
        let key = ImageKey {
            source: frame.digest,
            width: rect.width,
            height: rect.height,
            recipe: Recipe::HeaderV1,
        };
        let image = self
            .cache
            .try_get_or_insert_with(key, || {
                let resize = |image: &RgbaImage| {
                    imageops::resize(
                        image,
                        (image.width() as f64 * rect.height as f64 / image.height() as f64)
                            .round()
                            .max(1.0) as u32,
                        rect.height,
                        imageops::FilterType::Lanczos3,
                    )
                };
                let [left, center, right] = frame.caps.each_ref().map(resize);
                let mut image = RgbaImage::new(rect.width, rect.height);
                // CSS uses fixed-aspect caps over a repeating, centered middle strip.
                let center_x = (rect.width as i64 - center.width() as i64) / 2;
                let mut x = center_x.rem_euclid(center.width() as i64) - center.width() as i64;
                while x < rect.width as i64 {
                    imageops::overlay(&mut image, &center, x, 0);
                    x += center.width() as i64;
                }
                imageops::overlay(
                    &mut image,
                    &right,
                    rect.width as i64 - right.width() as i64,
                    0,
                );
                imageops::overlay(&mut image, &left, 0, 0);
                RasterImage::from_rgba(image)
            })
            .map_err(|e| e.to_string())?;
        self.headers.push((key, image));
        Ok(())
    }

    pub fn header(&self, painter: &mut Painter<'_>, rect: Rect, rarity: &str, double: bool) {
        let frame = self
            .frames
            .get(Self::frame_name(rarity, double).as_str())
            .unwrap_or(&self.frames["normal"]);
        let key = ImageKey {
            source: frame.digest,
            width: rect.width,
            height: rect.height,
            recipe: Recipe::HeaderV1,
        };
        let image = &self
            .headers
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .expect("header prepared before paint")
            .1;
        painter.draw_image(image, rect.x, rect.y, rect.width, rect.height);
    }

    pub fn divider(&self, painter: &mut Painter<'_>, rect: Rect, rarity: &str, scale: f32) {
        if let Some(image) = self
            .frames
            .get(rarity.to_ascii_lowercase().as_str())
            .unwrap_or(&self.frames["normal"])
            .divider
            .as_ref()
        {
            painter.draw_image_contained(
                image,
                rect.x,
                rect.y,
                rect.width,
                (8.0 * scale).round() as u32,
            );
        }
    }

    pub fn icon(&self, painter: &mut Painter<'_>, rect: Rect, name: &str, hovered: bool) {
        if let Some(images) = self.icons.get(name) {
            painter.draw_image_contained(
                &images[usize::from(hovered)],
                rect.x,
                rect.y,
                rect.width,
                rect.height,
            );
        }
    }

    pub fn search_ornaments(&self, painter: &mut Painter<'_>, rects: [Rect; 2], highlighted: bool) {
        let images = self
            .search_scaled
            .as_ref()
            .expect("controls prepared before paint");
        for (side, rect) in rects.into_iter().enumerate() {
            painter.draw_image(
                &images[usize::from(highlighted) * 2 + side],
                rect.x,
                rect.y,
                rect.width,
                rect.height,
            );
        }
    }

    pub fn profile_radio(&self, painter: &mut Painter<'_>, rect: Rect, selected: bool) {
        painter.draw_image(
            &self
                .profile_scaled
                .as_ref()
                .expect("controls prepared before paint")[usize::from(selected)],
            rect.x,
            rect.y,
            rect.width,
            rect.height,
        );
    }

    pub fn prepare_controls(
        &mut self,
        search: (u32, u32),
        radio: (u32, u32),
    ) -> Result<(), String> {
        let mut scaled = |source: &RgbaImage, name: &str, size: (u32, u32)| {
            self.cache
                .try_get_or_insert_with(
                    ImageKey {
                        source: source_digest(name),
                        width: size.0,
                        height: size.1,
                        recipe: Recipe::LanczosV1,
                    },
                    || {
                        RasterImage::from_rgba(imageops::resize(
                            source,
                            size.0,
                            size.1,
                            imageops::FilterType::Lanczos3,
                        ))
                    },
                )
                .map_err(|e| e.to_string())
        };
        let search_images: [_; 4] = std::array::from_fn(|index| {
            scaled(
                &self.search_caps[index],
                [
                    "search-left-idle",
                    "search-right-idle",
                    "search-left-highlight",
                    "search-right-highlight",
                ][index],
                search,
            )
        });
        let [a, b, c, d] = search_images;
        self.search_scaled = Some([a?, b?, c?, d?]);
        let profile_images: [_; 2] = std::array::from_fn(|index| {
            scaled(
                &self.profile_icons[index],
                ["profile-radio", "profile-radio-selected"][index],
                radio,
            )
        });
        let [a, b] = profile_images;
        self.profile_scaled = Some([a?, b?]);
        Ok(())
    }
}
