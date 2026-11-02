use super::{
    Filter, card, composition::position, filters::declaration, filters::declaration::CONTENT,
    filters::declaration::PANEL, foundry_model::Model,
};
use retinue_ui::{Bounds, Key, prelude::*, semantics::Role};

#[derive(PartialEq)]
pub struct FiltersView {
    pub model: Model,
}

impl View for FiltersView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let Some(g) = self.model.filters.get() else {
            return empty();
        };
        let offset = self.model.filter_offset.get();
        let close = super::Rect {
            x: g.bounds.width.saturating_sub(g.px(36.0)),
            y: g.px(4.0),
            width: g.px(32.0),
            height: g.px(32.0),
        };
        let clear = super::Rect {
            x: g.px(12.0),
            y: g.bounds.height.saturating_sub(g.px(42.0)),
            width: g.bounds.width.saturating_sub(g.px(24.0)),
            height: g.px(30.0),
        };
        let viewport = card::bounds(
            super::Rect {
                x: g.viewport.x - g.bounds.x,
                y: g.viewport.y - g.bounds.y,
                ..g.viewport
            },
            g.scale,
        );
        let choices = ui! {
            VStack(0.0) {
                for filter in Filter::ALL {
                    VStack(4.0) {
                        empty().frame(g.width(), 19.0)
                        HStack(10.0) {
                            for value in [true, false] {
                                empty().frame(g.choice_width(), 32.0).interactive().id(declaration::choice_key(filter, value))
                            }
                        }
                    }.alignment(Alignment::Start).padding(Insets { bottom: 9.0, ..Insets::default() })
                }
            }.frame(g.width(), 64.0 * Filter::ALL.len() as f32).id(CONTENT)
        };
        position(ui! {
            ZStack() {
                empty().frame(g.bounds.width as f32 / g.scale, g.bounds.height as f32 / g.scale).interactive()
                position(empty(), card::bounds(close, g.scale)).interactive().id(Key::new("foundry.filter.close", 0))
                position(ScrollViewport::new(choices).position(-offset.0, -offset.1), viewport)
                position(empty(), card::bounds(clear, g.scale)).interactive().id(Key::new("foundry.filter.clear", 0))
                if viewport.height < 64.0 * Filter::ALL.len() as f32 {
                    position(empty(), Bounds::new(viewport.x + viewport.width, viewport.y, 16.0, viewport.height))
                        .interactive().id(Key::new("foundry.filter.scroll", 0))
                }
            }.alignment(Alignment::Start)
        }, card::bounds(g.bounds, g.scale)).id(PANEL).role(Role::Overlay)
    }
}
