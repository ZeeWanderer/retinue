use fontdue::Font;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

pub fn text_width(font: &Font, text: &str, size: f32) -> f32 {
    text.split('\n')
        .map(|line| {
            let mut width = 0.0;
            let mut previous = None;
            for character in line.chars().filter(|c| !c.is_control()) {
                if let Some(previous) = previous {
                    width += font
                        .horizontal_kern(previous, character, size)
                        .unwrap_or(0.0);
                }
                width += font.metrics(character, size).advance_width;
                previous = Some(character);
            }
            width
        })
        .fold(0.0, f32::max)
}

pub fn wrap_text<'a>(font: &Font, text: &'a str, size: f32, width: f32) -> Vec<&'a str> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push("");
            continue;
        }
        let mut start = 0;
        let mut last_break = 0;
        for (end, kind) in linebreaks(paragraph) {
            if text_width(font, paragraph[start..end].trim_end(), size) > width {
                if last_break > start {
                    lines.push(paragraph[start..last_break].trim_end());
                    start = last_break;
                }
                let content_end = last_break + paragraph[last_break..end].trim_end().len();
                for (offset, grapheme) in paragraph[last_break..content_end].grapheme_indices(true)
                {
                    let next = last_break + offset;
                    if next > start
                        && text_width(font, &paragraph[start..next + grapheme.len()], size) > width
                    {
                        lines.push(paragraph[start..next].trim_end());
                        start = next;
                    }
                }
            }
            last_break = end;
            if kind == BreakOpportunity::Mandatory {
                lines.push(paragraph[start..end].trim_end());
                start = end;
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::painter::{GlyphCache, load_overlay_font};

    #[test]
    fn fractional_advances_do_not_accumulate_pixel_rounding() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        for text in ["iiiiiiiiiiiiiiiiii", "AVATAR", "Recommended relics"] {
            let width = text_width(&font, text, 19.0);
            for scale in [1.0, 1.05, 1.2, 1.25, 1.5] {
                assert!((text_width(&font, text, 19.0 * scale) - width * scale).abs() < 0.001);
            }
        }
    }

    #[test]
    fn wrapping_preserves_words_and_explicit_blank_lines() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        let width = text_width(&font, "hello world", 19.0);
        assert_eq!(
            wrap_text(&font, "hello world again\n\nnext", 19.0, width),
            ["hello world", "again", "", "next"]
        );
    }

    #[test]
    fn long_words_wrap_at_graphemes_and_always_make_progress() {
        let font = load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap();
        let text = "a\u{301}a\u{301}a\u{301}";
        assert_eq!(
            wrap_text(&font, text, 19.0, 0.0),
            ["a\u{301}", "a\u{301}", "a\u{301}"]
        );
        let width = text_width(&font, "abcd", 19.0);
        let lines = wrap_text(&font, "abcdefghijk", 19.0, width);
        assert_eq!(lines.concat(), "abcdefghijk");
        assert!(
            lines
                .iter()
                .all(|line| text_width(&font, line, 19.0) <= width)
        );
        let lines = wrap_text(&font, "abcdefghijk next", 19.0, width);
        assert!(
            lines
                .iter()
                .all(|line| !line.is_empty() && !line.starts_with(' '))
        );
    }
}
