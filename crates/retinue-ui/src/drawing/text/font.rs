use super::shaping::ShapedText;
use parley::{
    FontContext, FontFamily, Layout, LayoutContext, OverflowWrap, StyleProperty,
    editing::{PlainEditor, PlainEditorDriver},
    fontique::{Collection, CollectionOptions, FontInfo},
};
use retinue_render::{
    painter::CachedFont,
    text::{Font, GlyphRun, TextError},
};
use rustc_hash::FxHashMap;
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::{Rc, Weak},
    sync::Arc,
};
mod fallback;
#[cfg(target_os = "linux")]
pub(super) use fallback::prepare_host_fonts;

thread_local! {
    static FACES: RefCell<FxHashMap<(u64, u32), Weak<TextFace>>> = RefCell::default();
    static SHAPER: RefCell<LayoutContext<()>> = RefCell::new(LayoutContext::new());
    static WORKER_FACES: RefCell<VecDeque<Rc<TextFace>>> = RefCell::default();
}

const WORKER_FACE_CAPACITY: usize = 16;

pub(crate) struct TextFace {
    native: Font,
    fonts: RefCell<FontContext>,
    family: String,
    info: FontInfo,
    ascii_covered: bool,
    fallbacks: RefCell<fallback::Fallbacks>,
}

impl TextFace {
    pub fn editor(&self) -> PlainEditor<()> {
        let mut editor = PlainEditor::new(12.0);
        editor.set_quantize(false);
        let styles = editor.edit_styles();
        styles.insert(StyleProperty::FontFamily(
            FontFamily::named(&self.family).into_owned(),
        ));
        styles.insert(StyleProperty::FontWeight(self.info.weight()));
        styles.insert(StyleProperty::FontStyle(self.info.style()));
        styles.insert(StyleProperty::FontWidth(self.info.width()));
        editor
    }

    pub fn edit<T>(
        &self,
        editor: &mut PlainEditor<()>,
        edit: impl FnOnce(&mut PlainEditorDriver<'_, ()>) -> T,
    ) -> T {
        SHAPER.with(|context| {
            edit(&mut editor.driver(&mut self.fonts.borrow_mut(), &mut context.borrow_mut()))
        })
    }

    pub fn glyphs(&self, layout: &Layout<()>) -> Result<Vec<GlyphRun>, TextError> {
        ShapedText::resolve(layout, &mut self.resolved_fonts(layout)?, None)
    }

    fn resolved_fonts(
        &self,
        layout: &Layout<()>,
    ) -> Result<std::collections::HashMap<(u64, u32), Font>, TextError> {
        use std::collections::hash_map::Entry;
        let allowed = self.fallbacks.borrow();
        let mut fonts =
            std::collections::HashMap::from([(self.native.identity(), self.native.clone())]);
        for run in layout.lines().flat_map(|line| line.runs()) {
            let id = (run.font().data.id(), run.font().index);
            if id != self.native.identity()
                && let Entry::Vacant(entry) = fonts.entry(id)
            {
                entry.insert(
                    allowed
                        .get(id)
                        .ok_or(TextError::UnsupportedFont("requested face was substituted"))?
                        .clone(),
                );
            }
        }
        Ok(fonts)
    }

    pub fn for_font(font: &CachedFont) -> Result<Rc<Self>, TextError> {
        Self::for_native(font.native())
    }

    #[cfg(test)]
    pub(crate) fn register_fallback(&self, cluster: &str, font: Font) {
        self.fallbacks
            .borrow_mut()
            .use_for_test(self, cluster, font)
    }

    pub(super) fn cached(native: &Font) -> Result<Rc<Self>, TextError> {
        WORKER_FACES.with(|recent| {
            let mut recent = recent.borrow_mut();
            if let Some(index) = recent.iter().position(|face| face.native == *native) {
                let face = recent.remove(index).unwrap();
                recent.push_front(face.clone());
                return Ok(face);
            }
            let face = Self::for_native(native.clone())?;
            recent.push_front(face.clone());
            recent.truncate(WORKER_FACE_CAPACITY);
            Ok(face)
        })
    }

    fn for_native(native: Font) -> Result<Rc<Self>, TextError> {
        FACES.with(|faces| {
            let mut faces = faces.borrow_mut();
            if let Some(face) = faces.get(&native.identity()).and_then(Weak::upgrade) {
                return Ok(face);
            }
            faces.retain(|_, face| face.strong_count() != 0);
            let mut fonts = FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                ..FontContext::default()
            };
            let selected = fonts
                .collection
                .register_fonts(native.data().data.clone(), None)
                .into_iter()
                .find_map(|(family, fonts)| {
                    fonts
                        .into_iter()
                        .find(|font| font.index() == native.data().index)
                        .map(|font| (family, font))
                })
                .ok_or(TextError::InvalidFont)?;
            let family = fonts
                .collection
                .family_name(selected.0)
                .ok_or(TextError::InvalidFont)?
                .to_owned();
            let face = Rc::new(Self {
                ascii_covered: (' '..='~').all(|c| native.contains(c)),
                fallbacks: Default::default(),
                native,
                fonts: RefCell::new(fonts),
                family,
                info: selected.1,
            });
            faces.insert(face.native.identity(), Rc::downgrade(&face));
            Ok(face)
        })
    }

    pub fn families(&self, text: &str) -> FontFamily<'_> {
        if self.ascii_covered && text.is_ascii() {
            return FontFamily::named(&self.family);
        }
        self.fallbacks.borrow_mut().families(self, text)
    }

    pub fn shape(&self, text: &str, size: f32) -> Result<ShapedText, TextError> {
        if !size.is_finite() || size <= 0.0 {
            return Err(TextError::InvalidSize);
        }
        let families = self.families(text);
        SHAPER.with(|context| {
            let mut context = context.borrow_mut();
            let mut fonts = self.fonts.borrow_mut();
            let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
            builder.push_default(StyleProperty::FontFamily(families));
            builder.push_default(StyleProperty::FontWeight(self.info.weight()));
            builder.push_default(StyleProperty::FontStyle(self.info.style()));
            builder.push_default(StyleProperty::FontWidth(self.info.width()));
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
            let mut layout = builder.build(text);
            layout.break_all_lines(None);
            let resolved = self.resolved_fonts(&layout)?;
            ShapedText::from_resolved(text, layout, resolved)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_render::painter::FontFaces;

    #[test]
    fn live_consumers_share_registration_without_pinning_dead_faces() {
        let bytes = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let faces = FontFaces::default();

        let first = faces.load(bytes, 0).unwrap();
        let second = faces.load(bytes, 0).unwrap();
        let a = TextFace::for_font(&first).unwrap();
        let b = TextFace::for_font(&second).unwrap();
        assert!(Rc::ptr_eq(&a, &b));
        let weak = Rc::downgrade(&a);
        drop((a, b));
        assert!(weak.upgrade().is_none());
        assert!(TextFace::for_font(&first).is_ok());
    }

    #[test]
    fn a_bold_face_is_not_replaced_by_regular_or_synthesized() {
        let bytes = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Bold.ttf");
        let font = CachedFont::from_bytes(bytes).unwrap();
        let face = TextFace::for_font(&font).unwrap();
        let shaped = face.shape("office", 19.25).unwrap();
        assert!(
            shaped
                .runs()
                .iter()
                .all(|run| run.font().identity() == font.native().identity())
        );
    }

    #[test]
    fn a_missing_glyph_preserves_surrounding_text_and_uses_the_requested_face() {
        let bytes = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let font = CachedFont::from_bytes(bytes).unwrap();
        let face = TextFace::for_font(&font).unwrap();
        // A noncharacter remains missing even if additional fallback fonts are installed.
        let shaped = face.shape("A\u{ffff}\u{407}", 18.0).unwrap();
        let ids: Vec<_> = shaped
            .runs()
            .iter()
            .flat_map(|r| r.glyphs().iter().map(|g| g.id))
            .collect();
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], 0);
        assert_eq!(ids[1], 0);
        assert_ne!(ids[2], 0);
        shaped.prepare().unwrap();
    }

    #[test]
    fn same_family_with_more_coverage_does_not_replace_the_primary() {
        let face = |data: &[u8]| {
            let font = CachedFont::from_bytes(data).unwrap();
            TextFace::for_font(&font).unwrap()
        };
        let primary = face(include_bytes!(
            "../../../../../apps/poe2companion/assets/Lato-Semibold.ttf"
        ));
        let fallback = face(include_bytes!(
            "../../../../../apps/poe2companion/assets/Lato-Regular.ttf"
        ));
        assert_eq!(primary.family, fallback.family);
        assert!(!primary.native.contains('\u{100}'));
        assert!(fallback.native.contains('\u{100}'));
        primary.register_fallback("\u{100}", fallback.native.clone());
        let shaped = primary.shape("A\u{100}Z", 18.0).unwrap();
        let identities: Vec<_> = shaped.runs().iter().map(|r| r.font().identity()).collect();
        assert_eq!(
            identities,
            [
                primary.native.identity(),
                fallback.native.identity(),
                primary.native.identity()
            ]
        );
        assert!(
            shaped
                .runs()
                .iter()
                .flat_map(|r| r.glyphs())
                .all(|g| g.id != 0)
        );
    }

    #[test]
    fn worker_registrations_are_shared_and_bounded() {
        std::thread::spawn(|| {
            let bytes = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");

            let font = CachedFont::from_bytes(bytes).unwrap();
            let native = font.native();
            let face = TextFace::cached(&native).unwrap();
            let weak = Rc::downgrade(&face);
            drop(face);
            assert!(weak.upgrade().is_some());
            assert!(Rc::ptr_eq(
                &weak.upgrade().unwrap(),
                &TextFace::cached(&native).unwrap()
            ));
            for _ in 0..WORKER_FACE_CAPACITY {
                let font = CachedFont::from_bytes(bytes).unwrap();
                TextFace::cached(&font.native()).unwrap();
            }
            WORKER_FACES.with(|recent| assert_eq!(recent.borrow().len(), WORKER_FACE_CAPACITY));
            assert!(weak.upgrade().is_none());
        })
        .join()
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires installed outline fonts covering Han and emoji"]
    fn installed_outline_fallback_paints_mixed_text() {
        use retinue_render::{Color, painter::Painter};
        let bytes = include_bytes!("../../../../../assets/fonts/roboto/Roboto-Regular.ttf");
        let font = CachedFont::from_bytes(bytes).unwrap();
        let face = TextFace::for_font(&font).unwrap();
        for c in ['\u{6f22}', '\u{1f600}'] {
            let mixed = face.shape(&format!("A{c}Z"), 24.0).unwrap();
            assert!(mixed.runs().iter().any(|r| r.font() == &face.native));
            let glyph = face.shape(&c.to_string(), 24.0).unwrap();
            assert!(glyph.runs().iter().all(|r| r.font() != &face.native));
            assert!(
                glyph
                    .runs()
                    .iter()
                    .flat_map(|r| r.glyphs())
                    .all(|g| g.id != 0)
            );
            let mut pixels = vec![0; 80 * 80 * 4];
            let mut p = Painter::new(&mut pixels, 80, 80).unwrap();
            glyph
                .prepare()
                .unwrap()
                .draw(&mut p, (4.0, 4.0), 1.0, Color::rgb(0xffffff));
            p.finish().unwrap();
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0), "{c}");
        }
    }
}
