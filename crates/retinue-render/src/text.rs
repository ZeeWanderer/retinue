//! Immutable shaped text and shared native font leases.
//!
//! Positions are baseline origins in logical, y-down coordinates. Shaping belongs
//! to the UI layer; submission never remaps glyph IDs or recomputes advances.

mod font;
pub(crate) mod preparation;
mod run;

pub use crate::painter::blend2d::font::PreparedFont;
pub use font::{Font, Hinting};
pub use linebender_resource_handle::{Blob, FontData};
pub use run::{Glyph, GlyphRun};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    InvalidFont,
    UnsupportedFont(&'static str),
    UnsupportedLayout(&'static str),
    UnsupportedHinting,
    InvalidSize,
    InvalidGlyph,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFont => f.write_str("invalid font or collection index"),
            Self::UnsupportedFont(reason) => write!(f, "unsupported native font: {reason}"),
            Self::UnsupportedLayout(reason) => write!(f, "unsupported text layout: {reason}"),
            Self::UnsupportedHinting => {
                f.write_str("native text currently supports unhinted outlines only")
            }
            Self::InvalidSize => f.write_str("text size must be positive and finite"),
            Self::InvalidGlyph => f.write_str("invalid glyph ID or position"),
        }
    }
}

impl std::error::Error for TextError {}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod fixture;
