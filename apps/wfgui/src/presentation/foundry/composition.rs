use super::{FoundryView, foundry_model::Model, *};
use retinue_render::{Bounds, Command, Key as ElementKey, RasterMode, Renderer};
use retinue_ui::{
    authoring::{Body, ViewExt, compose},
    drawing::DrawList,
    drawing::PixelCanvas as Canvas,
    environment::Environment,
    ir::InstanceGraph,
    layout::Insets,
    layout::LayoutSize,
    layout::grid::UniformGrid,
    state::KeyedOrder,
};
use std::{cell::OnceCell, rc::Rc};

const ROOT: ElementKey = ElementKey::new("foundry", 0);
pub(super) const TOOLBAR: ElementKey = ElementKey::new("foundry", 1);
pub(super) const GRID: ElementKey = ElementKey::new("foundry", 2);
pub(super) const OVERLAYS: ElementKey = ElementKey::new("foundry", 3);
pub(super) const CONTENT: ElementKey = ElementKey::new("foundry", 4);

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
    pub tree: InstanceGraph,
    work: retinue_ui::ir::Work,
    pub(super) renderer: Rc<RefCell<Renderer>>,
    #[cfg(test)]
    pub raster_mode: RasterMode,
    inputs: Model,
    state: Option<State>,
    pub(super) execution: Option<retinue_ui::runtime::LayoutExecution>,
    controls: OnceCell<Vec<(ElementKey, Rect)>>,
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
            tree: mount(inputs.clone()),
            work: Default::default(),
            renderer: Rc::new(RefCell::new(Renderer::new(
                retinue_render::ResidencyPolicy::desktop(32 * 1024 * 1024),
            ))),
            #[cfg(test)]
            raster_mode: RasterMode::Explicit,
            inputs,
            state: None,
            execution: None,
            controls: OnceCell::new(),
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
            .named_bounds(key)
            .expect("unique Foundry key")
            .map(|geometry| retinue_ui::scene::Geometry {
                key,
                parent: geometry.parent.and_then(|key| self.tree.authored_key(key)),
                ..geometry
            })
    }

    pub fn geometry(&self) -> Vec<retinue_ui::scene::Geometry> {
        self.tree.named_geometry().collect()
    }
    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.renderer.borrow_mut().set_host(host);
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
    pub fn view_stats(&self) -> retinue_ui::ir::Work {
        self.work
    }

    #[cfg(test)]
    pub fn grid_builds(&self) -> usize {
        self.inputs.grid_builds.get()
    }

    pub fn clear_resources(&mut self) {
        self.state = None;
        self.controls.take();
        self.artwork.clear();
        self.actions.clear();
        self.inputs = Model::default();
        self.tree = mount(self.inputs.clone());
        self.renderer.borrow_mut().clear_cache();
    }

    fn state(view: &Foundry) -> State {
        State {
            layout: LayoutState::new(view),
            images: view.image_revision,
            hovered: view.hovered_card(),
        }
    }

    pub fn update(&mut self, view: &Foundry) -> bool {
        self.synchronize(view);
        if view.scale <= 0.0 {
            return true;
        }
        if !self.tree.has_pending_inputs()
            && !self.tree.is_layout_pending()
            && self.tree.frame().size() == view.size
            && self.tree.frame().scale() == view.scale
        {
            return true;
        }
        let update = self
            .tree
            .advance_at(
                view.size,
                view.scale,
                retinue_ui::timeline::FrameTime::now(),
                self.execution.as_ref(),
            )
            .expect("Foundry tracked inputs");
        self.work = update.work;
        let ready = update.completion.expect("Foundry layout").is_ready();
        if ready {
            self.controls.take();
        }
        ready
    }

    fn synchronize(&mut self, view: &Foundry) {
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
        if let Some(old) = self.state.as_ref().filter(|old| {
            old.layout.scale == layout.scale
                && old.layout.grid == layout.grid
                && old.layout.revision == layout.revision
        }) {
            let previous = old.layout.visible.clone();
            let old_hover = old.hovered;
            let images_changed = old.images != state.images;
            if previous != layout.visible {
                self.sync_window(view, &previous, &layout.visible, state.hovered);
            }
            if images_changed {
                self.sync_artwork(
                    view,
                    previous.start.max(layout.visible.start)..previous.end.min(layout.visible.end),
                );
            }
            if old_hover != state.hovered {
                for index in [old_hover, state.hovered]
                    .into_iter()
                    .flatten()
                    .filter(|i| layout.visible.contains(i))
                {
                    let id = self.identities[&view.snapshot.items[view.filtered[index]].id];
                    let key = ElementKey::new("foundry.card", id);
                    let mut content = self.inputs.cards.get(key).expect("visible Foundry card");
                    content.hovered = state.hovered == Some(index);
                    self.inputs.cards.set(key, content);
                }
            }
        } else {
            self.sync_cards(view, layout, state.hovered);
        }
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

    fn sync_cards(&mut self, view: &Foundry, layout: &LayoutState, hovered: Option<usize>) {
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

        self.actions.retain(|key, _| {
            matches!(key.namespace, "foundry.control" | "foundry.overlay.control")
        });
        let cards = self.inputs.cards.clone();
        cards
            .replace(layout.visible.clone().map(|index| {
                let key = ElementKey::new(
                    "foundry.card",
                    self.identities[&view.snapshot.items[view.filtered[index]].id],
                );
                (key, self.build_card(view, index, key, hovered))
            }))
            .expect("unique Foundry items");
        let visible = cards.order();
        self.artwork.retain(|key, _| {
            visible
                .index(ElementKey::new("foundry.card", key.value))
                .is_some()
        });
    }

    fn sync_window(
        &mut self,
        view: &Foundry,
        old: &std::ops::Range<usize>,
        new: &std::ops::Range<usize>,
        hovered: Option<usize>,
    ) {
        for index in old.clone().filter(|index| !new.contains(index)) {
            let card = &view.snapshot.items[view.filtered[index]];
            let id = self.identities[&card.id];
            self.artwork.remove(&ElementKey::new("foundry.title", id));
            self.artwork
                .remove(&ElementKey::new("foundry.card.content", id));
            self.actions
                .remove(&ElementKey::new("foundry.favorite", id));
            for part in 0..card.components.len().min(6) {
                for ns in ["foundry.component", "foundry.component.relic"] {
                    self.actions
                        .remove(&ElementKey::new(ns, id * 8 + part as u64));
                }
            }
        }
        let order = KeyedOrder::new(new.clone().map(|index| {
            ElementKey::new(
                "foundry.card",
                self.identities[&view.snapshot.items[view.filtered[index]].id],
            )
        }))
        .expect("unique Foundry items");
        self.inputs.cards.clone().set_order_with(order, |pos, key| {
            self.build_card(view, new.start + pos, key, hovered)
        });
    }

    fn build_card(
        &mut self,
        view: &Foundry,
        index: usize,
        key: ElementKey,
        hovered: Option<usize>,
    ) -> card::Content {
        let row = view.filtered[index];
        let card = &view.snapshot.items[row];
        let id = key.value;
        let layout = card::CardLayout::new(view, card);
        let mut canvas = Canvas::new(view.scale);
        view.record_card_content(&mut canvas, card, &layout);
        let artwork = self.artwork(ElementKey::new("foundry.card.content", id), canvas.commands);
        let title = self.artwork(
            ElementKey::new("foundry.title", id),
            card::title(view, card, &layout),
        );
        self.actions.insert(
            ElementKey::new("foundry.favorite", id),
            Action::Favorite(row),
        );
        for part in 0..layout.parts.len() {
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
        let mut content = card::Content::new(view, card, index, layout, title, artwork);
        content.hovered = hovered == Some(index);
        content
    }

    fn sync_artwork(&mut self, view: &Foundry, retained: std::ops::Range<usize>) {
        for index in retained {
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

    pub(super) fn action(&self, key: ElementKey) -> Option<Action> {
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
        (action(result.pointer_hit), action(result.activated))
    }

    fn published_controls(&self) -> &[(ElementKey, Rect)] {
        self.controls.get_or_init(|| {
            self.tree
                .scene()
                .geometry()
                .iter()
                .filter(|g| !g.hit.is_empty())
                .filter_map(|g| {
                    let key = self.tree.authored_key(g.key)?;
                    self.actions.contains_key(&key).then_some((key, g.hit))
                })
                .collect()
        })
    }

    #[cfg(test)]
    pub(super) fn has_control_index(&self) -> bool {
        self.controls.get().is_some()
    }

    pub fn controls(&self, items: bool) -> Vec<(Action, Rect)> {
        self.published_controls()
            .iter()
            .filter_map(|(key, rect)| {
                let action = self.actions.get(key)?;
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
                (item == items).then_some((*action, *rect))
            })
            .collect()
    }

    pub fn control_bounds(&self, action: Action) -> Option<Rect> {
        self.published_controls()
            .iter()
            .find_map(|(key, rect)| (self.actions.get(key) == Some(&action)).then_some(*rect))
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
        let update = self
            .tree
            .advance(state.layout.size, state.layout.scale, None)
            .expect("Foundry chrome");
        self.work = update.work;
        assert!(update.completion.expect("Foundry chrome layout").is_ready());
        #[cfg(all(test, feature = "profiling"))]
        let updated = Instant::now();
        #[cfg(all(test, feature = "profiling"))]
        self.paint_profile
            .set([updated - started, Duration::ZERO, Duration::ZERO]);
    }

    pub fn prepare_rasters(&self, repair: &retinue_render::damage::Region) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        self.renderer
            .borrow_mut()
            .prepare_for(self.tree.frame(), self.raster_mode(), repair);
        #[cfg(all(test, feature = "profiling"))]
        {
            let mut profile = self.paint_profile.get();
            profile[1] = started.elapsed();
            self.paint_profile.set(profile);
        }
    }

    pub fn paint(&self, view: &Foundry, p: &mut Painter<'_>) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        self.renderer.borrow_mut().draw_with(
            self.tree.frame(),
            p,
            Rect {
                width: view.size.0,
                height: view.size.1,
                ..Rect::default()
            },
            |_, _| unreachable!("Foundry ink is recorded"),
        );
        #[cfg(all(test, feature = "profiling"))]
        {
            let mut profile = self.paint_profile.get();
            profile[2] = started.elapsed();
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

pub(super) fn position(view: impl Body, bounds: Bounds) -> impl Body {
    view.frame(crate::presentation::widgets::compiled::frame(
        bounds.width,
        bounds.height,
    ))
    .offset(bounds.x, bounds.y)
}

fn mount(model: Model) -> InstanceGraph {
    let mut graph = InstanceGraph::default();
    let root = graph
        .mount_body(
            move |_| compose(FoundryView(model.clone())).named(ROOT),
            Environment::default(),
        )
        .expect("Foundry template");
    graph.set_root(Some(root)).unwrap();
    graph
}

impl Foundry {
    fn sync_input_geometry(&self) {
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
    }

    pub(super) fn input_composition(&self) -> std::cell::Ref<'_, Composition> {
        self.sync_input_geometry();
        self.composition.borrow()
    }

    pub(super) fn input_composition_mut(&self) -> std::cell::RefMut<'_, Composition> {
        self.sync_input_geometry();
        self.composition.borrow_mut()
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
