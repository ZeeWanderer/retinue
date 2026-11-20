use retinue_render::painter::CachedFont;
use retinue_ui::drawing::text::text_width;

pub(in crate::presentation) fn fit_text(
    font: &CachedFont,
    mut text: String,
    size: f32,
    width: f32,
) -> String {
    if text_width(font, &text, size) > width + 0.5 {
        let ends = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        let index = ends.partition_point(|&end| {
            text_width(font, &format!("{}...", &text[..end]), size) <= width
        });
        text.truncate(ends[index.saturating_sub(1)]);
        text.push_str("...");
    }
    text
}
