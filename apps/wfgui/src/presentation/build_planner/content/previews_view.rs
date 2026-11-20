use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct PreviewsView {
    pub metrics: State<Metrics>,
    pub frames: Items<topology::PreviewFrame>,
}

#[view]
impl View for PreviewsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let m = self.metrics.get();
        ui! {
            ZStack() {
                ForEach(self.frames.order().keys().iter().copied(), |id| *id, |id| {
                    let frame = self.frames.get(id).unwrap();
                    place(slot(id), paint_frame(&m, frame.bounds, 8.0)).interactive().hit_testing(frame.active)
                })
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, m.height))
        }
    }
}
