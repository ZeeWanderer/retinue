use crate::model::{Accent, Model};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct Meter;

#[derive(ViewProperties)]
struct Fields {
    #[environment]
    model: Model,
    #[environment(Accent)]
    color: Color,
}

impl View for Meter {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let Fields { model, color } = context.properties();
        let filled = model.filled.get();

        ui! {
            HStack(8.0) {
                for index in 0..5 {
                    Rectangle(if index < filled { color } else { Color::rgb(0x35383b) })
                        .corner_radius(4.0).frame(40.0, 20.0).id(index as u64)
                }
            }
        }
    }
}
