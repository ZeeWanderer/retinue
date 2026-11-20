use super::{card, card::Content, component_view::ComponentView, components_view::ComponentsView};
use crate::presentation::widgets::compiled::frame;
use retinue_render::{CachePolicy, Key};
use retinue_ui::{authoring::*, layout::LayoutSize, state::State};

#[derive(PartialEq)]
pub(super) struct CardView {
    pub key: Key,
    pub content: State<Content>,
}

impl ItemView for CardView {
    type Item = Content;
    fn from_item(key: Key, content: State<Content>) -> Self {
        Self { key, content }
    }
}

#[retinue_ui::authoring::view]
impl View for CardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let c = self.content.read();
        let id = self.key.value;
        let parts = c.layout.parts.iter().take(c.relics.len()).enumerate();
        let components = parts
            .clone()
            .map(|(i, part)| ComponentView::new(id * 8 + i as u64, part, c.scale, c.relics[i]))
            .collect();
        let frames = parts.map(|(i, part)| {
            (
                ComponentView::cluster_key(id * 8 + i as u64),
                card::bounds(part.image, c.scale),
            )
        });
        ui! {
            PositionedStack(LayoutSize::new(c.frame.width, c.frame.height)) {
                Canvas(card::Background { owned: c.owned, hovered: c.hovered }).cache_policy(CachePolicy::Prefer)
                Canvas(c.artwork.clone()).cache_policy(CachePolicy::Prefer).named(Key::new("foundry.card.content", id))
                ForEach([
                    ("foundry.favorite", c.layout.title.0, true),
                    ("foundry.mastered", c.layout.title.2, false),
                    ("foundry.hero", c.layout.hero, false),
                    ("foundry.subsumed", c.layout.subsumed, false),
                ], |(namespace, _, _)| Key::new(namespace, id)) { |(namespace, rect, hit)|
                    empty().frame(frame(rect.width as f32 / c.scale, rect.height as f32 / c.scale))
                        .placed(card::bounds(rect, c.scale)).interactive().hit_testing(hit)
                        .named(Key::new(namespace, id))
                }
                Canvas::new(c.title.clone()).placed(card::bounds(c.layout.title.1, c.scale))
                    .named(Key::new("foundry.title", id))
                compose(ComponentsView(components))
            }.child_frames(frames).clipped().named(self.key)
        }
    }
}
