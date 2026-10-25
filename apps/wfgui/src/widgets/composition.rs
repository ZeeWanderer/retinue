use super::*;
use retinue_ui::composition::{
    Bounds, CachePolicy, Canvas, Change, Color, Command, Element, ImageFit, Key as ElementKey,
    Paint, PaintKey, RasterMode, RecordingMode, Renderer, Scene,
};

const OVERLAYS: ElementKey = ElementKey::new("page.overlays", 0);

pub(super) struct Composition {
    scene: Scene,
    renderer: RefCell<Renderer>,
    targets: HashMap<ElementKey, usize>,
    controls: HashMap<ElementKey, String>,
    primitives: HashMap<ElementKey, (Rc<Vec<Command>>, Paint)>,
    groups: Vec<(ElementKey, Vec<(ElementKey, (f32, f32))>)>,
    revision: u64,
}

impl Default for Composition {
    fn default() -> Self {
        Self {
            scene: Scene::default(),
            renderer: RefCell::new(Renderer::new(16 * 1024 * 1024)),
            targets: HashMap::new(),
            controls: HashMap::new(),
            primitives: HashMap::new(),
            groups: Vec::new(),
            revision: 0,
        }
    }
}

impl Composition {
    pub fn clear_cache(&mut self) {
        self.renderer.get_mut().clear_cache();
    }

    pub fn cache_stats(&self) -> retinue_ui::resources::CacheStats {
        self.renderer.borrow().cache_stats()
    }

    pub fn update(&mut self, ui: &Ui) {
        let mut root = Element::at(
            ElementKey::new("page", 0),
            Bounds::new(
                0.0,
                0.0,
                ui.size.0 as f32 / ui.scale,
                ui.size.1 as f32 / ui.scale,
            ),
        );
        root.style.position = retinue_ui::taffy::Position::Relative;
        self.targets.clear();
        self.controls.clear();
        self.groups.clear();
        let mut viewport_group = None;
        let mut previous_viewport = None;
        let mut groups = 0;
        let mut retained_primitives = std::collections::HashSet::new();
        let mut index = 0;
        while index < ui.visuals.len() {
            let (draw, viewport) = &ui.visuals[index];
            let mut end = index + 1;
            if ui.visual_groups[index].is_some() {
                while end < ui.visuals.len()
                    && ui.visual_groups[end] == ui.visual_groups[index]
                    && ui.visuals[end].1 == *viewport
                {
                    end += 1;
                }
                if ui.visuals[index..end].iter().any(|(draw, _)| draw.live()) {
                    end = index + 1;
                }
            }
            if previous_viewport != Some(*viewport) {
                previous_viewport = Some(*viewport);
                viewport_group = viewport.map(|(clip, _)| {
                    let mut group = Element::at(
                        ElementKey::new("page.clip", groups),
                        logical(clip, ui.scale),
                    );
                    group.clip = true;
                    groups += 1;
                    root.children.push(group);
                    root.children.len() - 1
                });
            }
            let bounds = ui.visuals[index..end]
                .iter()
                .map(|(draw, _)| draw.bounds(ui.scale))
                .reduce(Rect::union)
                .unwrap();
            if Ui::project_rect(bounds, *viewport).is_none() {
                index = end;
                continue;
            }
            let grouped = end > index + 1;
            let key = if grouped {
                ElementKey::new(
                    "page.scroll.item",
                    identity_hash((ui.visual_groups[index], ui.visual_keys[index])),
                )
            } else {
                ui.visual_keys[index]
            };
            let mut position = logical(bounds, ui.scale);
            if let Some((clip, (x, y))) = viewport {
                position.x = (bounds.x as f32 + *x as f32 - clip.x as f32) / ui.scale;
                position.y = (bounds.y as f32 + *y as f32 - clip.y as f32) / ui.scale;
            }
            let mut node = Element::at(key, position);
            node.paint = Some(if grouped {
                let mut members = Vec::new();
                for i in index..end {
                    let draw = &ui.visuals[i].0;
                    let member = ui.visual_keys[i];
                    retained_primitives.insert(member);
                    if let Some(commands) = draw.commands(ui, None) {
                        self.record(member, commands, CachePolicy::Never);
                    } else if !self.primitives.contains_key(&member) {
                        self.record(member, Vec::new(), CachePolicy::Never);
                    }
                    let b = draw.bounds(ui.scale);
                    members.push((
                        member,
                        (
                            (b.x as f32 - bounds.x as f32) / ui.scale,
                            (b.y as f32 - bounds.y as f32) / ui.scale,
                        ),
                    ));
                }
                retained_primitives.insert(key);
                self.record_group(key, &members);
                self.groups.push((key, members));
                self.primitives[&key].1.clone()
            } else if !draw.live() {
                retained_primitives.insert(key);
                if let Some(commands) = draw.commands(ui, None) {
                    self.record(key, commands, CachePolicy::Never);
                } else if !self.primitives.contains_key(&key) {
                    self.record(key, Vec::new(), CachePolicy::Never);
                }
                self.primitives[&key].1.clone()
            } else {
                self.targets.insert(key, index);
                Paint::external(key)
            });
            if let Some(group) = viewport_group {
                root.children[group].children.push(node);
            } else {
                root.children.push(node);
            }
            index = end;
        }
        retained_primitives.insert(OVERLAYS);
        self.primitives
            .retain(|key, _| retained_primitives.contains(key));
        let mut occurrences = HashMap::<&str, u64>::new();
        for button in ui.active_buttons() {
            let occurrence = occurrences.entry(&button.key).or_default();
            let key = ElementKey::new("page.control", access_id(*occurrence, &button.key).0);
            *occurrence += 1;
            let mut node = Element::at(key, logical(button.rect, ui.scale));
            node.interactive = true;
            self.controls.insert(key, button.key.clone());
            root.children.push(node);
        }
        if !self.primitives.contains_key(&OVERLAYS) {
            self.record(OVERLAYS, Vec::new(), CachePolicy::Never);
        }
        let mut overlays = Element::at(
            OVERLAYS,
            Bounds::new(
                0.0,
                0.0,
                ui.size.0 as f32 / ui.scale,
                ui.size.1 as f32 / ui.scale,
            ),
        );
        overlays.paint = Some(self.primitives[&OVERLAYS].1.clone());
        root.children.push(overlays);
        self.scene
            .update(root, ui.size, ui.scale, RecordingMode::Retained)
            .expect("unique page composition identities");
    }

    fn record_group(
        &mut self,
        key: ElementKey,
        members: &[(ElementKey, (f32, f32))],
    ) -> Option<Paint> {
        let commands = members
            .iter()
            .map(|(key, offset)| Command::Translate {
                offset: *offset,
                commands: self.primitives[key].0.clone(),
            })
            .collect();
        self.record(key, commands, CachePolicy::Prefer)
    }

    fn record(
        &mut self,
        key: ElementKey,
        commands: Vec<Command>,
        cache: CachePolicy,
    ) -> Option<Paint> {
        if self
            .primitives
            .get(&key)
            .is_some_and(|(old, paint)| **old == commands && paint.cache == cache)
        {
            return None;
        }
        self.revision += 1;
        let commands = Rc::new(commands);
        let retained = commands.clone();
        let mut paint = Paint::new(PaintKey::new(key, self.revision), move |_| {
            retained.as_ref().clone()
        });
        paint.cache = cache;
        self.primitives.insert(key, (commands, paint.clone()));
        Some(paint)
    }

    pub fn paint(&mut self, ui: &Ui, p: &mut Painter<'_>, media: &Media) {
        let mut changes = Vec::new();
        let mut canvas = Canvas::new(ui.scale);
        ui.paint_overlays(&mut canvas);
        if let Some(paint) = self.record(OVERLAYS, canvas.commands, CachePolicy::Never) {
            changes.push(Change::Paint(OVERLAYS, Some(paint)));
        }
        for (index, (draw, _)) in ui.visuals.iter().enumerate() {
            let key = ui.visual_keys[index];
            if !self.primitives.contains_key(&key) || !draw.media() {
                continue;
            }
            if let Some(commands) = draw.commands(ui, Some(media))
                && let Some(paint) = self.record(key, commands, CachePolicy::Never)
            {
                if self.scene.bounds(key).is_some() {
                    changes.push(Change::Paint(key, Some(paint)));
                }
            }
        }
        let groups = std::mem::take(&mut self.groups);
        for (key, members) in &groups {
            if let Some(paint) = self.record_group(*key, members) {
                changes.push(Change::Paint(*key, Some(paint)));
            }
        }
        self.groups = groups;
        if !changes.is_empty() {
            self.scene.patch(changes).expect("page resource bindings");
        }
        self.renderer
            .get_mut()
            .prepare(&self.scene, RasterMode::Explicit);
        self.renderer.borrow_mut().draw_with(
            &self.scene,
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
                let node = self.scene.bounds(key).unwrap();
                let (draw, viewport) = &ui.visuals[index];
                let origin = (
                    (node.layout.x * ui.scale).round() as i32,
                    (node.layout.y * ui.scale).round() as i32,
                );
                p.translated((-origin.0, -origin.1), |p| {
                    if let Some((clip, offset)) = viewport {
                        p.viewport(*clip, *offset, |p| ui.paint_live(p, draw));
                    } else {
                        ui.paint_live(p, draw);
                    }
                });
            },
        );
    }

    pub fn hit(&self, point: (f64, f64)) -> Option<String> {
        self.scene
            .hit_test(point)
            .and_then(|key| self.controls.get(&key).cloned())
    }

    pub fn report(&self) -> serde_json::Value {
        let renderer = self.renderer.borrow();
        serde_json::json!({"nodes":self.scene.stats.nodes,"chunks":self.scene.stats.visible_chunks,"layout_computations":self.scene.stats.layout_computations,"recordings":self.scene.stats.recordings,
            "rasters":renderer.stats.prepared_rasters,"blits":renderer.stats.blits,"repaired_pixels":renderer.stats.repaired_pixels,"raster_bytes":renderer.cache_stats().live_bytes,
            "geometry":self.scene.geometry().iter().map(|node|serde_json::json!({
                "scope":node.key.namespace,"id":node.key.value,"parent":node.parent.map(|key|(key.namespace,key.value)),
                "layout":[node.layout.x,node.layout.y,node.layout.width,node.layout.height],
                "hit":[node.hit.x,node.hit.y,node.hit.width,node.hit.height],
                "paint":[node.paint.x,node.paint.y,node.paint.width,node.paint.height],
                "interactive":node.interactive,
            })).collect::<Vec<_>>()})
    }
}

impl Visual {
    fn live(&self) -> bool {
        matches!(self, Self::Field(..) | Self::Progress(..))
    }

    fn media(&self) -> bool {
        matches!(
            self,
            Self::Image(..)
                | Self::CircleImage(..)
                | Self::Covered(..)
                | Self::Icon(..)
                | Self::ClippedIcon(..)
                | Self::Texture(..)
        )
    }

    fn commands(&self, ui: &Ui, media: Option<&Media>) -> Option<Vec<Command>> {
        let bounds = self.bounds(ui.scale);
        let mut canvas = Canvas::at((bounds.x, bounds.y), ui.scale);
        match self {
            Self::Commands(_, commands) => return Some(commands.clone()),
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
            Self::Image(r, asset) | Self::CircleImage(r, asset) | Self::Covered(r, asset, _) => {
                let (edge, fit) = match self {
                    Self::Covered(_, _, edge) => (*edge, ImageFit::Cover),
                    Self::CircleImage(..) => (r.width.max(r.height), ImageFit::Circle),
                    _ => (r.width.max(r.height), ImageFit::Contain),
                };
                if let Some(image) = media?.image_resource(asset, edge) {
                    canvas.image(*r, image.clone(), fit);
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
            Self::ComponentBase(r, ready) => canvas.fill(
                *r,
                r.width / 2,
                if *ready {
                    crate::style::COMPONENT_READY
                } else {
                    crate::style::COMPONENT_EMPTY
                },
            ),
            Self::ComponentGlow(r) => {
                canvas
                    .commands
                    .push(if let Some(image) = ui.component_glows.get(&r.width) {
                        Command::Image {
                            bounds: canvas.bounds(*r),
                            image: image.clone().into(),
                            fit: ImageFit::Stretch,
                        }
                    } else {
                        Command::InsetShadow {
                            bounds: canvas.bounds(*r),
                            radius: (r.width / 2) as f32 / ui.scale,
                            blur: 4.0,
                            spread: 1.0,
                            color: Color::from_bgra(crate::style::COMPONENT_GLOW),
                        }
                    })
            }
            Self::Field(..) | Self::Progress(..) => return None,
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

    fn layout(ui: &mut Ui, scale: f32, offset: u32, popup: bool, changed: bool) {
        ui.begin((700, 500), scale);
        ui.fill(
            Rect {
                width: 700,
                height: 500,
                ..Rect::default()
            },
            0.0,
            PANEL,
        );
        ui.begin_scroll(ui.rect(20.0, 30.0, 260.0, 140.0), offset);
        for i in 0..2 {
            ui.begin_scope(i);
            let y = 7.0 + i as f32 * 74.0;
            ui.fill(ui.rect(4.0, y, 230.0, 68.0), 8.0, CARD);
            ui.text(
                ui.rect(10.0, y + 2.0, 210.0, 24.0),
                "Retained card",
                14.0,
                TEXT,
            );
            ui.fill(
                ui.rect(12.0, y + 32.0, 20.0, 20.0),
                4.0,
                if changed && i == 0 { BAD } else { GOOD },
            );
            ui.icon(ui.rect(50.0, y + 32.0, 20.0, 20.0), "owned");
            ui.hit(
                format!("card:{i}"),
                ui.rect(4.0, y, 230.0, 68.0),
                "Card",
                true,
            );
            ui.end_scope();
        }
        ui.end_scroll();
        if popup {
            ui.fill(ui.rect(80.0, 50.0, 100.0, 50.0), 4.0, [30, 20, 10, 180]);
            ui.text(ui.rect(90.0, 55.0, 90.0, 30.0), "Popup", 14.0, TEXT);
        }
        ui.finish();
    }

    fn render(ui: &Ui, media: &Media) -> Vec<u8> {
        let mut pixels = vec![0; 700 * 500 * 4];
        let mut painter = Painter::new(&mut pixels, 700, 500).unwrap();
        ui.paint(&mut painter, media);
        painter.finish().unwrap();
        pixels
    }

    #[test]
    fn scroll_rasters_survive_motion_and_overlays_and_repair_changed_children() {
        for scale in [1.0, 1.25, 2.0] {
            let mut cached = Ui::new().unwrap();
            let mut replay = Ui::new().unwrap();
            replay.composition.get_mut().renderer = RefCell::new(Renderer::new(0));
            let media = Media::default();
            for (offset, popup, changed, expected) in [
                (0, false, false, 2),
                (12, false, false, 0),
                (12, true, false, 0),
                (19, true, true, 1),
                (23, false, true, 0),
            ] {
                layout(&mut cached, scale, offset, popup, changed);
                layout(&mut replay, scale, offset, popup, changed);
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
                if changed && expected == 1 {
                    assert!(
                        report["repaired_pixels"].as_u64().unwrap()
                            < (30.0 * 30.0 * scale * scale) as u64
                    );
                }
            }
        }
    }

    #[test]
    fn late_scroll_artwork_repairs_cached_content_without_layout() {
        use retinue_ui::{image, resources::ResourceCache};
        let mut ui = Ui::new().unwrap();
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
            | Self::CircleImage(r, ..)
            | Self::ComponentBase(r, ..)
            | Self::ComponentGlow(r)
            | Self::Covered(r, ..)
            | Self::Icon(r, ..)
            | Self::ClippedIcon(r, ..)
            | Self::Texture(r, ..)
            | Self::Field(r, ..)
            | Self::Commands(r, ..)
            | Self::Progress(r) => *r,
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
