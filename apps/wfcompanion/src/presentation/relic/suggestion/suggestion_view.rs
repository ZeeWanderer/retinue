use super::suggestion_geometry::Slot;
use retinue_render::Key;
use retinue_ui::layout::anchors::AnchorPoint;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
pub(super) struct SuggestionView {
    pub width: u32,
    pub height: u32,
    pub count: usize,
}

impl View for SuggestionView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let margin = 20.min(self.width / 2).min(self.height / 2);
        let width = 480.min(self.width.saturating_sub(margin));
        let height = 220.min(self.height.saturating_sub(margin));
        let inner_width = width.saturating_sub(4);
        let inner_height = height.saturating_sub(4);
        let header = 44.min(inner_height);
        let footer = 31.min(inner_height.saturating_sub(header));
        let grid_height = inner_height.saturating_sub(header + footer);
        let padding = 5.min(inner_width / 2).min(grid_height / 2);
        let cell_width = inner_width.saturating_sub(padding * 2 + 5) / 2;
        let cell_height = grid_height.saturating_sub(padding * 2 + 5) / 2;
        ui! {
            VStack(0.0) {
                ZStack() {
                    empty().frame(28.0, 28.0)
                        .offset(inner_width.saturating_sub(35) as f32, 8.0).id(Key::from(Slot::Close))
                }.alignment(Alignment::Start)
                    .frame_aligned(inner_width as f32, header as f32, AnchorPoint::TOP_LEADING)
                    .id(Key::from(Slot::Header))
                VStack(5.0) {
                    for row in 0..self.count.div_ceil(2) {
                        HStack(5.0) {
                            for col in 0..2 {
                                if row * 2 + col < self.count {
                                    empty().frame(cell_width as f32, cell_height as f32)
                                        .id(Key::from(Slot::Card(row * 2 + col)))
                                }
                            }
                        }
                    }
                }.alignment(Alignment::Start).padding(padding as f32)
                    .frame_aligned(inner_width as f32, grid_height as f32, AnchorPoint::TOP_LEADING)
                    .clipped().id(Key::from(Slot::Grid))
                empty().frame(inner_width as f32, footer as f32).id(Key::from(Slot::Footer))
            }.alignment(Alignment::Start).padding(2.0)
                .frame_aligned(width as f32, height as f32, AnchorPoint::TOP_LEADING)
                .offset(self.width.saturating_sub(width + margin) as f32, margin as f32)
                .id(Key::from(Slot::Shell))
        }
    }
}
