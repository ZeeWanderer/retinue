use super::{FontData, TextError};
use read_fonts::{FontRef, TableProvider, model, tables::layout::ScriptTags, types::Tag};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hinting {
    None,
    Light,
    Full,
}

/// A validated static outline face. Font bytes are shared with the shaper.
#[derive(Clone)]
pub struct Font {
    pub(super) data: FontData,
    pub(super) metrics: model::Font,
}

impl Font {
    pub fn new(data: FontData) -> Result<Self, TextError> {
        let raw = FontRef::from_index(data.data.data(), data.index)
            .map_err(|_| TextError::InvalidFont)?;
        let has = |tag| raw.table_data(Tag::new(tag)).is_some();
        if has(b"fvar") || has(b"CFF2") || has(b"VARC") {
            return Err(TextError::UnsupportedFont("variable outlines"));
        }
        if [b"COLR", b"SVG ", b"sbix", b"CBDT", b"EBDT"]
            .into_iter()
            .any(has)
        {
            return Err(TextError::UnsupportedFont("color or bitmap glyphs"));
        }
        if !(has(b"glyf") && has(b"loca") || has(b"CFF ")) {
            return Err(TextError::UnsupportedFont("no TrueType or CFF outlines"));
        }
        let (shared, _) = data.data.clone().into_raw_parts();
        let metrics = model::Font::new(shared, data.index).ok_or(TextError::InvalidFont)?;
        if metrics.units_per_em() == 0 {
            return Err(TextError::InvalidFont);
        }
        Ok(Self { data, metrics })
    }

    pub fn data(&self) -> &FontData {
        &self.data
    }

    pub fn identity(&self) -> (u64, u32) {
        (self.data.data.id(), self.data.index)
    }

    pub fn contains(&self, character: char) -> bool {
        self.metrics
            .charmap()
            .map_unicode(character)
            .is_some_and(|id| id.to_u32() != 0)
    }

    pub fn has_script_substitutions(&self, iso15924: [u8; 4]) -> bool {
        let tags = ScriptTags::from_unicode(Tag::new(&iso15924));
        FontRef::from_index(self.data.data.data(), self.data.index)
            .and_then(|face| face.gsub()?.script_list())
            .is_ok_and(|scripts| tags.iter().any(|&tag| scripts.index_for_tag(tag).is_some()))
    }
}

impl PartialEq for Font {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Font").field(&self.identity()).finish()
    }
}
