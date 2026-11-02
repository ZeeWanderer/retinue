use super::*;
use retinue_render::{Command, TextAlignment};

#[derive(PartialEq)]
pub(super) struct PriceView {
    pub text: State<Option<String>>,
    pub style: TextStyle,
}

impl retinue_ui::view::Primitive for PriceView {
    fn resolve(&self, _: &mut ViewContext<'_>) -> retinue_ui::view::PrimitiveOutput {
        retinue_ui::view::PrimitiveOutput::new(
            PricePaint {
                text: self.text.get().unwrap_or_default(),
                style: self.style.clone(),
            },
            self.style.clone(),
        )
        .fill_available()
    }
}

impl IntoElement for PriceView {
    fn into_element(self) -> Element {
        Element::primitive(self)
    }
}

#[derive(PartialEq)]
struct PricePaint {
    text: String,
    style: TextStyle,
}

impl CanvasContent for PricePaint {
    fn draw(&self, cx: &mut DrawingContext) {
        cx.emit(Command::Text {
            bounds: cx.bounds,
            font: self.style.font.clone(),
            text: self.text.clone(),
            size: self.style.size,
            color: self.style.color,
            align: TextAlignment::Center,
        });
    }
}
