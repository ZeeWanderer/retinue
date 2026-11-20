use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct ContentView {
    pub metrics: State<Metrics>,
    pub state: State<PageState>,
    pub enabled: State<bool>,
    pub pieces: Items<(Rc<Piece>, bool)>,
    pub offset: State<f32>,
    pub height: State<f32>,
}

#[view]
impl View for ContentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
          if self.enabled.get() {
            let m = self.metrics.get();
            let viewport = m.viewport(&self.state.get().tab);
            ZStack() {
                place(ScrollViewport::new(ui! {
                    ZStack() {
                        ForEach(self.pieces.order().keys().iter().copied(), |id| *id, |id| {
                            let (piece, _) = self.pieces.get(id).unwrap();
                            let interactive = matches!(&piece.drawing, Drawing::Slot(_) | Drawing::Text { link: Some(_), .. });
                            place(slot(id), piece.frame(&m)).interactive().hit_testing(interactive)
                        })
                    }.alignment(AnchorPoint::TOP_LEADING).frame(frame(viewport.width, self.height.get() + 20.0))
                }).position(0.0, self.offset.get()), viewport).named(BODY).reuse(CacheHint::Conditional)
                control(SCROLL, viewport)
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, m.height))
          }
        }
    }
}
