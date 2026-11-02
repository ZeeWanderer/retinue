use super::*;

#[derive(PartialEq)]
pub(super) struct ContentView {
    pub metrics: State<Metrics>,
    pub state: State<PageState>,
    pub enabled: State<bool>,
    pub pieces: Items<(Rc<Piece>, bool)>,
    pub offset: State<f32>,
    pub height: State<f32>,
}

impl View for ContentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        if !self.enabled.get() {
            return empty();
        }
        let m = self.metrics.get();
        let viewport = m.viewport(&self.state.get().tab);
        ui! {
            ZStack() {
                place(ScrollViewport::new(ui! {
                    ZStack() {
                        for &id in self.pieces.order().keys() {
                            let (piece, _) = self.pieces.get(id).unwrap();
                            let interactive = matches!(&piece.drawing, Drawing::Slot(_) | Drawing::Text { link: Some(_), .. });
                            place(slot(id), piece.frame(&m)).interactive().hit_testing(interactive)
                        }
                    }.alignment(Alignment::Start).frame_aligned(viewport.width, self.height.get() + 20.0, AnchorPoint::TOP_LEADING)
                }).position(0.0, self.offset.get()), viewport).id(BODY).reuse(CacheHint::Conditional)
                control(SCROLL, viewport)
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
