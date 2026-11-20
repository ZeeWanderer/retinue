use retinue_render::Key;
use retinue_ui::{authoring::*, drawing::TextStyle};

#[derive(Clone, PartialEq)]
pub(super) struct LabelView {
    pub id: Key,
    pub text: String,
    pub style: TextStyle,
}

#[view]
impl View for LabelView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Text(self.text.clone())
            .font(self.style.font.clone())
            .font_size(self.style.size)
            .color(self.style.color)
            .named(self.id)
    }
}
