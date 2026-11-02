use super::*;

#[derive(PartialEq)]
pub(super) struct RailView {
    pub area: State<(f32, f32)>,
    pub market: State<bool>,
    pub ready: State<bool>,
    pub error: State<String>,
    pub events: Element,
    pub market_content: Element,
    pub font: TextStyle,
}

impl View for RailView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (w, h) = self.area.get();
        if w <= 0.0 || h <= 0.0 {
            return empty();
        }
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), Bounds::new(0.0, 10.0, w, (h - 10.0).max(0.0)))
                for i in 0..2 {
                    place(slot(Key::new("rail.tab", i)), Bounds::new(i as f32 * w / 2.0, 0.0, w / 2.0, 36.0)).interactive()
                }
                if self.market.get() {
                    self.market_content.clone()
                } else if self.ready.get() {
                    self.events.clone()
                    place(slot(NOTIFICATION), Bounds::new(w - 164.0, 190.0, 60.0, 27.0)).interactive()
                    for i in 0..3 {
                        place(slot(Key::new("rail.filter", i)), Bounds::new(w - 95.0 + i as f32 * 29.0, 190.0, 26.0, 27.0)).interactive()
                    }
                    place(slot(SCROLL), Bounds::new(7.0, 224.0, (w - 14.0).max(0.0), (h - 224.0).max(0.0))).interactive()
                } else {
                    ZStack() {
                        if self.error.get().is_empty() {
                            loading_view::LoadingView { width: w, height: h, font: self.font.clone() }
                        } else {
                            place(label(self.error.get(), self.font.font.clone(), 14.0, BAD, false), Bounds::new(20.0, 65.0, (w - 40.0).max(0.0), 70.0))
                            place(slot(RETRY), Bounds::new(20.0, 145.0, (w - 40.0).max(0.0), 32.0)).interactive()
                        }
                    }.alignment(Alignment::Start).frame_aligned(w, h, AnchorPoint::TOP_LEADING).id(LOADING)
                }
            }.alignment(Alignment::Start).frame_aligned(w, h, AnchorPoint::TOP_LEADING)
        }
    }
}
