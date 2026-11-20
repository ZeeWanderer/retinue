use super::{CHROME, OVERLAYS, paint_slot::PaintSlot};
use retinue_ui::{authoring::*, layout::anchors::AnchorPoint};

pub(super) struct Empty;
#[view]
impl View for Empty {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ()
    }
}

pub(super) struct RootView<V>(pub V);
#[view]
impl<V: View> View for RootView<V> {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl Body {
        ui! {
            ZStack() {
                self.0.body(cx)
                compose(PaintSlot(CHROME))
                compose(PaintSlot(OVERLAYS))
            }.alignment(AnchorPoint::TOP_LEADING)
        }
    }
}
