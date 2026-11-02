use super::card::PartLayout;
use retinue_ui::{
    Key, layout::anchors::AnchorPoint, layout::anchors::AnchorTarget, layout::anchors::Anchored,
    layout::anchors::Attachment, prelude::*,
};

#[derive(PartialEq)]
pub struct ComponentView {
    pub id: u64,
    pub part: PartLayout,
    pub scale: f32,
    pub relic: bool,
}

impl View for ComponentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let image = Key::new("foundry.component", self.id);
        let count = Key::new("foundry.component.count", self.id);
        let relic = Key::new("foundry.component.relic", self.id);
        let layout = Anchored::new(
            AnchorPoint::TOP_LEADING,
            vec![
                Attachment {
                    child: count,
                    target: AnchorTarget::Child(image),
                    target_point: AnchorPoint::BOTTOM_TRAILING,
                    child_point: AnchorPoint::BOTTOM_TRAILING,
                    offset: (3.0, 2.0),
                },
                Attachment {
                    child: relic,
                    target: AnchorTarget::Child(image),
                    target_point: AnchorPoint::BOTTOM_LEADING,
                    child_point: AnchorPoint::BOTTOM_LEADING,
                    offset: (-2.0, 3.0),
                },
            ],
        )
        .unwrap();
        let size = |rect: super::Rect| {
            (
                rect.width as f32 / self.scale,
                rect.height as f32 / self.scale,
            )
        };
        let (iw, ih) = size(self.part.image);
        let (cw, ch) = size(self.part.count);
        let (rw, rh) = size(self.part.relic);
        ui! {
            LayoutView(layout) {
                empty().frame(iw, ih).interactive().id(image)
                empty().frame(cw, ch).id(count)
                empty().frame(rw, rh).interactive().hit_testing(self.relic).id(relic)
            }
        }
    }
}
