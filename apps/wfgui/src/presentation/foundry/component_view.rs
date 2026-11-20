use super::card::PartLayout;
use crate::presentation::widgets::compiled::frame;
use retinue_ui::layout::anchors::{AnchorTarget, Anchored, Attachment};
use retinue_ui::{
    Key,
    authoring::*,
    layout::{LayoutSize, anchors::AnchorPoint},
};

#[derive(Clone, PartialEq)]
pub struct ComponentView {
    id: u64,
    image: LayoutSize,
    count: LayoutSize,
    indicator: LayoutSize,
    relic: bool,
}

impl ComponentView {
    pub(super) fn cluster_key(id: u64) -> Key {
        Key::new("foundry.component.cluster", id)
    }

    pub(super) fn key(&self) -> Key {
        Self::cluster_key(self.id)
    }

    pub(super) fn new(id: u64, part: &PartLayout, scale: f32, relic: bool) -> Self {
        let size = |rect: super::Rect| {
            LayoutSize::new(rect.width as f32 / scale, rect.height as f32 / scale)
        };
        Self {
            id,
            image: size(part.image),
            count: size(part.count),
            indicator: size(part.relic),
            relic,
        }
    }
}

#[retinue_ui::authoring::view]
impl View for ComponentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
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
        ui! {
            LayoutView::new(layout) {
                empty().frame(frame(self.image.width, self.image.height)).interactive().named(image)
                empty().frame(frame(self.count.width, self.count.height)).named(count)
                empty().frame(frame(self.indicator.width, self.indicator.height)).interactive().hit_testing(self.relic).named(relic)
            }
        }
    }
}
