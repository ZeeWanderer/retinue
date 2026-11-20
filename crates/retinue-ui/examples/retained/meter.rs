use crate::model::{Accent, Model};
use retinue_render::Color;
use retinue_ui::{ViewProperties, authoring::*};

#[derive(PartialEq)]
pub struct Meter;

#[derive(ViewProperties)]
struct Fields {
    #[environment]
    model: Model,
    #[environment(Accent)]
    color: Color,
}

#[view]
impl View for Meter {
    fn body(&self, context: &mut ViewContext<'_>) -> impl Body {
        let Fields { model, color } = context.properties();
        let filled = model.filled.get();

        ui! {
            HStack(8.0) {
                ForEach(0..5u32, |i| u64::from(*i)) { |index|
                    Rectangle(if index < filled { color } else { Color::rgb(0x35383b) })
                        .corner_radius(4.0).frame(Frame::new().width(40.0).height(20.0))
                }
            }
        }
    }
}
