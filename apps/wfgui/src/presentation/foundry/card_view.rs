use super::{card, card::Content, component_view::ComponentView};
use retinue_render::{Bounds, CachePolicy, Key};
use retinue_ui::{layout::positioned::Placement, layout::positioned::Positioned, prelude::*};

#[derive(PartialEq)]
pub struct CardView {
    pub key: Key,
    pub content: State<Content>,
}

impl View for CardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let c = self.content.read();
        let id = self.key.value;
        ui! {
            LayoutView(Positioned::new(LayoutSize::new(c.frame.width, c.frame.height))) {
                Canvas(card::Background { owned: c.owned, hovered: c.hovered }).cache_policy(CachePolicy::Prefer)
                Canvas(c.artwork.clone()).cache_policy(CachePolicy::Prefer).id(Key::new("foundry.card.content", id))
                for (namespace, rect, hit) in [
                    ("foundry.favorite", c.layout.title.0, true),
                    ("foundry.mastered", c.layout.title.2, false),
                    ("foundry.hero", c.layout.hero, false),
                    ("foundry.subsumed", c.layout.subsumed, false),
                ] {
                    place(empty().frame(rect.width as f32 / c.scale, rect.height as f32 / c.scale),
                        card::bounds(rect, c.scale)).interactive().hit_testing(hit)
                        .id(Key::new(namespace, id))
                }
                place(Canvas::new(c.title.clone()), card::bounds(c.layout.title.1, c.scale))
                    .id(Key::new("foundry.title", id))
                for (index, part) in c.layout.parts.iter().take(6).enumerate() {
                    place(ComponentView { id: id * 8 + index as u64, part: part.clone(), scale: c.scale,
                        relic: c.relics[index] }, card::bounds(part.image, c.scale))
                        .id(Key::new("foundry.component.cluster", id * 8 + index as u64))
                }
            }.clipped()
        }
    }
}

fn place(view: impl IntoElement, bounds: Bounds) -> Element {
    view.layout_value::<Placement>(Some(bounds))
}
