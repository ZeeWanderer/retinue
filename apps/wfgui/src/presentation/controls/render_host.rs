use super::*;
use retinue_render::{
    Bounds, CachePolicy, Color, Command, ImageFit, Key as ElementKey, LayerPolicy, RasterMode,
    Renderer,
};
use retinue_ui::{drawing::PixelCanvas as Canvas, scene::Scene};

const OVERLAYS: ElementKey = super::view_host::OVERLAYS;
const CHROME: ElementKey = super::view_host::CHROME;

pub(super) struct Composition {
    pub(super) renderer: RefCell<Renderer>,
    targets: HashMap<ElementKey, usize>,
    pub(super) host: super::view_host::Host,
    images: HashMap<ElementKey, Request>,
    slots: HashMap<ElementKey, std::ops::Range<usize>>,
    #[cfg(all(test, feature = "profiling"))]
    paint_profile: (Duration, Duration, Duration),
}

impl Default for Composition {
    fn default() -> Self {
        let renderer = Renderer::with_layers(
            retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024),
            LayerPolicy::desktop(),
        );
        #[cfg(test)]
        let renderer = if std::env::var_os("RETINUE_BENCH_NO_LAYERS").is_some() {
            Renderer::new(retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024))
        } else {
            renderer
        };
        Self {
            renderer: RefCell::new(renderer),
            targets: HashMap::new(),
            host: super::view_host::Host::new(retinue_ui::view::empty()),
            images: HashMap::new(),
            slots: HashMap::new(),
            #[cfg(all(test, feature = "profiling"))]
            paint_profile: Default::default(),
        }
    }
}

impl Composition {
    pub fn begin_bindings(&mut self) {
        self.slots.clear();
        self.targets.clear();
    }

    pub(super) fn scene(&self) -> &Scene {
        self.host.tree.scene()
    }

    pub(super) fn bounds(&self, key: ElementKey) -> Option<retinue_ui::scene::Geometry> {
        self.host.bounds(key)
    }

    pub(super) fn input_bounds(&self, key: ElementKey) -> Option<retinue_ui::scene::Geometry> {
        self.host
            .tree
            .geometry(self.host.tree.unique(key).expect("unique input owner")?)
    }

    pub fn assets(&self) -> Vec<Asset> {
        self.host.images.assets()
    }

    pub fn requests(&self, media: &Media) -> Vec<Request> {
        self.images
            .values()
            .cloned()
            .chain(self.host.images.requests(media))
            .collect()
    }

    pub fn deadline(&self, _now: Instant) -> Option<Instant> {
        self.host.tree.next_deadline()
    }

    pub fn image_damage(
        &mut self,
        keys: &std::collections::HashSet<retinue_render::thumbnails::Key>,
        media: &Media,
        size: (u32, u32),
        scale: f32,
    ) -> retinue_render::damage::Damage {
        self.host.images.publish(media);
        for (&key, request) in &self.images {
            if keys.contains(&request.key) {
                bind_icon(&self.host, key, request, media);
            }
        }
        if self.host.tree.needs_update() {
            self.host
                .update(size, scale, retinue_ui::timeline::FrameTime::now())
        } else {
            Default::default()
        }
    }

    fn reset_view(&mut self) {
        self.install_view(retinue_ui::view::empty());
    }

    fn install_view(&mut self, view: impl retinue_ui::view::IntoElement) {
        self.host = super::view_host::Host::new(view);
        self.targets.clear();
        self.images.clear();
        self.slots.clear();
        self.renderer.get_mut().clear_cache();
    }

    fn update_view(
        &mut self,
        size: (u32, u32),
        scale: f32,
        time: retinue_ui::timeline::FrameTime,
    ) -> retinue_render::damage::Damage {
        let damage = self.host.update(size, scale, time);
        self.images.retain(|key, _| {
            self.host
                .bounds(*key)
                .is_some_and(|node| !node.hit.is_empty())
        });
        damage
    }

    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.renderer.get_mut().set_host(host);
    }

    pub fn clear_cache(&mut self) {
        self.renderer.get_mut().clear_cache();
    }

    pub fn cache_stats(&self) -> retinue_render::ResidencyStats {
        self.renderer.borrow().residency_stats()
    }

    pub fn update(&mut self, ui: &ControlHost) {
        self.host.prepare(ui.size, ui.scale);
    }

    fn command(
        &mut self,
        ui: &ControlHost,
        media: &Media,
        index: usize,
        key: ElementKey,
    ) -> Option<Command> {
        let draw = &ui.visuals[index].0;
        let bounds = draw.bounds(ui.scale);
        if draw.live() {
            self.targets.insert(key, index);
            Some(Command::External {
                key,
                bounds: logical(bounds, ui.scale),
            })
        } else {
            Some(Command::Translate {
                offset: (bounds.x as f32 / ui.scale, bounds.y as f32 / ui.scale),
                commands: Rc::new(draw.commands(ui, Some(media))?),
            })
        }
    }

    pub fn prepare_paint(&mut self, ui: &ControlHost, media: &Media) {
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        self.targets.clear();
        let mut canvas = Canvas::new(ui.scale);
        ui.paint_overlays(&mut canvas);
        self.host
            .record(OVERLAYS, canvas.commands, CachePolicy::Never);

        let slots = std::mem::take(&mut self.slots);
        let mut bound = vec![false; ui.visuals.len()];
        for (&key, indices) in &slots {
            let Some(node) = self.bounds(key) else {
                continue;
            };
            let origin = (
                (node.layout.x * ui.scale).round() as i32,
                (node.layout.y * ui.scale).round() as i32,
            );
            let local_clip = |clip: Rect| {
                Bounds::new(
                    (clip.x as f32 - origin.0 as f32) / ui.scale,
                    (clip.y as f32 - origin.1 as f32) / ui.scale,
                    clip.width as f32 / ui.scale,
                    clip.height as f32 / ui.scale,
                )
            };
            let mut commands = Vec::new();
            for index in indices.clone() {
                bound[index] = true;
                let target = ElementKey::new(
                    "page.control.live",
                    identity_hash((key, index - indices.start)),
                );
                let Some(command) = self.command(ui, media, index, target) else {
                    continue;
                };
                let (clip, offset) = ui.visuals[index]
                    .1
                    .expect("control paint has a local viewport");
                commands.push(if (clip, offset) == (node.hit, origin) {
                    command
                } else {
                    Command::Clip {
                        bounds: local_clip(clip),
                        commands: Rc::new(vec![Command::Translate {
                            offset: (
                                (offset.0 - origin.0) as f32 / ui.scale,
                                (offset.1 - origin.1) as f32 / ui.scale,
                            ),
                            commands: Rc::new(vec![command]),
                        }]),
                    }
                });
            }
            self.host.record(
                key,
                vec![Command::Clip {
                    bounds: Bounds::new(0.0, 0.0, node.layout.width, node.layout.height),
                    commands: Rc::new(commands),
                }],
                CachePolicy::Never,
            );
        }
        self.slots = slots;

        let mut commands = Vec::new();
        for (index, (_, viewport)) in ui.visuals.iter().enumerate() {
            if bound[index] {
                continue;
            }
            let target = ElementKey::new("page.chrome.live", index as u64);
            let Some(command) = self.command(ui, media, index, target) else {
                continue;
            };
            let (clip, offset) = viewport.unwrap_or((
                Rect {
                    width: ui.size.0,
                    height: ui.size.1,
                    ..Rect::default()
                },
                (0, 0),
            ));
            commands.push(Command::Clip {
                bounds: logical(clip, ui.scale),
                commands: Rc::new(vec![Command::Translate {
                    offset: (offset.0 as f32 / ui.scale, offset.1 as f32 / ui.scale),
                    commands: Rc::new(vec![command]),
                }]),
            });
        }
        self.host.record(CHROME, commands, CachePolicy::Never);
        for (&key, request) in &self.images {
            bind_icon(&self.host, key, request, media);
        }
        self.host.images.publish(media);
        self.host.prepare(ui.size, ui.scale);
        #[cfg(all(test, feature = "profiling"))]
        {
            self.paint_profile = (started.elapsed(), Duration::ZERO, Duration::ZERO);
        }
    }

    pub fn draw_prepared(&mut self, ui: &ControlHost, p: &mut Painter<'_>) {
        #[cfg(all(test, feature = "profiling"))]
        let prepared = Instant::now();
        self.renderer.borrow_mut().prepare_for(
            self.scene().frame(),
            RasterMode::Explicit,
            &p.visible_regions(),
        );
        #[cfg(all(test, feature = "profiling"))]
        let drawing = Instant::now();
        self.renderer.borrow_mut().draw_with(
            self.scene().frame(),
            p,
            Rect {
                width: ui.size.0,
                height: ui.size.1,
                ..Rect::default()
            },
            |key, p| {
                let Some(&index) = self.targets.get(&key) else {
                    return;
                };
                let draw = &ui.visuals[index].0;
                let bounds = draw.bounds(ui.scale);
                // External callbacks start at the leaf; editors retain slot-local rectangles.
                p.translated((-(bounds.x as i32), -(bounds.y as i32)), |p| {
                    ui.paint_live(p, draw)
                });
            },
        );
        #[cfg(all(test, feature = "profiling"))]
        {
            self.paint_profile = (self.paint_profile.0, drawing - prepared, drawing.elapsed());
        }
    }

    pub fn report(&self) -> serde_json::Value {
        let renderer = self.renderer.borrow();
        let tree = &self.host.tree;
        serde_json::json!({"declarative":true, "view":{
            "bodies":tree.stats.bodies,"mounted":tree.stats.mounted,
            "unmounted":tree.stats.unmounted,"projected":tree.stats.projected,
        },
            "nodes":self.scene().stats.nodes,"chunks":self.scene().stats.visible_chunks,"layout_computations":self.scene().stats.layout_computations,"recordings":self.scene().stats.recordings,
            "rasters":renderer.stats.prepared_rasters,"blits":renderer.stats.blits,"repaired_pixels":renderer.stats.repaired_pixels,"raster_bytes":renderer.residency_stats().live_bytes,
            "simplified_commands":renderer.stats.simplified_commands,"promoted_layers":renderer.stats.promoted_layers,"demoted_layers":renderer.stats.demoted_layers,
            "layer_bytes":renderer.stats.layer_bytes,"layer_prepare_ns":renderer.stats.layer_prepare_ns,
            "geometry":tree.named_geometry().iter().map(|node|serde_json::json!({
                "scope":node.key.namespace,"id":node.key.value,"parent":node.parent.map(|key|(key.namespace,key.value)),
                "layout":[node.layout.x,node.layout.y,node.layout.width,node.layout.height],
                "hit":[node.hit.x,node.hit.y,node.hit.width,node.hit.height],
                "paint":[node.paint.x,node.paint.y,node.paint.width,node.paint.height],
                "interactive":node.interactive,
            })).collect::<Vec<_>>()})
    }

    pub fn declared_elements(&self) -> Vec<serde_json::Value> {
        let mut elements = Vec::new();
        for instance in self.scene().frame().instances() {
            let key = self
                .host
                .tree
                .authored_key(instance.key)
                .unwrap_or(instance.key);
            if self.slots.contains_key(&key)
                || key == CHROME
                || key == OVERLAYS
                || self.images.contains_key(&key)
            {
                continue;
            }
            super::report::commands(
                &mut elements,
                instance.program.commands(),
                instance.origin(),
                instance.clip(),
                self.scene().frame().scale(),
            );
        }
        elements
    }
}

fn bind_icon(host: &super::view_host::Host, key: ElementKey, request: &Request, media: &Media) {
    let Some(node) = host.bounds(key) else {
        return;
    };
    let commands = media
        .embedded_resource(request)
        .map(|image| {
            vec![Command::Image {
                bounds: Bounds::new(0.0, 0.0, node.layout.width, node.layout.height),
                image: image.clone().into(),
                fit: ImageFit::Contain,
            }]
        })
        .unwrap_or_default();
    host.record(key, commands, CachePolicy::Never);
}

impl ControlHost {
    pub fn view_control(&mut self, key: ElementKey, bind: impl FnOnce(&mut ControlHost, Rect)) {
        let Some(node) = self.composition.borrow().bounds(key) else {
            return;
        };
        if node.hit.is_empty() {
            return;
        }
        let previous = self.viewport.replace((
            node.hit,
            (
                (node.layout.x * self.scale).round() as i32,
                (node.layout.y * self.scale).round() as i32,
            ),
        ));
        let first = self.visuals.len();
        let first_button = self.buttons.len();
        bind(
            self,
            Rect {
                width: (node.layout.width * self.scale).round() as u32,
                height: (node.layout.height * self.scale).round() as u32,
                ..Rect::default()
            },
        );
        let origin = (
            (node.layout.x * self.scale).round(),
            (node.layout.y * self.scale).round(),
        );
        for button in &mut self.buttons[first_button..] {
            let local = Bounds::new(
                (button.rect.x as f32 - origin.0) / self.scale,
                (button.rect.y as f32 - origin.1) / self.scale,
                button.rect.width as f32 / self.scale,
                button.rect.height as f32 / self.scale,
            );
            button.binding = Some((
                key,
                if button.rect == node.hit {
                    None
                } else {
                    Some(local)
                },
            ));
        }
        self.viewport = previous;
        self.composition
            .get_mut()
            .slots
            .insert(key, first..self.visuals.len());
    }
    pub fn install_view(&mut self, view: impl retinue_ui::view::IntoElement) {
        self.composition.get_mut().install_view(view);
    }

    pub fn reset_view(&mut self) {
        self.composition.get_mut().reset_view();
    }

    pub fn update_view(
        &mut self,
        time: retinue_ui::timeline::FrameTime,
    ) -> retinue_render::damage::Damage {
        let damage = self
            .composition
            .borrow_mut()
            .update_view(self.size, self.scale, time);
        self.sync_input();
        damage
    }

    #[cfg(all(test, feature = "profiling"))]
    pub fn view_profile(
        &self,
    ) -> (
        retinue_ui::runtime::UpdateStats,
        retinue_ui::runtime::UpdateProfile,
        retinue_ui::scene::UpdateProfile,
        retinue_ui::scene::UpdateStats,
    ) {
        let composition = self.composition.borrow();
        let tree = &composition.host.tree;
        (
            tree.stats,
            tree.profile,
            tree.scene().profile,
            tree.scene().stats,
        )
    }

    #[cfg(all(test, feature = "profiling"))]
    pub fn paint_profile(&self) -> (Duration, Duration, Duration) {
        self.composition.borrow().paint_profile
    }

    pub fn view_bounds(&self, key: ElementKey) -> Option<Rect> {
        let composition = self.composition.borrow();
        let node = composition.bounds(key)?;
        Some(node.hit)
    }

    #[cfg(test)]
    pub fn view_geometry(&self, key: ElementKey) -> Option<retinue_ui::scene::Geometry> {
        self.composition.borrow().bounds(key)
    }

    pub fn view_hit(
        &mut self,
        node: ElementKey,
        action: impl Into<String>,
        label: impl Into<String>,
        enabled: bool,
    ) {
        if let Some(rect) = self
            .view_bounds(node)
            .filter(|r| r.width > 0 && r.height > 0)
        {
            self.hit(action, rect, label, enabled);
            self.buttons.last_mut().unwrap().binding = Some((node, None));
        }
    }

    pub fn view_icon(&mut self, node: ElementKey, name: &str, tint: Option<[u8; 4]>) {
        let composition = self.composition.get_mut();
        let Some(bounds) = composition.bounds(node) else {
            return;
        };
        if bounds.hit.is_empty() {
            return;
        }
        let bytes = media::icon(name);
        let size = (bounds.layout.width.max(bounds.layout.height) * self.scale).round() as u32;
        let mut request = self
            .embedded
            .entry((bytes.as_ptr() as usize, size))
            .or_insert_with(|| Request::embedded(bytes, size, None))
            .clone();
        if let Some(color) = tint {
            request = request.tinted([color[2], color[1], color[0], color[3]]);
        }
        composition.images.insert(node, request);
    }

    pub fn font_resource(&self, weight: Weight) -> Rc<CachedFont> {
        match weight {
            Weight::Light => self.light.clone(),
            Weight::Regular => self.regular.clone(),
            Weight::Medium => self.medium.clone(),
            Weight::Bold => self.bold.clone(),
        }
    }
}

#[cfg(test)]
impl ControlHost {
    pub(crate) fn paint_snapshot(
        &self,
        target: &mut retinue_render::PaintFrame,
        clip: Rect,
    ) -> retinue_render::FrameSnapshot {
        let composition = self.composition.borrow();
        let frame = composition.scene().frame();
        let instances = frame
            .instances()
            .iter()
            .cloned()
            .map(|mut instance| {
                instance.set_pixels(instance.origin(), instance.clip().intersection(clip));
                instance
            })
            .collect();
        target
            .replace(
                frame.size(),
                frame.scale(),
                instances,
                frame.groups().to_vec(),
            )
            .unwrap();
        assert!(
            !target.has_external_paint(),
            "replay needs a recorded replacement for live paint"
        );
        target.snapshot()
    }

    pub(crate) fn set_layer_caching(&mut self, enabled: bool) {
        self.composition.get_mut().renderer = RefCell::new(if enabled {
            Renderer::with_layers(
                retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024),
                LayerPolicy::desktop(),
            )
        } else {
            Renderer::new(retinue_render::ResidencyPolicy::desktop(16 * 1024 * 1024))
        });
    }

    pub(crate) fn paint_stats(&self) -> retinue_render::PaintStats {
        self.composition.borrow().renderer.borrow().stats
    }

    pub(crate) fn with_full_replay(&self, paint: impl FnOnce()) {
        let mut direct = Renderer::new(retinue_render::ResidencyPolicy::desktop(0));
        direct.set_optimizations(false);
        let old = self.composition.borrow().renderer.replace(direct);
        paint();
        self.composition.borrow().renderer.replace(old);
    }
}

impl Visual {
    fn live(&self) -> bool {
        matches!(self, Self::Field(..))
    }

    fn commands(&self, ui: &ControlHost, media: Option<&Media>) -> Option<Vec<Command>> {
        let bounds = self.bounds(ui.scale);
        let mut canvas = Canvas::at((bounds.x, bounds.y), ui.scale);
        match self {
            Self::Fill(r, radius, color) => canvas.fill(*r, ui.px(*radius), *color),
            Self::Outline(r, radius, color) => {
                canvas.outline(*r, ui.px(*radius), ui.px(1.0).max(1), *color)
            }
            Self::Text(r, text, size, color, weight) => {
                let font = match weight {
                    Weight::Light => &ui.light,
                    Weight::Regular => &ui.regular,
                    Weight::Medium => &ui.medium,
                    Weight::Bold => &ui.bold,
                };
                canvas.text(*r, font.clone(), text.clone(), *size, *color);
            }
            Self::Line(a, b, color) => canvas.commands.push(Command::Line {
                from: (
                    (a.0 as f32 - bounds.x as f32) / ui.scale,
                    (a.1 as f32 - bounds.y as f32) / ui.scale,
                ),
                to: (
                    (b.0 as f32 - bounds.x as f32) / ui.scale,
                    (b.1 as f32 - bounds.y as f32) / ui.scale,
                ),
                thickness: 2.0,
                color: Color::from_bgra(*color),
            }),
            Self::Polygon(points, color) => canvas.commands.push(Command::Polygon {
                points: points
                    .iter()
                    .map(|p| {
                        [
                            (p[0] as f32 - bounds.x as f32) / ui.scale,
                            (p[1] as f32 - bounds.y as f32) / ui.scale,
                        ]
                    })
                    .collect(),
                color: Color::from_bgra(*color),
            }),
            Self::Image(r, asset) | Self::Covered(r, asset, _) => {
                let (edge, fit) = match self {
                    Self::Covered(_, _, edge) => (*edge, ImageFit::Cover),
                    _ => (r.width.max(r.height), ImageFit::Contain),
                };
                if let Some(image) = media?.image_resource(asset, edge) {
                    canvas.image(*r, image.clone(), fit);
                }
            }
            #[cfg(test)]
            Self::CircleImage(r, asset) => {
                if let Some(image) = media?.image_resource(asset, r.width.max(r.height)) {
                    canvas.image(*r, image.clone(), ImageFit::Circle);
                }
            }
            Self::Icon(r, request)
            | Self::Texture(r, request, _)
            | Self::ClippedIcon(_, r, request) => {
                if let Some(image) = media?.embedded_resource(request) {
                    let fit = match self {
                        Self::Texture(_, _, true) => ImageFit::Cover,
                        Self::Texture(_, _, false) => ImageFit::Stretch,
                        _ => ImageFit::Contain,
                    };
                    if let Self::ClippedIcon(clip, _, _) = self {
                        canvas.clip(*clip, |c| c.image(*r, image.clone(), fit));
                    } else {
                        canvas.image(*r, image.clone(), fit);
                    }
                }
            }
            #[cfg(test)]
            Self::ComponentBase(r, ready) => canvas.fill(
                *r,
                r.width / 2,
                if *ready {
                    crate::presentation::style::COMPONENT_READY
                } else {
                    crate::presentation::style::COMPONENT_EMPTY
                },
            ),
            #[cfg(test)]
            Self::ComponentGlow(r) => {
                canvas
                    .commands
                    .push(crate::presentation::style::component_glow(
                        canvas.bounds(*r),
                        ui.scale,
                    ));
            }
            Self::Progress(r) => canvas.commands.push(
                retinue_ui::scene::LoadingFrame::sample(ui.animation_elapsed, false)
                    .progress(canvas.bounds(*r), Color::from_bgra(ACCENT)),
            ),
            Self::Field(..) => return None,
        }
        Some(canvas.commands)
    }
}

fn logical(r: Rect, scale: f32) -> Bounds {
    Bounds::new(
        r.x as f32 / scale,
        r.y as f32 / scale,
        r.width as f32 / scale,
        r.height as f32 / scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq)]
    struct BoundTree {
        visible: retinue_ui::state::State<bool>,
        controls: bool,
    }
    impl retinue_ui::view::View for BoundTree {
        fn body(
            &self,
            _: &mut retinue_ui::view::ViewContext<'_>,
        ) -> impl retinue_ui::view::IntoElement {
            use crate::{
                presentation::widgets::fill, presentation::widgets::place,
                presentation::widgets::slot,
            };
            use retinue_ui::prelude::*;
            retinue_ui::ui! {
                ZStack() {
                    fill(PANEL, 0.0)
                    if self.controls {
                        if self.visible.get() {
                            place(slot(ElementKey::new("test.controls", 0)), Bounds::new(40.0, 50.0, 220.0, 180.0))
                        }
                    } else {
                        place(slot(ElementKey::new("test.control", 0)), Bounds::new(20.0, 20.0, 200.0, 80.0))
                        place(slot(ElementKey::new("test.icon", 0)), Bounds::new(240.0, 20.0, 32.0, 32.0))
                        if self.visible.get() { empty().frame(1.0, 1.0) }
                    }
                }.alignment(Alignment::Start)
            }
        }
    }

    #[test]
    fn bound_paint_survives_unrelated_tree_changes() {
        use retinue_render::thumbnails::Thumbnails;
        use retinue_ui::{prelude::FrameTime, prelude::State};
        let control = ElementKey::new("test.control", 0);
        let icon = ElementKey::new("test.icon", 0);
        for scale in [1.0, 1.25, 2.0] {
            let branch = State::new(false);
            let mut ui = ControlHost::new().unwrap();
            ui.install_view(BoundTree {
                visible: branch.clone(),
                controls: false,
            });
            let mut media = Media::default();
            let mut thumbnails = Thumbnails::new(0, 0);
            let mut expected: Option<Vec<u8>> = None;
            for selected in [false, true, false, true] {
                branch.set(selected);
                ui.begin((700, 500), scale);
                ui.update_view(FrameTime::now());
                ui.view_control(control, |ui, r| {
                    ui.fill(r, 4.0, CARD);
                    ui.icon(ui.rect(10.0, 10.0, 40.0, 40.0), "mastered");
                });
                ui.view_icon(icon, "owned", None);
                ui.fill(ui.rect(20.0, 120.0, 100.0, 30.0), 4.0, GOOD);
                ui.finish();
                let requests = ui.requests(&media);
                assert_eq!(requests.len(), 2);
                let ready = requests
                    .iter()
                    .map(|r| (r.key, thumbnails.prepare(r).unwrap()))
                    .collect::<Vec<_>>();
                media.accept(&ready, &requests);
                let pixels = render(&ui, &media);
                if let Some(expected) = &expected {
                    let maximum = pixels
                        .iter()
                        .zip(expected)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max();
                    assert_eq!(maximum, Some(0), "scale={scale}, branch={selected}");
                } else {
                    expected = Some(pixels);
                }
            }
        }
    }

    fn editor_controls(ui: &mut ControlHost, _: Rect) {
        ui.number_field(
            "amount",
            ui.rect(8.0, 8.0, 180.0, 30.0),
            "42",
            "Quantity",
            1,
            100,
        );
        ui.begin_scroll(ui.rect(8.0, 50.0, 200.0, 100.0), ui.px(20.0));
        ui.fill(ui.rect(0.0, 0.0, 220.0, 160.0), 4.0, CARD);
        ui.field(
            "nested",
            ui.rect(4.0, 12.0, 180.0, 30.0),
            "Local editor",
            "Text",
        );
        ui.end_scroll();
        ui.choice(
            "choice",
            ui.rect(8.0, 155.0, 180.0, 25.0),
            "a",
            &[("a", "First"), ("b", "Second")],
        );
    }

    #[test]
    fn scrolling_does_not_rerecord_viewport_clipped_slot_content() {
        use retinue_ui::prelude::*;
        const SLOT: ElementKey = ElementKey::new("test.scroll.content", 0);
        #[derive(PartialEq)]
        struct Scrolling(State<f32>);
        impl View for Scrolling {
            fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
                crate::presentation::widgets::place(
                    ScrollViewport::new(
                        crate::presentation::widgets::slot(SLOT).frame(100.0, 160.0),
                    )
                    .position(0.0, self.0.get()),
                    Bounds::new(20.0, 32.0, 100.0, 80.0),
                )
            }
        }
        fn content(ui: &mut ControlHost, r: Rect) {
            ui.fill(r, 8.0, CARD);
            ui.fill(ui.rect(20.0, 60.0, 30.0, 30.0), 5.0, GOOD);
        }
        for scale in [1.0, 1.25, 2.0] {
            let offset = State::new(0.0);
            let mut ui = ControlHost::new().unwrap();
            ui.install_view(Scrolling(offset.clone()));
            let media = Media::default();
            for (step, y) in [0.0, 8.0, 32.0, 60.0, 12.0].into_iter().enumerate() {
                offset.set(y);
                ui.begin((700, 500), scale);
                ui.update_view(FrameTime::now());
                ui.view_control(SLOT, content);
                ui.finish();
                let pixels = render(&ui, &media);
                if step > 0 {
                    assert_eq!(
                        ui.composition.borrow().scene().stats.recordings,
                        0,
                        "static paint was clipped into its source at scale={scale}, y={y}"
                    );
                }
                let mut reference = ControlHost::new().unwrap();
                reference.begin((700, 500), scale);
                reference.begin_scroll(reference.rect(20.0, 32.0, 100.0, 80.0), reference.px(y));
                let rect = reference.rect(0.0, 0.0, 100.0, 160.0);
                content(&mut reference, rect);
                reference.end_scroll();
                reference.finish();
                assert!(
                    pixels
                        .iter()
                        .zip(render(&reference, &media))
                        .all(|(a, b)| a.abs_diff(b) <= 1),
                    "scroll clip changed pixels at scale={scale}, y={y}"
                );
            }
        }
    }

    #[test]
    fn declared_controls_preserve_editor_paint_input_and_nested_clips() {
        use retinue_ui::{prelude::FrameTime, prelude::State};
        let id = ElementKey::new("test.controls", 0);
        for scale in [1.0, 1.25, 2.0] {
            let mut declared = ControlHost::new().unwrap();
            let mut legacy = ControlHost::new().unwrap();
            let visible = State::new(true);
            declared.install_view(BoundTree {
                visible: visible.clone(),
                controls: true,
            });
            for ui in [&mut declared, &mut legacy] {
                ui.begin((700, 500), scale);
            }
            declared.update_view(FrameTime::now());
            declared.view_control(id, editor_controls);
            legacy.fill(
                legacy.rect(0.0, 0.0, 700.0 / scale, 500.0 / scale),
                0.0,
                PANEL,
            );
            legacy.begin_scroll(legacy.rect(40.0, 50.0, 220.0, 180.0), 0);
            editor_controls(&mut legacy, Rect::default());
            legacy.end_scroll();
            for ui in [&mut declared, &mut legacy] {
                ui.finish();
            }
            for key in ["amount", "nested", "choice"] {
                assert_eq!(declared.bounds(key), legacy.bounds(key), "{key} at {scale}");
            }
            let media = Media::default();
            for focused in [false, true] {
                if focused {
                    for ui in [&mut declared, &mut legacy] {
                        let r = ui.bounds("amount").unwrap();
                        ui.pointer_move(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64));
                        ui.pointer_button(true);
                        ui.pointer_button(false);
                        ui.key(Key::Named(NamedKey::Home), ModifiersState::empty());
                        ui.key(Key::Character("7"), ModifiersState::empty());
                    }
                    assert_eq!(
                        declared.fields["amount"].borrow().text(),
                        legacy.fields["amount"].borrow().text()
                    );
                }
                let a = render(&declared, &media);
                let b = render(&legacy, &media);
                let changed = a
                    .chunks_exact(4)
                    .zip(b.chunks_exact(4))
                    .enumerate()
                    .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1))
                    .map(|(i, _)| Rect {
                        x: (i % 700) as u32,
                        y: (i / 700) as u32,
                        width: 1,
                        height: 1,
                    })
                    .reduce(Rect::union);
                assert!(
                    changed.is_none(),
                    "scale={scale}, focused={focused}, changed={changed:?}"
                );
            }
            visible.set(false);
            declared.begin((700, 500), scale);
            declared.update_view(FrameTime::now());
            declared.view_control(id, |_, _| panic!("hidden control was bound"));
            declared.finish();
            assert!(declared.bounds("amount").is_none());
            assert!(declared.focus.is_none());
            render(&declared, &media);
            assert!(
                !declared
                    .composition
                    .borrow()
                    .scene()
                    .frame()
                    .has_external_paint()
            );
        }
    }

    #[derive(PartialEq)]
    struct CachedSlot(ElementKey);

    impl retinue_ui::view::Primitive for CachedSlot {
        fn resolve(
            &self,
            cx: &mut retinue_ui::view::ViewContext<'_>,
        ) -> retinue_ui::view::PrimitiveOutput {
            let mut output = super::super::view_host::PaintSlot(self.0).resolve(cx);
            output.cache = CachePolicy::Prefer;
            output
        }
    }

    #[derive(PartialEq)]
    struct CardsView {
        scale: f32,
        offset: u32,
    }

    impl retinue_ui::view::View for CardsView {
        fn body(
            &self,
            _: &mut retinue_ui::view::ViewContext<'_>,
        ) -> impl retinue_ui::view::IntoElement {
            use crate::{presentation::widgets::fill, presentation::widgets::place};
            use retinue_ui::prelude::*;
            retinue_ui::ui! {
                ZStack() {
                    fill(PANEL, 0.0)
                    place(ScrollViewport::new(retinue_ui::ui! {
                        ZStack() {
                            for i in 0..2 {
                                place(Element::primitive(CachedSlot(ElementKey::new("test.card", i)))
                                    .id(ElementKey::new("test.card", i)),
                                    Bounds::new(4.0, 7.0 + i as f32 * 74.0, 230.0, 68.0))
                            }
                        }.alignment(Alignment::Start).frame(260.0, 160.0)
                    }).position(0.0, self.offset as f32 / self.scale),
                        Bounds::new(20.0, 30.0, 260.0, 140.0))
                }.alignment(Alignment::Start)
            }
        }
    }

    fn layout(ui: &mut ControlHost, scale: f32, offset: u32, popup: bool, changed: bool) {
        ui.begin((700, 500), scale);
        ui.composition
            .get_mut()
            .host
            .set_view(CardsView { scale, offset });
        ui.update_view(retinue_ui::timeline::FrameTime::now());
        for i in 0..2 {
            ui.view_control(ElementKey::new("test.card", i), |ui, r| {
                ui.fill(r, 8.0, CARD);
                ui.text(
                    ui.rect(6.0, 2.0, 210.0, 24.0),
                    if i == 0 { "Ash" } else { "Mesa" },
                    14.0,
                    TEXT,
                );
                ui.fill(
                    ui.rect(8.0, 32.0, 20.0, 20.0),
                    4.0,
                    if changed && i == 0 { BAD } else { GOOD },
                );
                ui.icon(ui.rect(46.0, 32.0, 20.0, 20.0), "owned");
                ui.hit(format!("card:{i}"), r, "Card", true);
            });
        }
        if popup {
            ui.fill(ui.rect(80.0, 50.0, 100.0, 50.0), 4.0, [30, 20, 10, 180]);
            ui.text(ui.rect(90.0, 55.0, 90.0, 30.0), "Popup", 14.0, TEXT);
        }
        ui.finish();
    }

    fn render(ui: &ControlHost, media: &Media) -> Vec<u8> {
        let mut pixels = vec![0; 700 * 500 * 4];
        let mut painter = Painter::new(&mut pixels, 700, 500).unwrap();
        ui.paint(&mut painter, media);
        painter.finish().unwrap();
        pixels
    }

    #[test]
    fn scroll_rasters_survive_motion_and_overlays_and_repair_changed_children() {
        for scale in [1.0, 1.25, 2.0] {
            let mut cached = ControlHost::new().unwrap();
            let mut replay = ControlHost::new().unwrap();
            replay
                .composition
                .get_mut()
                .renderer
                .get_mut()
                .set_optimizations(false);
            let media = Media::default();
            for (offset, popup, changed, expected) in [
                (0, false, false, None),
                (12, false, false, Some(0)),
                (12, true, false, Some(0)),
                (19, true, true, Some(1)),
                (23, false, true, Some(0)),
            ] {
                layout(&mut cached, scale, offset, popup, changed);
                layout(&mut replay, scale, offset, popup, changed);
                let expected = expected.unwrap_or(2);
                let actual = render(&cached, &media);
                let oracle = render(&replay, &media);
                let difference = actual
                    .iter()
                    .zip(&oracle)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    difference <= 2,
                    "scale={scale} offset={offset} delta={difference}"
                );
                let report = cached.composition.borrow().report();
                assert_eq!(report["rasters"], expected, "{report}");
                assert_eq!(report["blits"], 2);
                assert_eq!(replay.composition.borrow().report()["blits"], 0);
                if changed && expected == 1 {
                    assert!(
                        report["repaired_pixels"].as_u64().unwrap()
                            < (30.0 * 30.0 * scale * scale) as u64,
                        "scale={scale}, repaired={}",
                        report["repaired_pixels"]
                    );
                }
            }
        }
    }

    #[test]
    fn late_scroll_artwork_repairs_cached_content_without_layout() {
        use retinue_render::{image, resources::ResourceCache};
        let mut ui = ControlHost::new().unwrap();
        layout(&mut ui, 1.25, 12, false, false);
        let mut media = Media::default();
        let empty = render(&ui, &media);
        let requests = ui.requests(&media);
        let mut cache = ResourceCache::new(4096);
        let image = cache.insert(
            requests[0].key,
            RasterImage::from_rgba(image::RgbaImage::from_pixel(
                8,
                8,
                image::Rgba([230, 70, 40, 255]),
            ))
            .unwrap(),
        );
        media.accept(&[(requests[0].key, image)], &requests);
        assert_ne!(render(&ui, &media), empty);
        assert_eq!(ui.composition.borrow().report()["layout_computations"], 0);
        media.clear();
        assert_eq!(render(&ui, &media), empty);
        ui.clear_rasters();
        assert_eq!(ui.raster_memory().0, 0);
        render(&ui, &media);
        assert_eq!(ui.composition.borrow().report()["rasters"], 2);
    }
}

impl Visual {
    fn bounds(&self, scale: f32) -> Rect {
        match self {
            Self::Fill(r, ..)
            | Self::Outline(r, ..)
            | Self::Text(r, ..)
            | Self::Image(r, ..)
            | Self::Covered(r, ..)
            | Self::Icon(r, ..)
            | Self::ClippedIcon(r, ..)
            | Self::Texture(r, ..)
            | Self::Field(r, ..)
            | Self::Progress(r) => *r,
            #[cfg(test)]
            Self::CircleImage(r, ..) | Self::ComponentBase(r, ..) | Self::ComponentGlow(r) => *r,
            Self::Line(a, b, _) => {
                let inset = scale.ceil() as u32 + 1;
                let x = a.0.min(b.0).saturating_sub(inset);
                let y = a.1.min(b.1).saturating_sub(inset);
                Rect {
                    x,
                    y,
                    width: a.0.max(b.0) + inset - x,
                    height: a.1.max(b.1) + inset - y,
                }
            }
            Self::Polygon(points, _) => {
                let left = points
                    .iter()
                    .map(|p| p[0])
                    .fold(f64::INFINITY, f64::min)
                    .floor()
                    .max(0.0) as u32;
                let top = points
                    .iter()
                    .map(|p| p[1])
                    .fold(f64::INFINITY, f64::min)
                    .floor()
                    .max(0.0) as u32;
                let right = points
                    .iter()
                    .map(|p| p[0])
                    .fold(0.0, f64::max)
                    .ceil()
                    .max(0.0) as u32;
                let bottom = points
                    .iter()
                    .map(|p| p[1])
                    .fold(0.0, f64::max)
                    .ceil()
                    .max(0.0) as u32;
                Rect {
                    x: left,
                    y: top,
                    width: right.saturating_sub(left),
                    height: bottom.saturating_sub(top),
                }
            }
        }
    }
}
