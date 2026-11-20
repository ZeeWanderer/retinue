use super::PaintSources;
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(crate) struct PaintSlot(pub retinue_render::Key);

#[view]
impl View for PaintSlot {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl Body {
        self.canvas(cx).named(self.0)
    }
}

impl PaintSlot {
    pub fn canvas(
        &self,
        cx: &mut ViewContext<'_>,
    ) -> retinue_ui::view::Canvas<retinue_ui::drawing::DrawList> {
        let sources = cx
            .environment()
            .model::<PaintSources>()
            .expect("page paint host");
        let content = sources.slot(self.0).get();
        Canvas(content.drawing).cache_policy(content.cache)
    }
}
