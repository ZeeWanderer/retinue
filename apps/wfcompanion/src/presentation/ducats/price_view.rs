use super::*;
use retinue_ui::drawing::text::{LabelAlignment, label_commands};

#[derive(PartialEq)]
pub(super) struct PriceView {
    pub text: State<Option<String>>,
    pub style: TextStyle,
}

#[view]
impl View for PriceView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        Canvas(PricePaint {
            text: self.text.get().unwrap_or_default(),
            style: self.style.clone(),
        })
    }
}

#[derive(PartialEq)]
struct PricePaint {
    text: String,
    style: TextStyle,
}

impl CanvasContent for PricePaint {
    fn draw(&self, cx: &mut DrawingContext) {
        for command in label_commands(
            &self.style.font,
            cx.bounds,
            self.style.size,
            &self.text,
            self.style.color,
            LabelAlignment::Center,
        )
        .expect("valid price label")
        {
            cx.emit(command);
        }
    }
}
