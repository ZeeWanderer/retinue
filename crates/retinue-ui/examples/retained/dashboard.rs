use crate::{
    meter::Meter,
    model::{Accent, Model},
};
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub struct Dashboard {
    pub initial: Model,
}

impl View for Dashboard {
    fn body(&self, context: &mut ViewContext<'_>) -> impl IntoElement {
        let model = context.owned("model", || self.initial.clone());
        ui! {
            ZStack() {
                Rectangle(Color::rgb(0x23272b)).corner_radius(8.0)
                VStack(16.0) {
                    Meter
                    Meter.environment::<Accent>(Color::rgb(0xd3b560))
                    if model.details.get() {
                        HStack(8.0) {
                            Rectangle(Color::rgb(0xd3b560)).corner_radius(4.0).frame(104.0, 12.0)
                            Rectangle(Color::rgb(0x7f9da9)).corner_radius(4.0).frame(128.0, 12.0)
                        }
                    } else { empty() }
                }
            }.frame(300.0, 140.0).environment_model(model)
        }
    }
}
