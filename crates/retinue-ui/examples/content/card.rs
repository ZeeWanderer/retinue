use crate::model::{Model, text_style};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct Card(pub Model);

impl View for Card {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            VStack(8.0) {
                Image(self.0.artwork.get()).resizable().frame(160.0, 72.0)
                    .background { Rectangle(Color::rgb(0x343c43)).corner_radius(4.0) }
                Text(self.0.title.get()).width(160.0)
            }.padding(12.0)
                .background { Rectangle(Color::rgb(0x252b30)).corner_radius(8.0) }
                .environment::<Typography>(text_style())
        }
    }
}
