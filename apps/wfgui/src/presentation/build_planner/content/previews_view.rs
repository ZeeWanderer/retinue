use super::*;

#[derive(PartialEq)]
pub(super) struct PreviewsView {
    pub metrics: State<Metrics>,
    pub frames: Items<topology::PreviewFrame>,
}

impl View for PreviewsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        ui! {
            ZStack() {
                for &id in self.frames.order().keys() {
                    let frame = self.frames.get(id).unwrap();
                    place(slot(id), paint_frame(&m, frame.bounds, 8.0)).interactive().hit_testing(frame.active)
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
