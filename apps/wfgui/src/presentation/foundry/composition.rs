use super::{FoundryView, foundry_model::Model, *};
use retinue_render::{Bounds, Command, Key as ElementKey, RasterMode, Renderer};
use retinue_ui::{
    drawing::DrawList, drawing::PixelCanvas as Canvas, layout::Insets, layout::LayoutSize,
    layout::grid::UniformGrid, prelude::Element, prelude::IntoElement, prelude::ViewExt,
    runtime::ViewTree, state::KeyedOrder,
};

const ROOT: ElementKey = ElementKey::new("foundry", 0);
pub(super) const TOOLBAR: ElementKey = ElementKey::new("foundry", 1);
pub(super) const GRID: ElementKey = ElementKey::new("foundry", 2);
pub(super) const OVERLAYS: ElementKey = ElementKey::new("foundry", 3);
pub(super) const CONTENT: ElementKey = ElementKey::new("foundry", 4);
pub(super) const SEARCH: ElementKey = ElementKey::new("foundry", 5);

pub(super) use super::card::CardLayout;

#[derive(Clone, PartialEq)]
struct State {
    layout: LayoutState,
    images: u64,
    hovered: Option<usize>,
}

#[derive(Clone, PartialEq)]
struct LayoutState {
    size: (u32, u32),
    scale: f32,
    viewport: Rect,
    grid: Grid,
    offset: u32,
    visible: std::ops::Range<usize>,
    revision: u64,
    categories: [f32; 8],
    search_width: f32,
    search_empty: bool,
    filters: Option<filters::declaration::Geometry>,
    filter_offset: u32,
    component_bounds: Option<Rect>,
    component_controls: Vec<(Action, Rect)>,
}

impl LayoutState {
    fn new(view: &Foundry) -> Self {
        Self {
            size: view.size,
            scale: view.scale,
            viewport: view.viewport,
            grid: view.grid,
            offset: view.offset,
            visible: view.visible(0),
            revision: view.layout_revision,
            categories: view.category_motion.weights,
            search_width: view.search_motion.width,
            search_empty: view.search.borrow().text().is_empty(),
            filters: filters::declaration::Geometry::new(view),
            filter_offset: view.filter_panel.offset,
            component_bounds: view
                .component_open()
                .then_some(view.component_panel.popup.bounds),
            component_controls: view.component_controls(),
        }
    }
}

pub(super) struct Composition {
    pub tree: ViewTree,
    pub(super) renderer: RefCell<Renderer>,
    #[cfg(test)]
    pub raster_mode: RasterMode,
    inputs: Model,
    state: Option<State>,
    identities: HashMap<String, u64>,
    next_identity: u64,
    actions: HashMap<ElementKey, Action>,
    artwork: HashMap<ElementKey, DrawList>,
    #[cfg(all(test, feature = "profiling"))]
    pub paint_profile: std::cell::Cell<[Duration; 3]>,
    #[cfg(all(test, feature = "profiling"))]
    pub record_profile: [Duration; 2],
    #[cfg(all(test, feature = "profiling"))]
    pub resolve_profile: (
        retinue_ui::runtime::UpdateProfile,
        retinue_ui::scene::UpdateProfile,
    ),
}

impl Default for Composition {
    fn default() -> Self {
        let inputs = Model::default();
        Self {
            tree: ViewTree::new(FoundryView(inputs.clone()).id(ROOT), || {}),
            renderer: RefCell::new(Renderer::new(retinue_render::ResidencyPolicy::desktop(
                32 * 1024 * 1024,
            ))),
            #[cfg(test)]
            raster_mode: RasterMode::Explicit,
            inputs,
            state: None,
            identities: HashMap::new(),
            next_identity: 0,
            actions: HashMap::new(),
            artwork: HashMap::new(),
            #[cfg(all(test, feature = "profiling"))]
            paint_profile: Default::default(),
            #[cfg(all(test, feature = "profiling"))]
            record_profile: Default::default(),
            #[cfg(all(test, feature = "profiling"))]
            resolve_profile: Default::default(),
        }
    }
}

impl Composition {
    pub fn bounds(&self, key: ElementKey) -> Option<retinue_ui::scene::Geometry> {
        self.tree
            .unique(key)
            .expect("unique Foundry key")
            .and_then(|id| self.tree.geometry(id))
            .map(|geometry| retinue_ui::scene::Geometry {
                key,
                parent: geometry.parent.and_then(|key| self.tree.authored_key(key)),
                ..geometry
            })
    }

    pub fn geometry(&self) -> Vec<retinue_ui::scene::Geometry> {
        self.tree.named_geometry()
    }
    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.renderer.get_mut().set_host(host);
    }

    pub fn cache_stats(&self) -> retinue_render::ResidencyStats {
        self.renderer.borrow().residency_stats()
    }

    pub fn raster_mode(&self) -> RasterMode {
        #[cfg(test)]
        return self.raster_mode;
        #[cfg(not(test))]
        RasterMode::Explicit
    }

    #[cfg(test)]
    pub fn paint_stats(&self) -> retinue_render::PaintStats {
        self.renderer.borrow().stats
    }

    #[cfg(test)]
    pub fn view_stats(&self) -> retinue_ui::runtime::UpdateStats {
        self.tree.stats
    }

    pub fn clear_resources(&mut self) {
        self.state = None;
        self.artwork.clear();
        self.actions.clear();
        self.inputs = Model::default();
        self.tree = ViewTree::new(FoundryView(self.inputs.clone()).id(ROOT), || {});
        self.renderer.get_mut().clear_cache();
    }

    fn state(view: &Foundry) -> State {
        State {
            layout: LayoutState::new(view),
            images: view.image_revision,
            hovered: view.hovered_card(),
        }
    }

    pub fn update(&mut self, view: &Foundry) {
        let state = Self::state(view);
        if self.state.as_ref() == Some(&state) || view.scale <= 0.0 {
            return;
        }
        let layout = &state.layout;
        self.inputs.offset.set(content_offset(view));
        self.inputs.filters.set(layout.filters);
        self.inputs.modal.set(view.popup_open());
        self.inputs.component_bounds.set(
            layout
                .component_bounds
                .map(|rect| card::bounds(rect, view.scale)),
        );
        let overlay_controls = if view.component_open() {
            layout.component_controls.clone()
        } else if view.filters_open() {
            Vec::new()
        } else if view.scroll.maximum() > 0.0 {
            vec![(
                Action::Scroll,
                Rect {
                    x: view.size.0.saturating_sub(view.px(18.0)),
                    y: view.viewport.y,
                    width: view.px(18.0),
                    height: view.viewport.height,
                },
            )]
        } else {
            Vec::new()
        };
        self.actions
            .retain(|key, _| key.namespace != "foundry.overlay.control");
        self.inputs
            .overlay_controls
            .replace(overlay_controls.into_iter().map(|(action, rect)| {
                let id = match action {
                    Action::Scroll => 0,
                    Action::ComponentFavorite => 1,
                    Action::ComponentSearch => 2,
                    Action::ComponentMarket => 3,
                    Action::ComponentRelics => 4,
                    _ => unreachable!(),
                };
                let key = ElementKey::new("foundry.overlay.control", id);
                self.actions.insert(key, action);
                (key, card::bounds(rect, view.scale))
            }))
            .expect("unique overlay controls");
        self.inputs
            .filter_offset
            .set((0.0, -(layout.filter_offset as f32) / view.scale));
        let content_only = self.state.as_ref().is_some_and(|old| {
            let mut layout = layout.clone();
            layout.offset = old.layout.offset;
            layout.visible = old.layout.visible.clone();
            layout.filters = old.layout.filters;
            layout.filter_offset = old.layout.filter_offset;
            layout == old.layout
        });
        if !content_only {
            self.sync_chrome(view);
        }
        let retained_window = self.state.as_ref().is_some_and(|old| {
            old.layout.scale == layout.scale
                && old.layout.grid == layout.grid
                && old.layout.revision == layout.revision
                && old.layout.visible == layout.visible
        });
        if retained_window {
            for index in [self.state.as_ref().and_then(|s| s.hovered), state.hovered]
                .into_iter()
                .flatten()
            {
                let card = &view.snapshot.items[view.filtered[index]];
                if let Some(&id) = self.identities.get(&card.id) {
                    let key = ElementKey::new("foundry.card", id);
                    if let Some(mut content) = self.inputs.cards.get(key) {
                        content.hovered = state.hovered == Some(index);
                        self.inputs.cards.set(key, content);
                    }
                }
            }
            if self
                .state
                .as_ref()
                .is_some_and(|old| old.images != state.images)
            {
                self.sync_artwork(view);
            }
        } else {
            self.sync_cards(view, layout);
        }
        self.tree
            .update(view.size, view.scale)
            .expect("Foundry tracked inputs");
        self.state = Some(state);
    }

    fn sync_chrome(&mut self, view: &Foundry) {
        let area = Rect {
            width: view.size.0,
            height: view.size.1,
            ..Rect::default()
        };
        self.inputs.area.set(card::bounds(area, view.scale));
        self.inputs.toolbar.set(card::bounds(
            Rect {
                height: view.viewport.y,
                ..area
            },
            view.scale,
        ));
        self.inputs
            .viewport
            .set(card::bounds(view.viewport, view.scale));
        self.inputs.extent.set((
            view.scroll_viewport().content_width() as f32 / view.scale,
            view.grid.content_height(view.count()) as f32 / view.scale,
        ));
        self.actions
            .retain(|key, _| key.namespace != "foundry.control");
        self.inputs
            .controls
            .replace(view.layout_controls().into_iter().enumerate().map(
                |(index, (action, rect))| {
                    let key = ElementKey::new("foundry.control", index as u64);
                    self.actions.insert(key, action);
                    (key, card::bounds(rect, view.scale))
                },
            ))
            .expect("unique Foundry controls");
    }

    fn sync_cards(&mut self, view: &Foundry, layout: &LayoutState) {
        if self
            .state
            .as_ref()
            .is_none_or(|old| old.layout.revision != layout.revision)
        {
            let ids: HashSet<_> = view
                .snapshot
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect();
            self.identities.retain(|id, _| ids.contains(id.as_str()));
        }
        let retained_content = self.state.as_ref().is_some_and(|old| {
            old.layout.revision == layout.revision
                && old.layout.grid == layout.grid
                && old.layout.scale == layout.scale
        });
        if !retained_content {
            let order = view.filtered.iter().map(|&row| {
                let id = *self
                    .identities
                    .entry(view.snapshot.items[row].id.clone())
                    .or_insert_with(|| {
                        self.next_identity += 1;
                        self.next_identity
                    });
                ElementKey::new("foundry.card", id)
            });
            let grid = UniformGrid::new(
                view.grid.columns,
                LayoutSize::new(
                    view.grid.width.max(1) as f32 / view.scale,
                    view.grid.height as f32 / view.scale,
                ),
                LayoutSize::new(
                    view.grid.gap as f32 / view.scale,
                    view.grid.gap as f32 / view.scale,
                ),
                Insets::default(),
            )
            .expect("Foundry grid tracks");
            self.inputs
                .grid
                .set(grid.arrange(KeyedOrder::new(order).expect("unique Foundry order")));
        }
        self.actions.retain(|key, _| {
            matches!(key.namespace, "foundry.control" | "foundry.overlay.control")
        });
        let mut cards = Vec::new();
        let mut visible_artwork = HashSet::new();
        for index in view.visible(0) {
            let row = view.filtered[index];
            let card = &view.snapshot.items[row];
            let id = self.identities[&card.id];
            let key = ElementKey::new("foundry.card", id);
            let content_key = ElementKey::new("foundry.card.content", id);
            let title_key = ElementKey::new("foundry.title", id);
            let mut content = if let Some(mut content) = retained_content
                .then(|| self.inputs.cards.get(key))
                .flatten()
            {
                if self
                    .state
                    .as_ref()
                    .is_some_and(|old| old.images != view.image_revision)
                {
                    let mut canvas = Canvas::new(view.scale);
                    view.record_card_content(&mut canvas, card, &content.layout);
                    content.artwork = self.artwork(content_key, canvas.commands);
                }
                content
            } else {
                let layout = card::CardLayout::new(view, card);
                let mut canvas = Canvas::new(view.scale);
                view.record_card_content(&mut canvas, card, &layout);
                let artwork = self.artwork(content_key, canvas.commands);
                let title = self.artwork(title_key, card::title(view, card, &layout));
                card::Content::new(view, card, index, layout, title, artwork)
            };
            content.hovered = view.hovered_card() == Some(index);
            visible_artwork.extend([content_key, title_key]);
            self.actions.insert(
                ElementKey::new("foundry.favorite", id),
                Action::Favorite(row),
            );
            for part in 0..content.layout.parts.len() {
                let part_id = id * 8 + part as u64;
                self.actions.insert(
                    ElementKey::new("foundry.component", part_id),
                    Action::Component(row, part),
                );
                if card.components[part].owned_relic {
                    self.actions.insert(
                        ElementKey::new("foundry.component.relic", part_id),
                        Action::ComponentRelic(row, part),
                    );
                }
            }
            cards.push((key, content));
        }
        self.inputs
            .cards
            .replace(cards)
            .expect("unique Foundry items");
        self.artwork.retain(|key, _| visible_artwork.contains(key));
    }

    fn sync_artwork(&mut self, view: &Foundry) {
        for index in view.visible(0) {
            let card = &view.snapshot.items[view.filtered[index]];
            let id = self.identities[&card.id];
            let key = ElementKey::new("foundry.card", id);
            let mut content = self.inputs.cards.get(key).expect("mounted Foundry card");
            let mut canvas = Canvas::new(view.scale);
            view.record_card_content(&mut canvas, card, &content.layout);
            content.artwork =
                self.artwork(ElementKey::new("foundry.card.content", id), canvas.commands);
            self.inputs.cards.set(key, content);
        }
    }

    fn artwork(&mut self, key: ElementKey, commands: Vec<Command>) -> DrawList {
        if let Some(old) = self.artwork.get(&key)
            && *old.0 == commands
        {
            return old.clone();
        }
        let paint = DrawList::from(commands);
        self.artwork.insert(key, paint.clone());
        paint
    }

    pub fn hit(&self, point: (f64, f64)) -> Option<Action> {
        let scale = self.tree.frame().scale();
        self.tree
            .hit_test((point.0 as f32 / scale, point.1 as f32 / scale))
            .and_then(|key| self.action(key))
    }

    fn action(&self, key: ElementKey) -> Option<Action> {
        filters::declaration::action(key).or_else(|| self.actions.get(&key).copied())
    }

    pub fn pointer(
        &mut self,
        point: (f64, f64),
        phase: retinue_ui::input::PointerPhase,
    ) -> (Option<Action>, Option<Action>) {
        let scale = self.tree.frame().scale();
        let result = self.tree.dispatch(&retinue_ui::input::Event::Pointer {
            id: retinue_ui::input::PointerId::MOUSE,
            phase,
            position: (point.0 as f32 / scale, point.1 as f32 / scale),
            modifiers: Default::default(),
        });
        let action = |id: Option<retinue_ui::runtime::ViewId>| {
            id.and_then(|id| self.tree.input_key(id))
                .and_then(|key| self.action(key))
        };
        (action(result.target), action(result.activated))
    }

    pub fn controls(&self, items: bool) -> Vec<(Action, Rect)> {
        self.geometry()
            .iter()
            .filter_map(|node| {
                let action = self.actions.get(&node.key)?;
                if matches!(
                    action,
                    Action::Scroll
                        | Action::ComponentFavorite
                        | Action::ComponentSearch
                        | Action::ComponentMarket
                        | Action::ComponentRelics
                ) {
                    return None;
                }
                let item = matches!(
                    action,
                    Action::Favorite(_) | Action::Component(_, _) | Action::ComponentRelic(_, _)
                );
                (item == items && !node.hit.is_empty()).then_some((*action, node.hit))
            })
            .collect()
    }

    pub fn control_bounds(&self, action: Action) -> Option<Rect> {
        self.actions.iter().find_map(|(&key, &candidate)| {
            if candidate != action {
                return None;
            }
            self.bounds(key)
                .map(|geometry| geometry.hit)
                .filter(|rect| !rect.is_empty())
        })
    }

    pub fn record_chrome(
        &mut self,
        background: Vec<Command>,
        toolbar: Vec<Command>,
        overlays: Vec<Command>,
    ) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        self.inputs.background_paint.set(DrawList::from(background));
        self.inputs.toolbar_paint.set(DrawList::from(toolbar));
        self.inputs.overlays_paint.set(DrawList::from(overlays));
        let state = self.state.as_ref().expect("resolved Foundry geometry");
        self.tree
            .update(state.layout.size, state.layout.scale)
            .expect("Foundry chrome");
        #[cfg(all(test, feature = "profiling"))]
        let updated = Instant::now();
        #[cfg(all(test, feature = "profiling"))]
        self.paint_profile
            .set([updated - started, Duration::ZERO, Duration::ZERO]);
    }

    pub fn paint(&self, view: &Foundry, p: &mut Painter<'_>) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        let mut renderer = self.renderer.borrow_mut();
        renderer.prepare_for(self.tree.frame(), self.raster_mode(), &p.visible_regions());
        #[cfg(all(test, feature = "profiling"))]
        let prepared = Instant::now();
        renderer.draw_with(
            self.tree.frame(),
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
        #[cfg(all(test, feature = "profiling"))]
        {
            let mut profile = self.paint_profile.get();
            profile[1] = prepared - started;
            profile[2] = prepared.elapsed();
            self.paint_profile.set(profile);
        }
    }
}

fn content_offset(view: &Foundry) -> (f32, f32) {
    let padding = view.scroll_viewport().padding;
    (
        padding.left as f32 / view.scale,
        (padding.top as f32 - view.offset as f32) / view.scale,
    )
}

pub(super) fn position(view: impl IntoElement, bounds: Bounds) -> Element {
    view.frame_aligned(
        bounds.width,
        bounds.height,
        retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING,
    )
    .offset(bounds.x, bounds.y)
}

impl Foundry {
    pub(super) fn input_composition(&self) -> std::cell::Ref<'_, Composition> {
        let layout = LayoutState::new(self);
        let stale = self
            .composition
            .borrow()
            .state
            .as_ref()
            .is_none_or(|state| state.layout != layout);
        if stale && self.active {
            self.composition.borrow_mut().update(self);
        }
        self.composition.borrow()
    }

    pub(super) fn resolved_composition(&self) -> std::cell::Ref<'_, Composition> {
        let stale = self.composition.borrow().state.as_ref() != Some(&Composition::state(self));
        if stale && self.active {
            self.composition.borrow_mut().update(self);
        }
        self.composition.borrow()
    }

    pub fn controls(&self) -> Vec<(Action, Rect)> {
        if self.scale <= 0.0 {
            return self.layout_controls();
        }
        self.input_composition().controls(false)
    }
}
