use super::suggestion_geometry::Slot;
use retinue_render::Key;
use retinue_ui::authoring::*;
use retinue_ui::layout::anchors::AnchorPoint;

#[derive(PartialEq)]
pub(super) struct SuggestionView {
    pub width: u32,
    pub height: u32,
    pub count: usize,
}

#[view]
impl View for SuggestionView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
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
                    empty().frame(Frame::new().width(28.0).height(28.0))
                        .offset(inner_width.saturating_sub(35) as f32, 8.0).named(Key::from(Slot::Close))
                }.alignment(AnchorPoint::TOP_LEADING)
                    .frame(Frame::new().width(inner_width as f32).height(header as f32).alignment(AnchorPoint::TOP_LEADING))
                    .named(Key::from(Slot::Header))
                VStack(5.0) {
                    ForEach(0..self.count.div_ceil(2), |row| *row as u64) { |row|
                        HStack(5.0) {
                            ForEach(row * 2..self.count.min(row * 2 + 2), |i| Key::from(Slot::Card(*i))) { |i|
                                empty().frame(Frame::new().width(cell_width as f32).height(cell_height as f32))
                                    .named(Key::from(Slot::Card(i)))
                            }
                        }
                    }
                }.alignment(HorizontalAlignment::Leading).padding(padding as f32)
                    .frame(Frame::new().width(inner_width as f32).height(grid_height as f32).alignment(AnchorPoint::TOP_LEADING))
                    .clipped().named(Key::from(Slot::Grid))
                empty().frame(Frame::new().width(inner_width as f32).height(footer as f32)).named(Key::from(Slot::Footer))
            }.alignment(HorizontalAlignment::Leading).padding(2.0)
                .frame(Frame::new().width(width as f32).height(height as f32).alignment(AnchorPoint::TOP_LEADING))
                .offset(self.width.saturating_sub(width + margin) as f32, margin as f32)
                .named(Key::from(Slot::Shell))
        }
    }
}
