use super::*;
use crate::resources::{CacheStats, ResourceCache, ResourceSize};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, ops::Deref, rc::Rc};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct GlyphKey {
    source: [u8; 32],
    collection: u32,
    geometry_scale: u32,
    substitutions: bool,
    glyph: u16,
    size: u32,
}

struct Glyph {
    metrics: fontdue::Metrics,
    mask: Option<GlyphMask>,
}

pub struct GlyphMask(pub(super) blend2d::Image);

impl GlyphMask {
    pub fn from_coverage(coverage: Vec<u8>, width: u32, height: u32) -> Result<Self, PainterError> {
        blend2d::Image::new(coverage, width, height, true).map(Self)
    }
}

impl Glyph {
    fn new((metrics, coverage): (fontdue::Metrics, Vec<u8>)) -> Result<Self, PainterError> {
        let mask = if metrics.width == 0 || metrics.height == 0 {
            None
        } else {
            Some(GlyphMask::from_coverage(
                coverage,
                metrics.width as u32,
                metrics.height as u32,
            )?)
        };
        Ok(Self { metrics, mask })
    }
}

impl ResourceSize for Glyph {
    fn heap_bytes(&self) -> usize {
        self.mask.as_ref().map_or(0, |mask| mask.0.heap_bytes())
    }
}

#[derive(Clone)]
pub struct GlyphCache(Rc<RefCell<ResourceCache<GlyphKey, Glyph>>>);

impl GlyphCache {
    pub fn new(budget: usize) -> Self {
        Self(Rc::new(RefCell::new(ResourceCache::new(budget))))
    }

    pub fn stats(&self) -> CacheStats {
        self.0.borrow().stats()
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

pub struct CachedFont {
    face: Font,
    key: GlyphKey,
    cache: GlyphCache,
}

impl CachedFont {
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    pub fn from_bytes(
        bytes: &[u8],
        settings: FontSettings,
        cache: &GlyphCache,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            face: Font::from_bytes(bytes, settings)?,
            key: GlyphKey {
                source: Sha256::digest(bytes).into(),
                collection: settings.collection_index,
                geometry_scale: settings.scale.to_bits(),
                substitutions: settings.load_substitutions,
                glyph: 0,
                size: 0,
            },
            cache: cache.clone(),
        })
    }
}

impl Deref for CachedFont {
    type Target = Font;

    fn deref(&self) -> &Font {
        &self.face
    }
}

pub trait LabelFont {
    fn face(&self) -> &Font;
    fn with_glyph(
        &self,
        character: char,
        size: f32,
        draw: impl FnOnce(fontdue::Metrics, Option<&GlyphMask>),
    ) -> Result<(), PainterError>;
}

impl<T: LabelFont> LabelFont for &T {
    fn face(&self) -> &Font {
        T::face(self)
    }

    fn with_glyph(
        &self,
        character: char,
        size: f32,
        draw: impl FnOnce(fontdue::Metrics, Option<&GlyphMask>),
    ) -> Result<(), PainterError> {
        T::with_glyph(self, character, size, draw)
    }
}

impl LabelFont for Font {
    fn face(&self) -> &Font {
        self
    }

    fn with_glyph(
        &self,
        character: char,
        size: f32,
        draw: impl FnOnce(fontdue::Metrics, Option<&GlyphMask>),
    ) -> Result<(), PainterError> {
        let glyph = Glyph::new(self.rasterize(character, size))?;
        draw(glyph.metrics, glyph.mask.as_ref());
        Ok(())
    }
}

impl LabelFont for CachedFont {
    fn face(&self) -> &Font {
        &self.face
    }

    fn with_glyph(
        &self,
        character: char,
        size: f32,
        draw: impl FnOnce(fontdue::Metrics, Option<&GlyphMask>),
    ) -> Result<(), PainterError> {
        let key = GlyphKey {
            glyph: self.face.lookup_glyph_index(character),
            size: size.to_bits(),
            ..self.key
        };
        let mut cache = self.cache.0.borrow_mut();
        let glyph = match cache.get(&key) {
            Some(glyph) => glyph,
            None => cache.insert(
                key,
                Glyph::new(self.face.rasterize_indexed(key.glyph, size))?,
            ),
        };
        drop(cache);
        draw(glyph.metrics, glyph.mask.as_ref());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: &[u8] = include_bytes!("../../../../assets/fonts/roboto/Roboto-Regular.ttf");

    #[test]
    fn cached_text_matches_uncached_pixels_at_fractional_sizes() {
        let plain = Font::from_bytes(BYTES, FontSettings::default()).unwrap();
        let cache = GlyphCache::new(1024 * 1024);
        let cached = CachedFont::from_bytes(BYTES, FontSettings::default(), &cache).unwrap();
        for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
            let mut a = vec![0; 420 * 140 * 4];
            let mut b = a.clone();
            let paint = |p: &mut Painter<'_>, font: &Font| {
                p.draw_text(font, 7, 3, 13.0 * scale, "AV To 3.5\nCaf\u{e9}", [255; 4]);
                p.draw_centered_text(
                    font,
                    TextBox::new(0, 65, 400),
                    17.0 * scale,
                    "WTS: 42",
                    [123; 4],
                );
            };
            let mut p = Painter::new(&mut a, 420, 140).unwrap();
            paint(&mut p, &plain);
            p.finish().unwrap();
            let mut p = Painter::new(&mut b, 420, 140).unwrap();
            p.draw_text(
                &cached,
                7,
                3,
                13.0 * scale,
                "AV To 3.5\nCaf\u{e9}",
                [255; 4],
            );
            p.draw_centered_text(
                &cached,
                TextBox::new(0, 65, 400),
                17.0 * scale,
                "WTS: 42",
                [123; 4],
            );
            p.finish().unwrap();
            assert_eq!(a, b, "scale {scale}");
        }
        let misses = cache.stats().misses;
        cached.with_glyph('A', 13.0, |_, _| {}).unwrap();
        assert_eq!(cache.stats().misses, misses);
    }

    #[test]
    fn cache_identity_includes_font_settings_and_actual_pixel_size() {
        let cache = GlyphCache::new(1024 * 1024);
        for geometry_scale in [40.0, 48.0] {
            let font = CachedFont::from_bytes(
                BYTES,
                FontSettings {
                    scale: geometry_scale,
                    ..FontSettings::default()
                },
                &cache,
            )
            .unwrap();
            for size in [16.0, 16.8, 20.0] {
                font.with_glyph('A', size, |_, _| {}).unwrap();
            }
        }
        assert_eq!(cache.stats().entries, 6);
    }

    #[test]
    fn queued_glyphs_survive_immediate_cache_eviction() {
        let mut reference = None;
        for threads in [1, 2, 4] {
            let cache = GlyphCache::new(0);
            let font = CachedFont::from_bytes(BYTES, FontSettings::default(), &cache).unwrap();
            let mut pixels = vec![0; 400 * 120 * 4];
            let mut painter = Painter::with_threads(
                &mut pixels,
                400,
                120,
                std::num::NonZeroU32::new(threads).unwrap(),
            )
            .unwrap();
            painter.draw_text(&font, 7, 3, 23.75, "Queued AV 3.5\nCaf\u{e9}", [255; 4]);
            assert_eq!(cache.stats().entries, 0);
            drop(font);
            drop(cache);
            painter.finish().unwrap();
            if let Some(reference) = &reference {
                assert!(&pixels == reference, "pixel mismatch: threads {threads}");
            } else {
                reference = Some(pixels);
            }
        }
    }
}
