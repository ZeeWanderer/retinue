use crate::model::{Model, text_style};
use retinue_render::Color;
use retinue_ui::{authoring::*, prelude::Typography};

#[derive(PartialEq)]
pub struct Card {
    pub model: Model,
}

#[view]
impl View for Card {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            VStack(8.0) {
                Image(self.model.artwork.get()).resizable().frame(Frame::new().width(160.0).height(72.0))
                    .background(Rectangle(Color::rgb(0x343c43)).corner_radius(4.0))
                Text(self.model.title.get()).frame(Frame::new().width(160.0))
            }.padding(12.0)
                .background(Rectangle(Color::rgb(0x252b30)).corner_radius(8.0))
                .environment::<Typography>(text_style())
                .padding(20.0)
        }
    }
}
