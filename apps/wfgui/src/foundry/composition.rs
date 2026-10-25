use super::*;
use retinue_ui::composition::{
    Bounds, CachePolicy, Canvas, Change, Color, Command, Element, Key as ElementKey, Paint,
    PaintKey, RasterMode, RecordingMode, Renderer, Scene, TextAlignment,
};

const ROOT: ElementKey = ElementKey::new("foundry", 0);
const TOOLBAR: ElementKey = ElementKey::new("foundry", 1);
const GRID: ElementKey = ElementKey::new("foundry", 2);
const OVERLAYS: ElementKey = ElementKey::new("foundry", 3);
const CONTENT: ElementKey = ElementKey::new("foundry", 4);
pub(super) const SEARCH: ElementKey = ElementKey::new("foundry", 5);

pub(super) struct CardLayout {
    pub title: (Rect, Rect, Rect),
    pub hero: Rect,
    pub subsumed: Rect,
    pub parts: Vec<PartLayout>,
}

pub(super) struct PartLayout {
    pub image: Rect,
    pub count: Rect,
    pub relic: Rect,
}

#[derive(Clone, PartialEq)]
struct State {
    size: (u32, u32),
    scale: f32,
    viewport: Rect,
    grid: Grid,
    offset: u32,
    revision: u64,
    images: u64,
    categories: [f32; 8],
    search_width: f32,
    search_empty: bool,
    hovered: Option<usize>,
}

pub(super) struct Composition {
    pub scene: Scene,
    renderer: RefCell<Renderer>,
    state: Option<State>,
    identities: HashMap<String, u64>,
    next_identity: u64,
    cards: HashMap<ElementKey, (usize, CardLayout)>,
    actions: HashMap<ElementKey, Action>,
    artwork: HashMap<ElementKey, (Vec<Command>, Paint)>,
    paint_revision: u64,
}

impl Default for Composition {
    fn default() -> Self {
        Self {
            scene: Scene::default(),
            renderer: RefCell::new(Renderer::new(32 * 1024 * 1024)),
            state: None,
            identities: HashMap::new(),
            next_identity: 0,
            cards: HashMap::new(),
            actions: HashMap::new(),
            artwork: HashMap::new(),
            paint_revision: 0,
        }
    }
}

impl Composition {
    pub fn cache_stats(&self) -> retinue_ui::resources::CacheStats {
        self.renderer.borrow().cache_stats()
    }

    #[cfg(test)]
    pub fn paint_stats(&self) -> retinue_ui::composition::PaintStats {
        self.renderer.borrow().stats
    }

    pub fn clear_resources(&mut self) {
        self.state = None;
        self.artwork.clear();
        self.cards.clear();
        self.scene = Scene::default();
        self.renderer.get_mut().clear_cache();
    }

    fn state(view: &Foundry) -> State {
        State {
            size: view.size,
            scale: view.scale,
            viewport: view.viewport,
            grid: view.grid,
            offset: view.offset,
            revision: view.layout_revision,
            images: view.image_revision,
            categories: view.category_motion.weights,
            search_width: view.search_motion.width,
            search_empty: view.search.borrow().text().is_empty(),
            hovered: view.hovered_card(),
        }
    }

    pub fn update(&mut self, view: &Foundry) {
        let state = Self::state(view);
        if self.state.as_ref() == Some(&state) || view.scale <= 0.0 {
            return;
        }
        let mut content_only = false;
        if let Some(old) = &self.state {
            let mut layout = state.clone();
            layout.offset = old.offset;
            layout.hovered = old.hovered;
            layout.images = old.images;
            content_only = &layout == old;
            if &layout == old
                && view
                    .grid
                    .visible(view.count(), old.offset, view.viewport.height, 0)
                    == view.visible(0)
            {
                let mut changes = Vec::new();
                if old.offset != state.offset {
                    changes.push(Change::Offset(
                        CONTENT,
                        (0.0, -(view.offset as f32) / view.scale),
                    ));
                }
                if old.hovered != state.hovered {
                    for index in [old.hovered, state.hovered].into_iter().flatten() {
                        let card = &view.snapshot.items[view.filtered[index]];
                        if let Some(&id) = self.identities.get(&card.id) {
                            let key = ElementKey::new("foundry.card", id);
                            changes.push(Change::Paint(
                                key,
                                Some(background(key, card.owned, state.hovered == Some(index))),
                            ));
                        }
                    }
                }
                self.scene
                    .patch(changes)
                    .expect("retained visible Foundry nodes");
                if old.images != state.images {
                    self.sync_artwork(view);
                }
                self.renderer
                    .get_mut()
                    .prepare(&self.scene, RasterMode::Explicit);
                self.state = Some(state);
                return;
            }
        }
        self.cards.clear();
        let mut cards = Vec::new();
        self.actions
            .retain(|key, _| content_only && key.namespace == "foundry.control");
        if self
            .state
            .as_ref()
            .is_none_or(|old| old.revision != state.revision)
        {
            let ids: HashSet<_> = view
                .snapshot
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect();
            self.identities.retain(|id, _| ids.contains(id.as_str()));
        }
        let element = |key, rect: Rect| {
            Element::at(
                key,
                Bounds::new(
                    rect.x as f32 / view.scale,
                    rect.y as f32 / view.scale,
                    rect.width as f32 / view.scale,
                    rect.height as f32 / view.scale,
                ),
            )
        };
        let area = Rect {
            width: view.size.0,
            height: view.size.1,
            ..Rect::default()
        };
        let mut root = if content_only {
            None
        } else {
            let mut root = element(ROOT, area);
            root.style.position = retinue_ui::taffy::Position::Relative;
            let mut toolbar = element(
                TOOLBAR,
                Rect {
                    height: view.viewport.y,
                    ..area
                },
            );
            toolbar.paint = Some(Paint::external(TOOLBAR));
            for (index, (action, rect)) in view.layout_controls().into_iter().enumerate() {
                let key = ElementKey::new("foundry.control", index as u64);
                let mut child = element(key, rect);
                child.interactive = true;
                self.actions.insert(key, action);
                toolbar.children.push(child);
            }
            root.children.push(toolbar);
            Some(root)
        };
        let mut content = element(
            CONTENT,
            Rect {
                width: view.viewport.width,
                height: view.grid.content_height(view.count()),
                ..Rect::default()
            },
        );
        content.offset = (0.0, -(view.offset as f32) / view.scale);
        for index in view.visible(0) {
            let row = view.filtered[index];
            let card = &view.snapshot.items[row];
            let id = *self.identities.entry(card.id.clone()).or_insert_with(|| {
                self.next_identity += 1;
                self.next_identity
            });
            let key = ElementKey::new("foundry.card", id);
            let (x, y) = view.grid.origin(index);
            let mut item = Element::at(
                key,
                Bounds::new(
                    x as f32 / view.scale,
                    y as f32 / view.scale,
                    view.grid.width as f32 / view.scale,
                    view.grid.height as f32 / view.scale,
                ),
            );
            item.clip = true;
            item.paint = Some(background(key, card.owned, state.hovered == Some(index)));
            let content_key = ElementKey::new("foundry.card.content", id);
            let mut artwork = element(
                content_key,
                Rect {
                    width: view.grid.width,
                    height: view.grid.height,
                    ..Rect::default()
                },
            );
            artwork.paint = Some(
                self.artwork
                    .get(&content_key)
                    .map(|(_, paint)| paint.clone())
                    .unwrap_or_else(|| Paint::new(PaintKey::new(content_key, 0), |_| Vec::new())),
            );
            item.children.push(artwork);
            cards.push((content_key, row));
            let (star, title, mastered) = view.title_rects(card);
            for (scope, rect, action) in [
                ("foundry.favorite", star, Some(Action::Favorite(row))),
                ("foundry.title", title, None),
                ("foundry.mastered", mastered, None),
            ] {
                let key = ElementKey::new(scope, id);
                let mut node = element(key, rect);
                if scope == "foundry.title" {
                    let text = title_text(view, card, rect.width);
                    let font = view.regular.clone();
                    node.paint = Some(Paint::new(
                        PaintKey::new(key, view.layout_revision),
                        move |size| {
                            vec![Command::Text {
                                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                                size: 20.0,
                                text: text.clone(),
                                font: font.clone(),
                                color: Color::from_bgra(TEXT),
                                align: TextAlignment::Center,
                            }]
                        },
                    ));
                }
                if let Some(action) = action {
                    node.interactive = true;
                    self.actions.insert(key, action);
                }
                item.children.push(node);
            }
            let warframe = card.group == "warframe";
            let diameter = view.px(if warframe { 105.0 } else { 86.0 });
            let hero = Rect {
                x: view.px(10.0) + view.px(3.0),
                y: if warframe {
                    view.grid.height - diameter
                } else {
                    view.px(43.0)
                },
                width: diameter,
                height: diameter,
            };
            item.children
                .push(element(ElementKey::new("foundry.hero", id), hero));
            item.children.push(element(
                ElementKey::new("foundry.subsumed", id),
                Rect {
                    x: hero.x + hero.width - view.px(27.0),
                    y: view.grid.height - view.px(29.0),
                    width: view.px(25.0),
                    height: view.px(25.0),
                },
            ));
            for (part, rect) in view.parts(card.components.len()).into_iter().enumerate() {
                let key = ElementKey::new("foundry.component", id * 8 + part as u64);
                let mut node = element(key, rect);
                node.interactive = true;
                self.actions.insert(key, Action::Component(row, part));
                item.children.push(node);
                let component = &card.components[part];
                let count =
                    view.count_badge(rect, &component.owned.to_string(), component.owned_relic);
                item.children.push(element(
                    ElementKey::new("foundry.component.count", id * 8 + part as u64),
                    count,
                ));
                {
                    let key = ElementKey::new("foundry.component.relic", id * 8 + part as u64);
                    let mut badge = element(key, view.relic_badge(rect));
                    badge.interactive = component.owned_relic;
                    if component.owned_relic {
                        self.actions.insert(key, Action::ComponentRelic(row, part));
                    }
                    item.children.push(badge);
                }
            }
            content.children.push(item);
        }
        if let Some(mut root) = root.take() {
            let mut grid = element(GRID, view.viewport);
            grid.clip = true;
            grid.children.push(content);
            root.children.push(grid);
            let mut overlays = element(OVERLAYS, area);
            overlays.paint = Some(Paint::external(OVERLAYS));
            root.children.push(overlays);
            self.scene
                .update(root, view.size, view.scale, RecordingMode::Retained)
                .expect("unique Foundry composition identities");
        } else {
            self.scene
                .update_subtree(content)
                .expect("Foundry content scope");
        }
        self.cards = cards
            .into_iter()
            .map(|(key, row)| {
                (
                    key,
                    (row, self.card_layout(key, view, &view.snapshot.items[row])),
                )
            })
            .collect();
        self.artwork.retain(|key, _| self.cards.contains_key(key));
        self.sync_artwork(view);
        self.renderer
            .get_mut()
            .prepare(&self.scene, RasterMode::Explicit);
        self.state = Some(state);
    }

    pub fn hit(&self, point: (f64, f64)) -> Option<Action> {
        self.scene
            .hit_test(point)
            .and_then(|key| self.actions.get(&key).copied())
    }

    fn sync_artwork(&mut self, view: &Foundry) {
        let mut changes = Vec::new();
        for (&key, (row, layout)) in &self.cards {
            let mut canvas = Canvas::new(view.scale);
            view.record_card_content(&mut canvas, &view.snapshot.items[*row], layout);
            if self
                .artwork
                .get(&key)
                .is_some_and(|(commands, _)| *commands == canvas.commands)
            {
                continue;
            }
            self.paint_revision += 1;
            let commands = canvas.commands.clone();
            let mut paint = Paint::new(PaintKey::new(key, self.paint_revision), move |_| {
                commands.clone()
            });
            paint.cache = CachePolicy::Prefer;
            changes.push(Change::Paint(key, Some(paint.clone())));
            self.artwork.insert(key, (canvas.commands, paint));
        }
        if !changes.is_empty() {
            let prior = self.scene.stats;
            self.scene.patch(changes).expect("Foundry artwork nodes");
            self.scene.stats.nodes += prior.nodes;
            self.scene.stats.layout_computations += prior.layout_computations;
            self.scene.stats.recordings += prior.recordings;
        }
    }

    pub fn controls(&self, items: bool) -> Vec<(Action, Rect)> {
        self.scene
            .geometry()
            .iter()
            .filter_map(|node| {
                let action = self.actions.get(&node.key)?;
                let item = matches!(
                    action,
                    Action::Favorite(_) | Action::Component(_, _) | Action::ComponentRelic(_, _)
                );
                (item == items && !node.hit.is_empty()).then_some((*action, node.hit))
            })
            .collect()
    }

    pub fn record_chrome(&mut self, toolbar: Vec<Command>, overlays: Vec<Command>) {
        self.scene
            .record_many([
                (TOOLBAR, toolbar, CachePolicy::Never),
                (OVERLAYS, overlays, CachePolicy::Never),
            ])
            .expect("Foundry chrome");
        self.renderer
            .get_mut()
            .prepare(&self.scene, RasterMode::Explicit);
    }

    pub fn paint(&self, view: &Foundry, p: &mut Painter<'_>) {
        self.renderer.borrow_mut().draw_with(
            &self.scene,
            p,
            Rect {
                width: view.size.0,
                height: view.size.1,
                ..Rect::default()
            },
            |key, p| match key {
                SEARCH => {
                    let r = view.search_bounds();
                    p.translated((-(r.x as i32), -(r.y as i32)), |p| view.paint_search(p, r));
                }
                _ => unreachable!("only the search editor uses live paint"),
            },
        );
    }

    fn card_layout(&self, card: ElementKey, view: &Foundry, data: &Card) -> CardLayout {
        let scale = view.scale;
        let origin = self
            .scene
            .bounds(ElementKey::new("foundry.card", card.value))
            .unwrap()
            .layout;
        let rect = |scope, id| {
            let node = self
                .scene
                .bounds(ElementKey::new(scope, id))
                .unwrap()
                .layout;
            Rect {
                x: ((node.x - origin.x) * scale).round().max(0.0) as u32,
                y: ((node.y - origin.y) * scale).round().max(0.0) as u32,
                width: (node.width * scale).round() as u32,
                height: (node.height * scale).round() as u32,
            }
        };
        let title = (
            rect("foundry.favorite", card.value),
            rect("foundry.title", card.value),
            rect("foundry.mastered", card.value),
        );
        CardLayout {
            title,
            hero: rect("foundry.hero", card.value),
            subsumed: rect("foundry.subsumed", card.value),
            parts: (0..data.components.len().min(6))
                .map(|part| {
                    let id = card.value * 8 + part as u64;
                    PartLayout {
                        image: rect("foundry.component", id),
                        count: rect("foundry.component.count", id),
                        relic: rect("foundry.component.relic", id),
                    }
                })
                .collect(),
        }
    }
}

fn background(key: ElementKey, owned: bool, hovered: bool) -> Paint {
    let mut paint = Paint::new(
        PaintKey::new(key, u64::from(hovered) + 2 * u64::from(owned)),
        move |size| {
            let bounds = Bounds::new(0.0, 0.0, size.width, size.height);
            let mut commands = vec![Command::Fill {
                bounds,
                radius: 15.0,
                color: Color::from_bgra(if hovered { CARD_HOVER } else { CARD }),
            }];
            if owned {
                commands.push(Command::Fill {
                    bounds,
                    radius: 15.0,
                    color: Color::from_bgra(if hovered {
                        crate::style::OWNED_CARD_HOVER
                    } else {
                        crate::style::OWNED_CARD
                    }),
                });
                commands.push(Command::InsetShadow {
                    bounds,
                    radius: 15.0,
                    blur: 7.0,
                    spread: 3.0,
                    color: Color::from_bgra(if hovered {
                        crate::style::OWNED_EDGE_HOVER
                    } else {
                        crate::style::OWNED_EDGE
                    }),
                });
            }
            commands
        },
    );
    paint.cache = CachePolicy::Prefer;
    paint
}

fn title_text(view: &Foundry, card: &Card, width: u32) -> String {
    let mut name = card.name.clone();
    if text_width(&view.regular, &name, 20.0 * view.scale) > width as f32 {
        while !name.is_empty()
            && text_width(&view.regular, &format!("{name}..."), 20.0 * view.scale) > width as f32
        {
            name.pop();
        }
        name.push_str("...");
    }
    name
}

impl Foundry {
    pub(super) fn resolved_composition(&self) -> std::cell::Ref<'_, Composition> {
        let stale = self.composition.borrow().state.as_ref() != Some(&Composition::state(self));
        if stale {
            self.composition.borrow_mut().update(self);
        }
        self.composition.borrow()
    }

    pub fn controls(&self) -> Vec<(Action, Rect)> {
        if self.scale <= 0.0 {
            return self.layout_controls();
        }
        self.resolved_composition().controls(false)
    }
}
