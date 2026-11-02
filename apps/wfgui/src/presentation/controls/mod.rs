use crate::{
    data::foundry::Asset, domain::media, domain::media::Media, presentation::settings::rgb,
};
#[cfg(test)]
use retinue_render::painter::RasterImage;
use retinue_render::{
    painter::{CachedFont, GlyphCache, Painter, TextLine, text_width},
    thumbnails::Request,
};
use retinue_ui::{
    Rect, desktop::winit::keyboard::Key, desktop::winit::keyboard::ModifiersState,
    desktop::winit::keyboard::NamedKey, desktop::winit::window::CursorIcon,
    focus_visible::FocusVisible, input::ControlRegion, input::Controls, input::FocusPolicy,
    input::HitShape, input::PointerPhase, line_edit::Colors, line_edit::LineEdit,
};
use std::{
    cell::RefCell, collections::HashMap, hash::Hash, hash::Hasher, rc::Rc, time::Duration,
    time::Instant,
};

#[cfg(test)]
mod control_tests;
mod render_host;
mod report;
pub(crate) mod view_host;

pub use crate::presentation::style::{ACCENT, BAD, CARD, GOOD, MUTED, PANEL, POPOVER, TEXT};

#[derive(Clone, Copy, PartialEq)]
pub enum Weight {
    Light = 300,
    Regular = 400,
    Medium = 500,
    Bold = 700,
}

pub enum Visual {
    Fill(Rect, f32, [u8; 4]),
    Outline(Rect, f32, [u8; 4]),
    Text(Rect, String, f32, [u8; 4], Weight),
    Line((u32, u32), (u32, u32), [u8; 4]),
    Polygon(Vec<[f64; 2]>, [u8; 4]),
    Image(Rect, Asset),
    #[cfg(test)]
    CircleImage(Rect, Asset),
    #[cfg(test)]
    ComponentBase(Rect, bool),
    #[cfg(test)]
    ComponentGlow(Rect),
    Covered(Rect, Asset, u32),
    Icon(Rect, Request),
    ClippedIcon(Rect, Rect, Request),
    Texture(Rect, Request, bool),
    Field(Rect, String, String),
    Progress(Rect),
}
pub struct Button {
    pub key: String,
    pub rect: Rect,
    pub label: String,
    pub enabled: bool,
    pub field: bool,
    binding: Option<(retinue_ui::Key, Option<retinue_ui::Bounds>)>,
    shape: HitShape,
}
#[derive(Clone, Debug)]
pub struct Signal {
    pub key: String,
    pub value: Option<String>,
}
type ControlId = (String, Option<retinue_ui::Key>, usize);
struct Search {
    motion: crate::presentation::widgets::search::Motion,
    style: crate::presentation::widgets::search::Style,
    width: f32,
}
impl Search {
    fn update(&mut self, active: bool, content_width: f32, now: Instant, animate: bool) -> bool {
        use crate::presentation::widgets::search::Style;
        match self.style {
            Style::Capsule(_) => self.motion.update(active, content_width, now, animate),
            Style::Fill(_) => self.motion.fill(active, self.width, now, animate),
            Style::Field => self.motion.fill(true, self.width, now, false),
        }
    }
}
pub struct ControlHost {
    pub regular: Rc<CachedFont>,
    bold: Rc<CachedFont>,
    light: Rc<CachedFont>,
    medium: Rc<CachedFont>,
    pub scale: f32,
    pub size: (u32, u32),
    visuals: Vec<(Visual, Option<(Rect, (i32, i32))>)>,
    viewport: Option<(Rect, (i32, i32))>,
    viewports: Vec<Option<(Rect, (i32, i32))>>,
    pub buttons: Vec<Button>,
    fields: HashMap<String, RefCell<LineEdit>>,
    edit_resources: retinue_ui::line_edit::EditResources,
    searches: HashMap<String, Search>,
    pub focus: Option<String>,
    pub hover: Option<String>,
    pressed: Option<String>,
    input: Controls<ControlId>,
    input_changed: bool,
    building: bool,
    #[cfg(all(test, feature = "profiling"))]
    pub(crate) input_profile: Duration,
    pointer: (f64, f64),
    pub signals: Vec<Signal>,
    pub focus_visible: FocusVisible,
    hover_since: Instant,
    animation_start: Instant,
    animation_elapsed: Duration,
    pub choices: HashMap<String, Vec<(String, String)>>,
    popup: Option<(String, usize)>,
    popup_input: Controls<usize>,
    context_keys: Vec<String>,
    context_close: String,
    context_invoker: Option<String>,
    pub clipboard: Option<(String, Instant, retinue_ui::desktop::clipboard::Operation)>,
    cut: Option<(String, Instant)>,
    embedded: HashMap<(usize, u32), Request>,
    selected_choices: HashMap<String, usize>,
    modal_start: usize,
    tooltip: bool,
    scrollbars: HashMap<String, (Rect, Rect, f32)>,
    scroll_grab: f32,
    regions: HashMap<String, Rect>,
    numbers: HashMap<String, (u32, u32)>,
    toggles: HashMap<String, bool>,
    composition: RefCell<render_host::Composition>,
}

impl ControlHost {
    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.composition.get_mut().set_render_host(host);
    }

    #[cfg(test)]
    pub fn new() -> Result<Self, String> {
        Self::with_faces(&Default::default())
    }
    pub fn with_faces(faces: &retinue_render::painter::FontFaces) -> Result<Self, String> {
        let glyphs = GlyphCache::new(4 * 1024 * 1024);
        Ok(Self {
            regular: Rc::new(CachedFont::with_faces(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/roboto/Roboto-Regular.ttf"
                )),
                Default::default(),
                &glyphs,
                faces,
            )?),
            bold: Rc::new(CachedFont::with_faces(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/roboto/Roboto-Bold.ttf"
                )),
                Default::default(),
                &glyphs,
                faces,
            )?),
            light: Rc::new(CachedFont::with_faces(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/roboto/Roboto-Light.ttf"
                )),
                Default::default(),
                &glyphs,
                faces,
            )?),
            medium: Rc::new(CachedFont::with_faces(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/roboto/Roboto-Medium.ttf"
                )),
                Default::default(),
                &glyphs,
                faces,
            )?),
            scale: 1.0,
            size: (0, 0),
            visuals: vec![],
            viewport: None,
            viewports: Vec::new(),
            buttons: vec![],
            fields: HashMap::new(),
            edit_resources: retinue_ui::line_edit::EditResources::new(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../assets/fonts/roboto/Roboto-Regular.ttf"
                )),
                "Roboto",
            ),
            searches: HashMap::new(),
            focus: None,
            hover: None,
            pressed: None,
            input: Controls::default(),
            input_changed: false,
            building: false,
            #[cfg(all(test, feature = "profiling"))]
            input_profile: Duration::ZERO,
            pointer: (-1.0, -1.0),
            signals: vec![],
            focus_visible: Default::default(),
            hover_since: Instant::now(),
            animation_start: Instant::now(),
            animation_elapsed: Duration::ZERO,
            choices: HashMap::new(),
            popup: None,
            popup_input: Controls::default(),
            context_keys: vec![],
            context_close: String::new(),
            context_invoker: None,
            clipboard: None,
            cut: None,
            embedded: HashMap::new(),
            selected_choices: HashMap::new(),
            modal_start: 0,
            tooltip: false,
            scrollbars: HashMap::new(),
            scroll_grab: 0.0,
            regions: HashMap::new(),
            numbers: HashMap::new(),
            toggles: HashMap::new(),
            composition: RefCell::new(render_host::Composition::default()),
        })
    }
    pub fn begin(&mut self, size: (u32, u32), scale: f32) {
        #[cfg(all(test, feature = "profiling"))]
        {
            self.input_profile = Duration::ZERO;
        }
        self.building = true;
        self.composition.get_mut().begin_bindings();
        if size.0 == 0 || size.1 == 0 {
            self.clear_rasters();
        }
        self.size = size;
        self.set_scale(scale);
        self.visuals.clear();
        self.viewport = None;
        self.viewports.clear();
        self.buttons.clear();
        self.choices.clear();
        self.selected_choices.clear();
        self.modal_start = 0;
        self.scrollbars.clear();
        self.regions.clear();
        self.numbers.clear();
        self.toggles.clear();
        self.context_keys.clear();
    }
    pub fn set_scale(&mut self, scale: f32) {
        if scale != self.scale {
            self.edit_resources.clear_glyphs();
        }
        self.scale = scale;
    }

    pub fn clear_rasters(&mut self) {
        self.composition.get_mut().clear_cache();
    }

    pub fn raster_memory(&self) -> (usize, usize) {
        let stats = self.composition.borrow().cache_stats();
        (stats.live_bytes, stats.peak_limit_bytes)
    }
    pub fn begin_modal(&mut self) {
        self.modal_start = self.buttons.len();
        self.tooltip = false;
        if self
            .popup
            .as_ref()
            .is_some_and(|(key, _)| self.buttons.iter().any(|b| &b.key == key))
        {
            self.popup = None;
        }
    }
    pub fn modal_open(&self) -> bool {
        self.modal_start > 0
    }
    pub fn finish(&mut self) {
        assert!(self.viewports.is_empty(), "unclosed UI viewport scope");
        if self.context_invoker.is_some() && !self.regions.contains_key("page.menu") {
            self.close_context();
        }
        if self
            .popup
            .as_ref()
            .is_some_and(|(key, _)| !self.active_buttons().iter().any(|b| &b.key == key))
        {
            self.popup = None;
        }
        if self.focus.as_ref().is_some_and(|key| {
            !self
                .active_buttons()
                .iter()
                .any(|b| &b.key == key && b.enabled)
        }) {
            self.focus = None;
        }
        if self.pressed.as_ref().is_some_and(|key| {
            !self
                .active_buttons()
                .iter()
                .any(|b| &b.key == key && b.enabled)
        }) {
            self.pressed = None;
        }
        self.fields
            .retain(|key, _| self.buttons.iter().any(|b| b.field && b.key == *key));
        self.composition.borrow_mut().update(self);
        self.building = false;
        self.sync_input();
    }

    fn sync_input(&mut self) {
        if self.building {
            return;
        }
        #[cfg(all(test, feature = "profiling"))]
        let started = Instant::now();
        let mut occurrences = rustc_hash::FxHashMap::default();
        let mut tab_stops = rustc_hash::FxHashSet::default();
        let mut bounds = rustc_hash::FxHashMap::default();
        let composition = self.composition.borrow();
        let regions = self
            .active_buttons()
            .iter()
            .filter_map(|button| {
                let owner = button.binding.map(|(key, _)| key);
                let occurrence = occurrences.entry((&button.key, owner)).or_insert(0);
                let id = (button.key.clone(), owner, *occurrence);
                *occurrence += 1;
                let mut region = if let Some((key, local)) = button.binding {
                    let node = (*bounds
                        .entry(key)
                        .or_insert_with(|| composition.input_bounds(key)))?;
                    let mut region = ControlRegion::geometry(id, node);
                    if let Some(local) = local {
                        region.bounds = retinue_ui::Bounds::new(
                            node.layout.x + local.x,
                            node.layout.y + local.y,
                            local.width,
                            local.height,
                        );
                    }
                    region
                } else {
                    ControlRegion::new(
                        id,
                        retinue_ui::Bounds::new(
                            button.rect.x as f32 / self.scale,
                            button.rect.y as f32 / self.scale,
                            button.rect.width as f32 / self.scale,
                            button.rect.height as f32 / self.scale,
                        ),
                    )
                };
                region.enabled = button.enabled;
                region.input.shape = button.shape;
                region.input.focus = if tab_stops.insert(&button.key) {
                    if button.field {
                        FocusPolicy::Text
                    } else {
                        FocusPolicy::Tab
                    }
                } else {
                    FocusPolicy::Programmatic
                };
                Some(region)
            })
            .collect::<Vec<_>>();
        drop(composition);
        self.input.update(regions);
        self.pressed = self.input.captured().map(|id| id.0.clone());
        self.input_changed |= self.refresh_hover();
        #[cfg(all(test, feature = "profiling"))]
        {
            self.input_profile += started.elapsed();
        }
    }

    fn refresh_hover(&mut self) -> bool {
        let hover = self.input.hit(self.input_position()).map(|id| id.0.clone());
        if hover == self.hover {
            return false;
        }
        self.hover = hover;
        self.hover_since = Instant::now();
        self.tooltip = false;
        true
    }

    fn input_position(&self) -> (f32, f32) {
        (
            self.pointer.0 as f32 / self.scale,
            self.pointer.1 as f32 / self.scale,
        )
    }
    fn active_buttons(&self) -> &[Button] {
        &self.buttons[self.modal_start..]
    }
    pub fn has_modal(&self) -> bool {
        self.modal_start > 0
    }
    pub fn popup_open(&self) -> bool {
        self.popup.is_some()
    }
    pub fn dismiss_popup_at(&mut self, point: (f64, f64)) -> bool {
        if self.popup.is_some()
            && !self
                .popup_bounds()
                .is_some_and(|r| r.contains(point.0, point.1))
        {
            self.popup = None;
            self.popup_input.cancel();
            self.pressed = None;
            self.input.cancel();
            return true;
        }
        false
    }
    pub fn cursor(&self) -> CursorIcon {
        match self
            .active_buttons()
            .iter()
            .find(|b| Some(&b.key) == self.hover.as_ref())
        {
            Some(b) if b.field => CursorIcon::Text,
            Some(b) if b.enabled => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }
    pub fn pointer(&self) -> (f64, f64) {
        self.pointer
    }
    pub fn region(&mut self, key: &str, rect: Rect) {
        if let Some(rect) = self.screen_rect(rect) {
            self.regions.insert(key.into(), rect);
        }
    }
    pub fn first_region(&mut self, key: &str, rect: Rect) {
        if !self.regions.contains_key(key) {
            self.region(key, rect);
        }
    }
    pub fn layout_report(&self) -> serde_json::Value {
        use serde_json::json;
        let bounds = |r: Rect| {
            json!({"x":r.x as f32/self.scale, "y":r.y as f32/self.scale,
            "width":r.width as f32/self.scale, "height":r.height as f32/self.scale})
        };
        let color = |c: &[u8; 4]| format!("#{:02x}{:02x}{:02x}{:02x}", c[2], c[1], c[0], c[3]);
        let regions: std::collections::BTreeMap<_, _> = self
            .regions
            .iter()
            .map(|(key, r)| (key, bounds(*r)))
            .collect();
        let mut elements: Vec<_> = self.visuals.iter().filter_map(|(draw, viewport)| {
            let (r, mut value) = match draw {
                Visual::Text(r, text, size, color, weight) => (*r, json!({"kind":"text", "text":text,
                    "font_family":"Roboto", "font_size":size, "font_weight":*weight as u32, "color":format!("#{:02x}{:02x}{:02x}{:02x}",color[2],color[1],color[0],color[3])})),
                Visual::Fill(r, radius, c) => (*r, json!({"kind":"fill", "radius":radius, "color":color(c)})),
                Visual::Outline(r, radius, c) => (*r, json!({"kind":"outline", "radius":radius, "color":color(c)})),
                Visual::Image(r, asset) | Visual::Covered(r, asset, _) => (*r, json!({"kind":"image", "asset":asset.id})),
                #[cfg(test)]
                Visual::CircleImage(r, asset) => (*r, json!({"kind":"image", "asset":asset.id})),
                #[cfg(test)]
                Visual::ComponentBase(r, ready) => (*r, json!({"kind":"component_background", "ready":ready,
                    "color":color(&if *ready {crate::presentation::style::COMPONENT_READY} else {crate::presentation::style::COMPONENT_EMPTY})})),
                #[cfg(test)]
                Visual::ComponentGlow(r) => (*r, json!({"kind":"component_glow", "color":color(&crate::presentation::style::COMPONENT_GLOW)})),
                Visual::Icon(r, _) | Visual::Texture(r, _, _) | Visual::ClippedIcon(r, _, _) => (*r, json!({"kind":"icon"})),
                Visual::Field(r, key, _) => (*r, json!({"kind":"field", "id":key})),
                _ => return None,
            };
            value["bounds"] = bounds(Self::project_rect(r, *viewport)?);
            Some(value)
        }).collect();
        elements.extend(self.composition.borrow().declared_elements());
        json!({"units":"logical_pixels", "scale":self.scale, "regions":regions, "elements":elements,"composition":self.composition.borrow().report(),
            "popup":self.popup_bounds().map(bounds),
            "controls":self.buttons.iter().map(|b|json!({"id":b.key,"bounds":bounds(b.rect),"enabled":b.enabled})).collect::<Vec<_>>()})
    }
    pub fn bounds(&self, key: &str) -> Option<Rect> {
        self.regions
            .get(key)
            .copied()
            .or_else(|| self.buttons.iter().find(|b| b.key == key).map(|b| b.rect))
    }
    pub fn progress(&mut self, r: Rect) {
        self.draw(Visual::Progress(r));
    }
    pub fn animation_damage(&self) -> retinue_render::damage::Damage {
        self.visuals
            .iter()
            .filter_map(|(draw, viewport)| {
                if let Visual::Progress(r) = draw {
                    Self::project_rect(*r, *viewport)
                } else {
                    None
                }
            })
            .fold(retinue_render::damage::Damage::None, |d, r| {
                d.union(retinue_render::damage::Damage::rect(r))
            })
    }
    pub fn scrollbar(&mut self, key: &str, viewport: Rect, maximum: f32, offset: f32) {
        if maximum <= 0.0 || viewport.height == 0 {
            return;
        }
        let track = Rect {
            x: viewport.x + viewport.width - self.px(8.0),
            width: self.px(8.0),
            ..viewport
        };
        let height = ((viewport.height as f32).powi(2) / (viewport.height as f32 + maximum))
            .max(self.px(24.0) as f32)
            .min(viewport.height as f32);
        let thumb = Rect {
            y: viewport.y
                + ((viewport.height as f32 - height) * offset.clamp(0.0, maximum) / maximum).round()
                    as u32,
            height: height.round() as u32,
            ..track
        };
        self.fill(
            Rect {
                x: thumb.x + self.px(2.0),
                width: thumb.width.saturating_sub(self.px(3.0)),
                ..thumb
            },
            3.0,
            MUTED,
        );
        self.hit(key, track, "Scroll", true);
        if let (Some(track), Some(thumb)) = (self.screen_rect(track), self.screen_rect(thumb)) {
            self.scrollbars.insert(key.into(), (track, thumb, maximum));
        }
    }
    fn drag_scrollbar(&mut self, key: &str) {
        if let Some((track, thumb, max)) = self.scrollbars.get(key) {
            let travel = track.height.saturating_sub(thumb.height) as f32;
            let value = ((self.pointer.1 as f32 - track.y as f32 - self.scroll_grab)
                / travel.max(1.0))
            .clamp(0.0, 1.0)
                * max;
            let signal = Signal {
                key: key.into(),
                value: Some(value.to_string()),
            };
            if let Some(last) = self.signals.last_mut().filter(|s| s.key == key) {
                *last = signal;
            } else {
                self.signals.push(signal);
            }
        }
    }
    pub fn px(&self, n: f32) -> u32 {
        (n * self.scale).round().max(0.0) as u32
    }
    pub fn measure(&self, text: &str, size: f32, bold: bool) -> f32 {
        text_width(
            if bold { &self.bold } else { &self.regular },
            text,
            size * self.scale,
        )
        .ceil()
            / self.scale
    }
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x: self.px(x),
            y: self.px(y),
            width: self.px(w),
            height: self.px(h),
        }
    }
    pub fn begin_scroll(&mut self, rect: Rect, offset: u32) {
        self.begin_scroll_viewport(
            retinue_ui::scroll::Viewport {
                bounds: rect,
                ..Default::default()
            },
            offset,
        );
    }
    pub fn begin_scroll_viewport(&mut self, viewport: retinue_ui::scroll::Viewport, offset: u32) {
        let clip = self.screen_rect(viewport.bounds).unwrap_or_default();
        let (parent_x, parent_y) = self.viewport.map_or((0, 0), |(_, origin)| origin);
        let (x, y) = viewport.translation(offset);
        self.viewports.push(self.viewport);
        self.viewport = Some((clip, (parent_x + x, parent_y + y)));
    }
    pub fn end_scroll(&mut self) {
        self.viewport = self.viewports.pop().expect("unbalanced UI viewport scope");
    }
    fn draw(&mut self, draw: Visual) {
        self.visuals.push((draw, self.viewport));
    }

    pub fn fill(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        self.draw(Visual::Fill(r, radius, color));
    }
    pub fn outline(&mut self, r: Rect, radius: f32, color: [u8; 4]) {
        self.draw(Visual::Outline(r, radius, color));
    }
    pub fn arrow(&mut self, r: Rect, down: bool, color: [u8; 4]) {
        let x = r.x + r.width / 2;
        let (head, tail) = if down {
            (r.y + r.height, r.y)
        } else {
            (r.y, r.y + r.height)
        };
        self.draw(Visual::Line((x, tail), (x, head), color));
        let wing = if down {
            head - r.height / 3
        } else {
            head + r.height / 3
        };
        self.draw(Visual::Line((x, head), (r.x, wing), color));
        self.draw(Visual::Line((x, head), (r.x + r.width, wing), color));
    }
    pub fn polygon(&mut self, points: &[[f32; 2]], fill: [u8; 4], stroke: [u8; 4]) {
        self.draw(Visual::Polygon(
            points
                .iter()
                .map(|[x, y]| [(*x * self.scale) as f64, (*y * self.scale) as f64])
                .collect(),
            fill,
        ));
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            self.draw(Visual::Line(
                (self.px(a[0]), self.px(a[1])),
                (self.px(b[0]), self.px(b[1])),
                stroke,
            ));
        }
    }
    pub fn text(&mut self, r: Rect, text: impl Into<String>, size: f32, color: [u8; 4]) {
        self.label(r, text.into(), size, color, Weight::Regular);
    }
    pub fn bold(&mut self, r: Rect, text: impl Into<String>, size: f32, color: [u8; 4]) {
        self.label(r, text.into(), size, color, Weight::Bold);
    }
    fn font(&self, weight: Weight) -> &CachedFont {
        match weight {
            Weight::Light => &self.light,
            Weight::Regular => &self.regular,
            Weight::Medium => &self.medium,
            Weight::Bold => &self.bold,
        }
    }
    pub fn aligned(
        &mut self,
        r: Rect,
        text: impl Into<String>,
        size: f32,
        color: [u8; 4],
        weight: Weight,
        alignment: f32,
    ) {
        let text = text.into();
        let width = text_width(self.font(weight), &text, size * self.scale).ceil() as u32;
        let offset =
            (r.width.saturating_sub(width) as f32 * alignment.clamp(0.0, 1.0)).round() as u32;
        self.label(
            Rect {
                x: r.x + offset,
                width: r.width - offset,
                ..r
            },
            text,
            size,
            color,
            weight,
        );
    }
    fn label(&mut self, r: Rect, text: String, size: f32, color: [u8; 4], weight: Weight) {
        let font = self.font(weight);
        let text =
            crate::presentation::widgets::fit_text(font, text, size * self.scale, r.width as f32);
        self.draw(Visual::Text(r, text, size, color, weight));
    }
    pub fn clipped_text(
        &mut self,
        r: Rect,
        text: impl Into<String>,
        size: f32,
        color: [u8; 4],
        weight: Weight,
    ) {
        self.draw(Visual::Text(r, text.into(), size, color, weight));
    }
    pub fn image(&mut self, r: Rect, asset: Option<&Asset>) {
        if let Some(asset) = asset {
            if asset.source == "builtin" && asset.image_name == "forma.png" {
                self.bitmap(
                    r,
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../wfcompanion/assets/forma.png"
                    )),
                );
                return;
            }
            self.draw(Visual::Image(r, asset.clone()));
        }
    }
    #[cfg(test)]
    pub fn component(&mut self, r: Rect, asset: Option<&Asset>, ready: bool) {
        self.draw(Visual::ComponentBase(r, ready));
        let inset = self.px(2.0);
        let art = Rect {
            x: r.x + inset,
            y: r.y + inset,
            width: r.width.saturating_sub(2 * inset),
            height: r.height.saturating_sub(2 * inset),
        };
        if let Some(asset) = asset {
            if asset.source == "builtin" {
                self.image(art, Some(asset));
            } else {
                self.draw(Visual::CircleImage(art, asset.clone()));
            }
        }
        if ready {
            self.draw(Visual::ComponentGlow(r));
        }
    }
    pub fn covered_sampled(&mut self, r: Rect, asset: Option<&Asset>, edge: u32) {
        if let Some(asset) = asset {
            self.draw(Visual::Covered(r, asset.clone(), edge));
        }
    }
    pub fn icon(&mut self, r: Rect, name: &str) {
        self.bitmap(r, media::icon(name));
    }
    pub fn tinted_icon(&mut self, r: Rect, name: &str, color: [u8; 4]) {
        self.tinted_bitmap(r, media::icon(name), color);
    }
    pub fn tinted_bitmap(&mut self, r: Rect, bytes: &'static [u8], color: [u8; 4]) {
        let request = self
            .embedded
            .entry((bytes.as_ptr() as usize, r.width.max(r.height)))
            .or_insert_with(|| Request::embedded(bytes, r.width.max(r.height), None))
            .clone()
            .tinted([color[2], color[1], color[0], color[3]]);
        self.draw(Visual::Icon(r, request));
    }
    pub fn bitmap(&mut self, r: Rect, bytes: &'static [u8]) {
        let request = self
            .embedded
            .entry((bytes.as_ptr() as usize, r.width.max(r.height)))
            .or_insert_with(|| Request::embedded(bytes, r.width.max(r.height), None))
            .clone();
        self.draw(Visual::Icon(r, request));
    }
    pub fn clipped_bitmap(&mut self, clip: Rect, r: Rect, bytes: &'static [u8]) {
        self.clipped_bitmap_sampled(clip, r, bytes, r.width.max(r.height));
    }
    pub fn clipped_bitmap_sampled(&mut self, clip: Rect, r: Rect, bytes: &'static [u8], edge: u32) {
        let request = self
            .embedded
            .entry((bytes.as_ptr() as usize, edge))
            .or_insert_with(|| Request::embedded(bytes, edge, None))
            .clone();
        self.draw(Visual::ClippedIcon(clip, r, request));
    }
    pub fn texture(&mut self, r: Rect, bytes: &'static [u8], edge: u32) {
        self.texture_sized(r, bytes, edge, false);
    }
    pub fn covered_texture(&mut self, r: Rect, bytes: &'static [u8], edge: u32) {
        self.texture_sized(r, bytes, edge, true);
    }
    fn texture_sized(&mut self, r: Rect, bytes: &'static [u8], edge: u32, cover: bool) {
        let request = self
            .embedded
            .entry((bytes.as_ptr() as usize, edge))
            .or_insert_with(|| Request::embedded(bytes, edge, None))
            .clone();
        self.draw(Visual::Texture(r, request, cover));
    }
    pub fn hit(
        &mut self,
        key: impl Into<String>,
        r: Rect,
        label: impl Into<String>,
        enabled: bool,
    ) {
        let Some(r) = self.screen_rect(r) else {
            return;
        };
        self.buttons.push(Button {
            key: key.into(),
            rect: r,
            label: label.into(),
            enabled,
            field: false,
            binding: None,
            shape: HitShape::Rectangle,
        });
    }
    fn screen_rect(&self, r: Rect) -> Option<Rect> {
        Self::project_rect(r, self.viewport)
    }
    fn project_rect(r: Rect, viewport: Option<(Rect, (i32, i32))>) -> Option<Rect> {
        Some(if let Some((clip, (x, y))) = viewport {
            let left = (r.x as i64 + x as i64).max(clip.x as i64);
            let top = (r.y as i64 + y as i64).max(clip.y as i64);
            let right = (r.x as i64 + x as i64 + r.width as i64).min((clip.x + clip.width) as i64);
            let bottom =
                (r.y as i64 + y as i64 + r.height as i64).min((clip.y + clip.height) as i64);
            if right <= left || bottom <= top {
                return None;
            }
            Rect {
                x: left as u32,
                y: top as u32,
                width: (right - left) as u32,
                height: (bottom - top) as u32,
            }
        } else {
            r
        })
    }
    pub fn button(
        &mut self,
        key: impl Into<String>,
        r: Rect,
        label: &str,
        selected: bool,
        enabled: bool,
    ) {
        let key = key.into();
        self.fill(
            r,
            6.0,
            if selected {
                ACCENT
            } else if self.hover.as_ref() == Some(&key) {
                rgb(0x2c3650)
            } else {
                CARD
            },
        );
        let width = text_width(&self.regular, label, 13.0 * self.scale).round() as u32;
        self.text(
            Rect {
                x: r.x + r.width.saturating_sub(width) / 2,
                ..r
            },
            label,
            13.0,
            if enabled { TEXT } else { MUTED },
        );
        self.hit(key, r, label, enabled);
    }
    pub fn chip(&mut self, key: impl Into<String>, r: Rect, label: &str, selected: bool) {
        let key = key.into();
        self.fill(
            r,
            r.height as f32 / self.scale / 2.0,
            if selected {
                ACCENT
            } else if self.hover.as_ref() == Some(&key) {
                rgb(0x2c3650)
            } else {
                CARD
            },
        );
        self.aligned(r, label, 14.0, TEXT, Weight::Regular, 0.5);
        self.hit(key, r, label, true);
    }
    pub fn destructive(&mut self, key: impl Into<String>, r: Rect, label: &str, enabled: bool) {
        self.outline(r, 6.0, BAD);
        self.text(
            Rect {
                x: r.x + self.px(9.0),
                ..r
            },
            label,
            13.0,
            if enabled { BAD } else { MUTED },
        );
        self.hit(key, r, label, enabled);
    }
    pub fn toggle(&mut self, key: impl Into<String>, r: Rect, label: &str, on: bool) {
        let key = key.into();
        self.toggles.insert(key.clone(), on);
        let box_ = Rect {
            x: r.x,
            y: r.y + r.height.saturating_sub(self.px(18.0)) / 2,
            width: self.px(18.0),
            height: self.px(18.0),
        };
        self.fill(box_, 4.0, if on { ACCENT } else { CARD });
        self.outline(box_, 4.0, if on { rgb(0xa686e5) } else { MUTED });
        if on {
            let p = (box_.x + self.px(4.0), box_.y + self.px(9.0));
            let q = (box_.x + self.px(8.0), box_.y + self.px(13.0));
            let r = (box_.x + self.px(14.0), box_.y + self.px(5.0));
            self.draw(Visual::Line(p, q, TEXT));
            self.draw(Visual::Line(q, r, TEXT));
        }
        self.text(
            Rect {
                x: r.x + self.px(26.0),
                width: r.width.saturating_sub(self.px(26.0)),
                ..r
            },
            label,
            13.0,
            TEXT,
        );
        self.hit(key, r, label, true);
    }
    pub fn field(&mut self, key: &str, r: Rect, value: &str, hint: &str) {
        self.field_value(key, value);
        self.fill(r, 6.0, rgb(0x20283e));
        self.outline(r, 6.0, rgb(0x333e58));
        self.field_control(key, r, hint);
    }
    fn field_value(&mut self, key: &str, value: &str) {
        let entry = self
            .fields
            .entry(key.into())
            .or_insert_with(|| RefCell::new(self.edit_resources.editor()));
        if entry.borrow().text() != value {
            entry.borrow_mut().set_text(value, false);
        }
    }
    fn field_control(&mut self, key: &str, r: Rect, hint: &str) {
        self.draw(Visual::Field(r, key.into(), hint.into()));
        let count = self.buttons.len();
        self.hit(key, r, hint, true);
        if self.buttons.len() > count {
            self.buttons.last_mut().unwrap().field = true;
        }
    }
    pub fn search_field(
        &mut self,
        key: &str,
        available: Rect,
        value: &str,
        hint: &str,
        anchor: crate::presentation::widgets::search::Anchor,
    ) -> Rect {
        self.styled_search_field(
            key,
            available,
            value,
            hint,
            crate::presentation::widgets::search::Style::Capsule(anchor),
        )
    }
    pub fn styled_search_field(
        &mut self,
        key: &str,
        available: Rect,
        value: &str,
        hint: &str,
        style: crate::presentation::widgets::search::Style,
    ) -> Rect {
        use crate::{presentation::widgets::search::Anchor, presentation::widgets::search::Style};
        self.field_value(key, value);
        let active = self.focus.as_deref() == Some(key)
            || self.focus.as_deref() == Some(&format!("clear:{key}"))
            || !value.is_empty();
        let search = self.searches.entry(key.into()).or_insert_with(|| Search {
            motion: Default::default(),
            style,
            width: 0.0,
        });
        search.style = style;
        search.width = available.width as f32 / self.scale;
        search.update(
            active,
            text_width(&self.regular, value, 14.0),
            Instant::now(),
            true,
        );
        let anchor = match style {
            Style::Capsule(anchor) | Style::Fill(anchor) => anchor,
            Style::Field => Anchor::Leading,
        };
        let width = (search.motion.width * self.scale)
            .round()
            .min(available.width as f32) as u32;
        let r = Rect {
            x: match anchor {
                crate::presentation::widgets::search::Anchor::Leading => available.x,
                crate::presentation::widgets::search::Anchor::Trailing => {
                    available.x + available.width - width
                }
            },
            width,
            ..available
        };
        let radius = if matches!(style, Style::Field) {
            6.0
        } else {
            r.height as f32 / self.scale / 2.0
        };
        self.fill(r, radius, CARD);
        self.field_control(key, r, hint);
        self.icon(
            Rect {
                x: r.x + self.px(7.0),
                y: r.y + r.height.saturating_sub(self.px(18.0)) / 2,
                width: self.px(18.0),
                height: self.px(18.0),
            },
            "search",
        );
        if !value.is_empty() && r.width >= self.px(64.0) {
            let clear = Rect {
                x: r.x + r.width - self.px(32.0),
                width: self.px(32.0),
                ..r
            };
            let id = format!("clear:{key}");
            if self.hover.as_ref() == Some(&id) {
                self.fill(clear, radius, crate::presentation::style::CARD_HOVER);
            }
            self.icon(
                Rect {
                    x: clear.x + self.px(9.0),
                    y: clear.y + clear.height.saturating_sub(self.px(14.0)) / 2,
                    width: self.px(14.0),
                    height: self.px(14.0),
                },
                "close",
            );
            self.hit(id, clear, "Clear search", true);
        }
        r
    }
    fn field_rect(&self, bounds: Rect, key: &str) -> Rect {
        let inset = self.px(if self.searches.contains_key(key) {
            32.0
        } else {
            8.0
        });
        Rect {
            x: bounds.x + inset.min(bounds.width),
            width: bounds.width.saturating_sub(inset * 2),
            ..bounds
        }
    }
    pub fn password(&mut self, key: &str, r: Rect, value: &str) {
        self.fields
            .entry(key.into())
            .or_insert_with(|| RefCell::new(self.edit_resources.password()));
        self.field(key, r, value, "Password");
    }
    pub fn number_field(
        &mut self,
        key: &str,
        r: Rect,
        value: &str,
        hint: &str,
        min: u32,
        max: u32,
    ) {
        self.field(
            key,
            Rect {
                width: r.width.saturating_sub(self.px(24.0)),
                ..r
            },
            value,
            hint,
        );
        self.numbers.insert(key.into(), (min, max));
        let x = r.x + r.width - self.px(22.0);
        for (suffix, down, y) in [("up", false, r.y), ("down", true, r.y + r.height / 2)] {
            let rect = Rect {
                x,
                y,
                width: self.px(22.0),
                height: r.height / 2,
            };
            self.fill(rect, 3.0, rgb(0x293149));
            let center = x + self.px(11.0);
            let cy = y + rect.height / 2;
            let tip = if down {
                cy + self.px(2.0)
            } else {
                cy.saturating_sub(self.px(2.0))
            };
            let edge = if down {
                cy.saturating_sub(self.px(2.0))
            } else {
                cy + self.px(2.0)
            };
            self.draw(Visual::Line(
                (center - self.px(4.0), edge),
                (center, tip),
                TEXT,
            ));
            self.draw(Visual::Line(
                (center, tip),
                (center + self.px(4.0), edge),
                TEXT,
            ));
            self.hit(
                format!("number:{key}:{suffix}"),
                rect,
                if down { "Decrease" } else { "Increase" },
                true,
            );
        }
    }
    fn step_number(&mut self, key: &str, up: bool) {
        if let Some(&(min, max)) = self.numbers.get(key)
            && let Some(field) = self.fields.get(key)
        {
            let current = field.borrow().text().parse::<u32>().unwrap_or(min);
            let next = if up {
                current.saturating_add(1)
            } else {
                current.saturating_sub(1)
            }
            .clamp(min, max);
            let next = next.to_string();
            field.borrow_mut().set_text(&next, false);
            self.signals.push(Signal {
                key: key.into(),
                value: Some(next),
            });
        }
    }
    pub fn forget(&mut self, key: &str) {
        self.fields.remove(key);
        self.searches.remove(key);
        if self.focus.as_deref() == Some(key) {
            self.focus = None;
        }
    }
    pub fn choice(&mut self, key: &str, r: Rect, value: &str, options: &[(&str, &str)]) {
        let label = options
            .iter()
            .find(|(id, _)| *id == value)
            .map(|(_, label)| *label)
            .unwrap_or(value);
        self.fill(
            r,
            6.0,
            if self.hover.as_deref() == Some(key) {
                rgb(0x2c3650)
            } else {
                CARD
            },
        );
        self.text(
            Rect {
                x: r.x + self.px(10.0),
                width: r.width.saturating_sub(self.px(34.0)),
                ..r
            },
            label,
            13.0,
            TEXT,
        );
        let x = r.x + r.width.saturating_sub(self.px(17.0));
        let y = r.y + r.height / 2;
        self.draw(Visual::Line(
            (x, y.saturating_sub(self.px(2.0))),
            (x + self.px(4.0), y + self.px(2.0)),
            MUTED,
        ));
        self.draw(Visual::Line(
            (x + self.px(4.0), y + self.px(2.0)),
            (x + self.px(8.0), y.saturating_sub(self.px(2.0))),
            MUTED,
        ));
        self.hit(key, r, label, true);
        self.choices.insert(
            key.into(),
            options
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        self.selected_choices.insert(
            key.into(),
            options.iter().position(|(id, _)| *id == value).unwrap_or(0),
        );
    }

    pub fn open_context(&mut self, invoker: String) {
        self.cancel_activation();
        self.context_invoker = Some(invoker);
    }

    pub fn close_context(&mut self) {
        self.context_keys.clear();
        if let Some(invoker) = self.context_invoker.take() {
            self.focus = Some(invoker);
        }
        self.cancel_activation();
    }

    pub fn begin_context(&mut self, entries: &[(String, String, bool)], close: &str) {
        self.begin_modal();
        self.context_close = close.into();
        self.context_keys = entries
            .iter()
            .filter(|(_, _, enabled)| *enabled)
            .map(|(key, _, _)| key.clone())
            .collect();
        if !self
            .focus
            .as_ref()
            .is_some_and(|key| self.context_keys.contains(key))
        {
            self.focus = self.context_keys.first().cloned();
        }
    }
    pub fn assets(&self) -> Vec<Asset> {
        self.visuals
            .iter()
            .filter_map(|(d, _)| match d {
                Visual::Image(_, a) | Visual::Covered(_, a, _) => Some(a.clone()),
                #[cfg(test)]
                Visual::CircleImage(_, a) => Some(a.clone()),
                _ => None,
            })
            .chain(self.composition.borrow().assets())
            .collect()
    }
    pub fn requests(&self, media: &Media) -> Vec<Request> {
        self.visuals
            .iter()
            .filter_map(|(d, _)| match d {
                Visual::Image(r, a) => media.request(a, r.width.max(r.height)),
                #[cfg(test)]
                Visual::CircleImage(r, a) => media.request(a, r.width.max(r.height)),
                Visual::Covered(_, a, edge) => media.request(a, *edge),
                Visual::Icon(_, r) | Visual::ClippedIcon(_, _, r) | Visual::Texture(_, r, _) => {
                    Some(r.clone())
                }
                _ => None,
            })
            .chain(self.composition.borrow().requests(media))
            .collect()
    }
    pub fn image_damage(
        &self,
        keys: &std::collections::HashSet<retinue_render::thumbnails::Key>,
        media: &Media,
    ) -> retinue_render::damage::Damage {
        use retinue_render::damage::Damage;
        self.visuals.iter().fold(
            self.composition
                .borrow_mut()
                .image_damage(keys, media, self.size, self.scale),
            |damage, (draw, viewport)| {
                let (bounds, key) = match draw {
                    Visual::Image(r, a) => {
                        (*r, media.request(a, r.width.max(r.height)).map(|r| r.key))
                    }
                    #[cfg(test)]
                    Visual::CircleImage(r, a) => {
                        (*r, media.request(a, r.width.max(r.height)).map(|r| r.key))
                    }
                    Visual::Covered(r, a, edge) => (*r, media.request(a, *edge).map(|r| r.key)),
                    Visual::Icon(r, request) | Visual::Texture(r, request, _) => {
                        (*r, Some(request.key))
                    }
                    Visual::ClippedIcon(clip, r, request) => {
                        (clip.intersection(*r), Some(request.key))
                    }
                    _ => return damage,
                };
                if key.is_some_and(|key| keys.contains(&key)) {
                    damage.union(
                        Self::project_rect(bounds, *viewport)
                            .map(Damage::rect)
                            .unwrap_or_default(),
                    )
                } else {
                    damage
                }
            },
        )
    }
    fn popup_rows(&self) -> Vec<(usize, Rect)> {
        let Some((key, _)) = &self.popup else {
            return vec![];
        };
        let Some(button) = self.active_buttons().iter().find(|b| &b.key == key) else {
            return vec![];
        };
        let total = self.choices.get(key).map_or(0, Vec::len);
        let n = total.min(
            (self.size.1 / self.px(30.0).max(1))
                .saturating_sub(2)
                .max(1) as usize,
        );
        let selected = self.popup.as_ref().map_or(0, |(_, i)| *i);
        let first = selected.saturating_sub(n / 2).min(total.saturating_sub(n));
        let h = self.px(30.0);
        let width = button.rect.width.max(self.px(160.0)).min(self.size.0);
        let padding = self.px(4.0);
        let height = h * n as u32 + 2 * padding;
        let y = if button.rect.y + button.rect.height + height + padding <= self.size.1 {
            button.rect.y + button.rect.height + padding
        } else {
            button.rect.y.saturating_sub(height + padding)
        };
        (0..n)
            .map(|i| {
                (
                    first + i,
                    Rect {
                        x: button.rect.x.min(self.size.0 - width) + padding,
                        y: y + padding + i as u32 * h,
                        width: width.saturating_sub(2 * padding),
                        height: h,
                    },
                )
            })
            .collect()
    }
    fn popup_bounds(&self) -> Option<Rect> {
        let rows = self.popup_rows();
        let first = rows.first()?.1;
        let last = rows.last()?.1;
        let p = self.px(4.0);
        Some(Rect {
            x: first.x.saturating_sub(p),
            y: first.y.saturating_sub(p),
            width: first.width + 2 * p,
            height: last.y + last.height - first.y + 2 * p,
        })
    }
    pub fn paint(&self, p: &mut Painter<'_>, media: &Media) {
        self.composition.borrow_mut().paint(self, p, media);
    }

    fn paint_live(&self, p: &mut Painter<'_>, draw: &Visual) {
        match draw {
            Visual::Field(r, key, hint) => self.paint_field(p, *r, key, hint),
            _ => unreachable!("ordinary visuals are retained composition commands"),
        }
    }

    fn paint_overlays(&self, p: &mut retinue_ui::drawing::PixelCanvas) {
        for button in self.active_buttons() {
            if self
                .focus_visible
                .shows(self.focus.as_ref() == Some(&button.key), button.field)
            {
                if self.searches.get(&button.key).is_some_and(|search| {
                    !matches!(
                        search.style,
                        crate::presentation::widgets::search::Style::Field
                    )
                }) {
                    crate::presentation::style::focus_capsule(p, button.rect, self.scale);
                } else {
                    crate::presentation::style::focus_ring(p, button.rect, self.scale);
                }
            }
        }
        if let Some((key, selected)) = &self.popup {
            if let Some(r) = self.popup_bounds() {
                crate::presentation::widgets::menu::frame(p, r, self.scale);
            }
            for (i, r) in self.popup_rows() {
                if let Some((_, label)) = self.choices.get(key).and_then(|v| v.get(i)) {
                    crate::presentation::widgets::menu::row(
                        p,
                        r,
                        &self.regular,
                        label,
                        true,
                        i == *selected,
                        self.scale,
                    );
                }
            }
        }
        self.paint_tooltip(p);
    }
    fn paint_field(&self, p: &mut Painter<'_>, bounds: Rect, key: &str, hint: &str) {
        let r = self.field_rect(bounds, key);
        if r.is_empty() {
            return;
        }
        let mut edit = self.fields[key].borrow_mut();
        let focused = self.focus.as_deref() == Some(key)
            && self.active_buttons().iter().any(|b| b.key == key);
        if edit.text().is_empty() && !focused {
            p.viewport(r, (0, 0), |p| {
                p.draw_text_vertically_centered(
                    self.regular.as_ref(),
                    TextLine::new(r.x, r.y, r.height),
                    14.0 * self.scale,
                    hint,
                    MUTED,
                )
            });
        } else if focused {
            edit.paint(
                p,
                r,
                14.0 * self.scale,
                Colors {
                    text: TEXT,
                    selection: rgb(0x644ba2),
                },
            );
        } else {
            let text = if key.ends_with("password") {
                "*".repeat(edit.text().chars().count())
            } else {
                edit.text()
            };
            p.viewport(r, (0, 0), |p| {
                p.draw_text_vertically_centered(
                    self.regular.as_ref(),
                    TextLine::new(r.x, r.y, r.height),
                    14.0 * self.scale,
                    &text,
                    TEXT,
                )
            });
        }
    }
    fn paint_tooltip(&self, p: &mut retinue_ui::drawing::PixelCanvas) {
        if !self.tooltip || self.popup.is_some() || self.pressed.is_some() {
            return;
        }
        let Some(button) = self
            .active_buttons()
            .iter()
            .find(|b| Some(&b.key) == self.hover.as_ref())
        else {
            return;
        };
        if button.label.is_empty() {
            return;
        }
        let max_width = self
            .px(340.0)
            .min(self.size.0.saturating_sub(self.px(20.0)));
        let lines = retinue_render::painter::wrap_text(
            &self.regular,
            &button.label,
            13.0 * self.scale,
            max_width as f32,
        );
        let pad = self.px(8.0);
        let line = self.px(18.0);
        let width = lines
            .iter()
            .map(|s| text_width(&self.regular, s, 13.0 * self.scale).ceil() as u32)
            .max()
            .unwrap_or(0)
            + pad * 2;
        let height = lines.len().min(12) as u32 * line + pad * 2;
        let x =
            (self.pointer.0.max(0.0) as u32 + self.px(10.0)).min(self.size.0.saturating_sub(width));
        let mut y = self.pointer.1.max(0.0) as u32 + self.px(20.0);
        if y + height > self.size.1 {
            y = (self.pointer.1.max(0.0) as u32).saturating_sub(height + self.px(6.0));
        }
        p.fill_rounded_rect(x, y, width, height, self.px(6.0), rgb(0x30374a));
        for (i, text) in lines.iter().take(12).enumerate() {
            p.draw_text_vertically_centered(
                &self.regular,
                TextLine::new(x + pad, y + pad + i as u32 * line, line),
                13.0 * self.scale,
                text,
                TEXT,
            );
        }
    }
    pub fn pointer_move(&mut self, p: (f64, f64)) -> bool {
        self.pointer = p;
        self.input
            .pointer(PointerPhase::Move, self.input_position());
        if let Some(key) = self
            .pressed
            .clone()
            .filter(|k| self.scrollbars.contains_key(k))
        {
            self.drag_scrollbar(&key);
            return true;
        }
        if let Some(key) = &self.pressed
            && let Some(field) = self.fields.get(key)
            && let Some(button) = self.buttons.iter().find(|b| &b.key == key)
        {
            field
                .borrow_mut()
                .drag(p.0 as f32 - self.field_rect(button.rect, key).x as f32);
            return true;
        }
        if let Some((_, selected)) = &self.popup {
            if let Some((i, _)) = self.popup_rows().iter().find(|(_, r)| r.contains(p.0, p.1)) {
                if *selected != *i {
                    self.popup.as_mut().unwrap().1 = *i;
                    return true;
                }
            }
        }
        self.refresh_hover()
    }
    pub fn pointer_button(&mut self, down: bool) {
        if down {
            self.focus_visible.pointer_down();
        }
        if let Some((key, _)) = self.popup.clone() {
            self.popup_input.update(
                self.popup_rows()
                    .into_iter()
                    .map(|(i, r)| {
                        ControlRegion::new(
                            i,
                            retinue_ui::Bounds::new(
                                r.x as f32 / self.scale,
                                r.y as f32 / self.scale,
                                r.width as f32 / self.scale,
                                r.height as f32 / self.scale,
                            ),
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            if down {
                if !self.dismiss_popup_at(self.pointer) {
                    self.popup_input.button(true, self.input_position());
                }
                return;
            }
            if !down {
                if let Some(i) = self
                    .popup_input
                    .button(false, self.input_position())
                    .activated
                {
                    let value = self.choices[&key][i].0.clone();
                    self.signals.push(Signal {
                        key,
                        value: Some(value),
                    });
                }
                self.popup = None;
                self.popup_input.cancel();
            }
            return;
        }
        let result = self.input.button(down, self.input_position());
        self.refresh_hover();
        self.pressed = self.input.captured().map(|id| id.0.clone());
        if down {
            self.focus = self.pressed.clone();
            self.tooltip = false;
            if let Some(key) = self.pressed.clone()
                && let Some((_, thumb, _)) = self.scrollbars.get(&key)
            {
                self.scroll_grab = if thumb.contains(self.pointer.0, self.pointer.1) {
                    self.pointer.1 as f32 - thumb.y as f32
                } else {
                    thumb.height as f32 / 2.0
                };
                self.drag_scrollbar(&key);
                return;
            }
            if let Some(key) = &self.focus
                && let Some(field) = self.fields.get(key)
                && let Some(button) = self.buttons.iter().find(|b| &b.key == key)
            {
                let rect = self.field_rect(button.rect, key);
                field.borrow_mut().layout(rect, 14.0 * self.scale);
                field
                    .borrow_mut()
                    .click(self.pointer.0 as f32 - rect.x as f32, false);
            }
        } else if let Some((key, _, _)) = result.activated {
            self.activate(&key);
        }
    }
    fn activate(&mut self, key: &str) {
        if let Some(key) = key
            .strip_prefix("clear:")
            .filter(|key| self.searches.contains_key(*key))
        {
            self.fields[key].borrow_mut().set_text("", false);
            self.focus = Some(key.into());
            self.signals.push(Signal {
                key: key.into(),
                value: Some(String::new()),
            });
            return;
        }
        if let Some((key, direction)) = key.strip_prefix("number:").and_then(|k| k.rsplit_once(':'))
        {
            self.step_number(key, direction == "up");
            return;
        }
        if self.scrollbars.contains_key(key) {
            return;
        }
        if self.choices.contains_key(key) {
            self.popup = Some((key.into(), *self.selected_choices.get(key).unwrap_or(&0)));
        } else if !self.buttons.iter().any(|b| b.key == key && b.field) {
            self.signals.push(Signal {
                key: key.into(),
                value: None,
            });
        }
    }
    pub fn focus_next(&mut self, reverse: bool) {
        self.focus_visible.keyboard_input();
        let current = self
            .input
            .keys()
            .find(|id| Some(&id.0) == self.focus.as_ref())
            .cloned();
        self.input.focus(current.as_ref());
        self.input.focus_next(reverse);
        self.focus = self.input.focused().map(|id| id.0.clone());
    }
    pub fn key(&mut self, key: Key<&str>, mods: ModifiersState) -> bool {
        self.focus_visible.keyboard_input();
        self.tooltip = false;
        if self.regions.contains_key("page.menu") {
            let current = self
                .focus
                .as_ref()
                .and_then(|key| self.context_keys.iter().position(|k| k == key))
                .unwrap_or(0);
            let count = self.context_keys.len();
            match key {
                Key::Named(NamedKey::Escape | NamedKey::Tab) => {
                    self.signals.push(Signal {
                        key: self.context_close.clone(),
                        value: None,
                    });
                    self.close_context();
                }
                Key::Named(NamedKey::ArrowDown) if count > 0 => {
                    self.focus = Some(self.context_keys[(current + 1) % count].clone())
                }
                Key::Named(NamedKey::ArrowUp) if count > 0 => {
                    self.focus = Some(self.context_keys[(current + count - 1) % count].clone())
                }
                Key::Named(NamedKey::Home) => self.focus = self.context_keys.first().cloned(),
                Key::Named(NamedKey::End) => self.focus = self.context_keys.last().cloned(),
                Key::Named(NamedKey::Enter | NamedKey::Space) if count > 0 => {
                    let id = self.context_keys[current].clone();
                    self.activate(&id);
                }
                _ => (),
            }
            return true;
        }
        if self.has_modal()
            && self
                .focus
                .as_ref()
                .is_some_and(|key| !self.active_buttons().iter().any(|b| &b.key == key))
        {
            self.focus = None;
        }
        if key == Key::Named(NamedKey::Escape) {
            self.popup = None;
            self.focus = None;
            return true;
        }
        if let Some((id, index)) = self.popup.clone() {
            match key {
                Key::Named(NamedKey::ArrowDown) => {
                    self.popup.as_mut().unwrap().1 =
                        (index + 1).min(self.choices[&id].len().saturating_sub(1))
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.popup.as_mut().unwrap().1 = index.saturating_sub(1)
                }
                Key::Named(NamedKey::Enter) => {
                    self.signals.push(Signal {
                        key: id.clone(),
                        value: Some(self.choices[&id][index].0.clone()),
                    });
                    self.popup = None;
                }
                _ => (),
            }
            return true;
        }
        if key == Key::Named(NamedKey::Tab) {
            self.focus_next(mods.shift_key());
            return true;
        }
        if mods.control_key() && key == Key::Character("f") {
            self.focus = self
                .active_buttons()
                .iter()
                .find(|b| b.field && self.searches.contains_key(&b.key))
                .map(|b| b.key.clone());
            if let Some(field) = self.focus.as_ref().and_then(|id| self.fields.get(id)) {
                field.borrow_mut().key(0x61, true, false);
            }
            return true;
        }
        let Some(id) = self.focus.clone() else {
            return false;
        };
        if self.numbers.contains_key(&id)
            && matches!(key, Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown))
        {
            self.step_number(&id, key == Key::Named(NamedKey::ArrowUp));
            return true;
        }
        if self.buttons.iter().any(|b| b.key == id && b.field) {
            let Some(edit) = self.fields.get(&id) else {
                return false;
            };
            let mut edit = edit.borrow_mut();
            let before = edit.text();
            if mods.control_key() && matches!(key, Key::Character("c" | "x" | "v")) {
                use retinue_ui::desktop::clipboard::Operation;
                let op = match key {
                    Key::Character("v") => Some(Operation::Read),
                    _ => edit.selected_text().map(Operation::Write),
                };
                if let Some(op) = op {
                    self.cut =
                        (key == Key::Character("x")).then(|| (id.clone(), edit.edit_stamp()));
                    self.clipboard = Some((id.clone(), edit.edit_stamp(), op));
                }
                return true;
            }
            match key {
                Key::Character("a") if mods.control_key() => {
                    edit.key(0x61, true, false);
                }
                Key::Character(text) if !mods.control_key() && !mods.alt_key() => {
                    edit.insert(text, |v| v.chars().count() <= 4096);
                }
                Key::Named(NamedKey::Enter) => {
                    self.signals.push(Signal {
                        key: format!("{id}.submit"),
                        value: Some(before),
                    });
                    return true;
                }
                Key::Named(key) => {
                    let symbol = match key {
                        NamedKey::Backspace => 0xff08,
                        NamedKey::Delete => 0xffff,
                        NamedKey::ArrowLeft => 0xff51,
                        NamedKey::ArrowRight => 0xff53,
                        NamedKey::Home => 0xff50,
                        NamedKey::End => 0xff57,
                        _ => return false,
                    };
                    edit.key(symbol, mods.control_key(), mods.shift_key());
                }
                _ => return false,
            }
            let text = edit.text();
            if text != before {
                self.signals.push(Signal {
                    key: id,
                    value: Some(text),
                });
            }
            return true;
        }
        if matches!(key, Key::Named(NamedKey::Enter | NamedKey::Space)) {
            self.activate(&id);
            return true;
        }
        false
    }
    pub fn clipboard_reply(&mut self, id: &str, stamp: Instant, value: Option<String>) {
        if let Some(field) = self.fields.get(id) {
            let mut field = field.borrow_mut();
            if self.cut.take() == Some((id.into(), stamp)) && field.edit_stamp() == stamp {
                field.key(0xffff, false, false);
                self.signals.push(Signal {
                    key: id.into(),
                    value: Some(field.text()),
                });
                return;
            }
            if field.edit_stamp() == stamp
                && let Some(text) = value
            {
                field.insert(&text, |s| s.chars().count() <= 4096);
                self.signals.push(Signal {
                    key: id.into(),
                    value: Some(field.text()),
                });
            }
        }
    }
    pub fn cancel_pointer(&mut self) {
        self.input.cancel();
        self.popup_input.cancel();
        self.pressed = None;
        self.hover = None;
        self.popup = None;
        self.pointer = (-1.0, -1.0);
        self.tooltip = false;
    }
    pub fn cancel_activation(&mut self) {
        self.input.cancel();
        self.popup_input.cancel();
        self.pressed = None;
        self.popup = None;
    }
    pub fn tick(&mut self, now: Instant) -> bool {
        self.animation_elapsed = now.saturating_duration_since(self.animation_start);
        let search_changed = self.update_searches(now, true);
        let tooltip = !self.tooltip
            && self.hover.is_some()
            && self.hover_since.elapsed() >= Duration::from_millis(600);
        self.tooltip |= tooltip;
        tooltip
            | std::mem::take(&mut self.input_changed)
            | search_changed
            | self
                .focus
                .as_ref()
                .and_then(|id| self.fields.get(id))
                .is_some_and(|f| f.borrow_mut().tick())
    }
    pub fn settle_searches(&mut self) {
        self.update_searches(Instant::now(), false);
    }
    fn update_searches(&mut self, now: Instant, animate: bool) -> bool {
        let mut search_changed = false;
        for (key, search) in &mut self.searches {
            if !self.buttons[self.modal_start..]
                .iter()
                .any(|b| &b.key == key)
            {
                continue;
            }
            let value = self.fields[key].borrow().text();
            let active = self.focus.as_ref() == Some(key)
                || self.focus.as_deref() == Some(&format!("clear:{key}"));
            search_changed |= search.update(
                active || !value.is_empty(),
                text_width(&self.regular, &value, 14.0),
                now,
                animate,
            );
        }
        search_changed
    }
    pub fn deadline(&self) -> Option<Instant> {
        let caret = self
            .focus
            .as_ref()
            .and_then(|id| self.fields.get(id))
            .map(|f| f.borrow().next_blink(Instant::now()));
        let hover = (!self.tooltip && self.hover.is_some())
            .then_some(self.hover_since + Duration::from_millis(600));
        let searches = self.active_buttons().iter().filter_map(|button| {
            self.searches
                .get(&button.key)
                .and_then(|search| search.motion.deadline(Instant::now()))
        });
        let animation = (self.animation_damage() != retinue_render::damage::Damage::None)
            .then(|| Instant::now() + Duration::from_millis(16));
        caret
            .into_iter()
            .chain(hover)
            .chain(searches)
            .chain(animation)
            .chain(self.composition.borrow().deadline(Instant::now()))
            .chain(self.input_changed.then(Instant::now))
            .min()
    }

    pub fn accessibility(&self, root: u64) -> retinue_ui::desktop::accesskit::TreeUpdate {
        use retinue_ui::{
            desktop::accesskit::Action, desktop::accesskit::Node, desktop::accesskit::NodeId,
            desktop::accesskit::Role, desktop::accesskit::TreeUpdate,
        };
        let menu = self.regions.contains_key("page.menu");
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        let mut focus = NodeId(root);
        for button in self.active_buttons() {
            let id = access_id(root, &button.key);
            let mut node = Node::new(if menu {
                Role::MenuItem
            } else if self.toggles.contains_key(&button.key) {
                Role::CheckBox
            } else if self.choices.contains_key(&button.key) {
                Role::ComboBox
            } else if button.field {
                Role::TextInput
            } else {
                Role::Button
            });
            node.set_label(&button.label);
            if let Some(&on) = self.toggles.get(&button.key) {
                node.set_toggled(if on {
                    retinue_ui::desktop::accesskit::Toggled::True
                } else {
                    retinue_ui::desktop::accesskit::Toggled::False
                });
            }
            if let Some(options) = self.choices.get(&button.key)
                && let Some(&index) = self.selected_choices.get(&button.key)
                && let Some((_, label)) = options.get(index)
            {
                node.set_value(label);
            }
            let r = button.rect;
            node.set_bounds(retinue_ui::desktop::accesskit::Rect {
                x0: r.x as f64,
                y0: r.y as f64,
                x1: (r.x + r.width) as f64,
                y1: (r.y + r.height) as f64,
            });
            if button.enabled {
                node.add_action(Action::Click);
                node.add_action(Action::Focus);
            } else {
                node.set_disabled();
            }
            if button.field && !button.key.ends_with("password") {
                node.set_value(self.fields[&button.key].borrow().text());
                node.add_action(Action::SetValue);
            }
            if self.focus.as_ref() == Some(&button.key) {
                focus = id;
            }
            children.push(id);
            nodes.push((id, node));
        }
        let mut node = Node::new(if menu {
            Role::Menu
        } else {
            Role::GenericContainer
        });
        node.set_children(children);
        nodes.push((NodeId(root), node));
        TreeUpdate {
            nodes,
            tree: None,
            focus,
            tree_id: retinue_ui::desktop::accesskit::TreeId::ROOT,
        }
    }
    pub fn access(
        &mut self,
        request: retinue_ui::desktop::accesskit::ActionRequest,
        root: u64,
    ) -> bool {
        use retinue_ui::{desktop::accesskit::Action, desktop::accesskit::ActionData};
        let Some(button) = self
            .active_buttons()
            .iter()
            .find(|b| access_id(root, &b.key) == request.target_node && b.enabled)
        else {
            return false;
        };
        let key = button.key.clone();
        self.focus = Some(key.clone());
        self.focus_visible.keyboard_input();
        match request.action {
            Action::Click => self.activate(&key),
            Action::SetValue => {
                if let Some(ActionData::Value(value)) = request.data
                    && let Some(edit) = self.fields.get(&key)
                {
                    if !value.chars().any(char::is_control) {
                        edit.borrow_mut().set_text(&value, false);
                        self.signals.push(Signal {
                            key,
                            value: Some(value.into()),
                        });
                    }
                }
            }
            _ => (),
        }
        true
    }
}

fn identity_hash(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn access_id(root: u64, key: &str) -> retinue_ui::desktop::accesskit::NodeId {
    retinue_ui::desktop::accesskit::NodeId(identity_hash((root, key)) | 1 << 63)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_controls_retain_identity_and_paint_when_items_reorder() {
        use retinue_ui::prelude::*;
        #[derive(PartialEq)]
        struct List([u64; 2]);
        impl View for List {
            fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
                ui! {
                    VStack(10.0) {
                        for id in self.0 {
                            view_host::PaintSlot(retinue_render::Key::new("test.item", id))
                                .frame(170.0, 50.0).id(retinue_render::Key::new("test.row", id))
                        }
                    }
                }
            }
        }
        let mut ui = ControlHost::new().unwrap();
        let media = Media::default();
        let mut before = None;
        for items in [[0, 1], [1, 0], [0, 1]] {
            ui.begin((600, 400), 1.25);
            ui.composition.get_mut().host.set_view(List(items));
            ui.update_view(FrameTime::now());
            for id in items {
                ui.view_control(retinue_render::Key::new("test.item", id), |ui, r| {
                    ui.fill(r, 8.0, CARD);
                    ui.text(r, if id == 0 { "Ash" } else { "Mesa" }, 14.0, TEXT);
                });
            }
            ui.finish();
            let mut pixels = vec![0; 600 * 400 * 4];
            let mut p = Painter::new(&mut pixels, 600, 400).unwrap();
            ui.paint(&mut p, &media);
            p.finish().unwrap();
            let composition = ui.composition.borrow();
            let ids = [0, 1].map(|id| {
                composition
                    .host
                    .tree
                    .unique(retinue_render::Key::new("test.item", id))
                    .unwrap()
                    .unwrap()
            });
            if let Some(previous) = before {
                assert_eq!(ids, previous);
                assert_eq!(composition.scene().stats.recordings, 0);
            }
            before = Some(ids);
        }
    }

    #[test]
    fn retained_primitives_match_custom_paint_at_clipped_scroll_edges() {
        for scale in [1.0, 1.25, 2.0] {
            let mut ui = ControlHost::new().unwrap();
            let size = (700, 500);
            ui.begin(size, scale);
            ui.fill(ui.rect(0.0, 0.0, 350.0, 250.0), 0.0, PANEL);
            ui.begin_scroll(ui.rect(23.0, 37.0, 230.0, 150.0), ui.px(29.0));
            let card = ui.rect(4.0, 7.0, 180.0, 73.0);
            ui.fill(card, 14.0, CARD);
            ui.outline(card, 14.0, ACCENT);
            ui.text(ui.rect(10.0, 9.0, 170.0, 60.0), "Clipped card", 18.0, TEXT);
            ui.draw(Visual::ComponentBase(
                ui.rect(90.0, 140.0, 40.0, 40.0),
                true,
            ));
            ui.end_scroll();
            ui.finish();
            let media = Media::default();
            let mut composed = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut expected = composed.clone();
            let mut p = Painter::new(&mut composed, size.0, size.1).unwrap();
            ui.paint(&mut p, &media);
            p.finish().unwrap();
            let mut p = Painter::new(&mut expected, size.0, size.1).unwrap();
            let paint = |p: &mut Painter<'_>, draw: &Visual| match draw {
                Visual::Fill(r, radius, color) => {
                    p.fill_rounded_rect(r.x, r.y, r.width, r.height, ui.px(*radius), *color)
                }
                Visual::Outline(r, radius, color) => {
                    p.outline(*r, ui.px(*radius), ui.px(1.0).max(1), *color)
                }
                Visual::Text(r, text, size, color, weight) => p.viewport(*r, (0, 0), |p| {
                    p.draw_text_vertically_centered(
                        ui.font(*weight),
                        TextLine::new(r.x, r.y, r.height),
                        *size * scale,
                        text,
                        *color,
                    )
                }),
                Visual::ComponentBase(r, ready) => p.fill_rounded_rect(
                    r.x,
                    r.y,
                    r.width,
                    r.height,
                    r.width / 2,
                    if *ready {
                        crate::presentation::style::COMPONENT_READY
                    } else {
                        crate::presentation::style::COMPONENT_EMPTY
                    },
                ),
                _ => unreachable!(),
            };
            for (draw, viewport) in &ui.visuals {
                if let Some((clip, offset)) = viewport {
                    p.viewport(*clip, *offset, |p| paint(p, draw));
                } else {
                    paint(&mut p, draw);
                }
            }
            p.finish().unwrap();
            assert_eq!(
                composed.iter().zip(expected).position(|(a, b)| *a != b),
                None,
                "scale={scale}"
            );
        }
    }

    #[test]
    fn artwork_arrives_and_clears_without_relaying_out_the_page() {
        use retinue_render::{image, resources::ResourceCache};
        let mut ui = ControlHost::new().unwrap();
        ui.begin((200, 160), 1.25);
        ui.begin_scroll(ui.rect(20.0, 20.0, 90.0, 70.0), ui.px(4.0));
        ui.icon(ui.rect(0.0, 0.0, 40.0, 40.0), "owned");
        ui.end_scroll();
        ui.finish();
        let mut media = Media::default();
        let paint = |media: &Media| {
            let mut pixels = vec![0; 200 * 160 * 4];
            let mut painter = Painter::new(&mut pixels, 200, 160).unwrap();
            ui.paint(&mut painter, media);
            painter.finish().unwrap();
            pixels
        };
        let empty = paint(&media);
        let requests = ui.requests(&media);
        let mut cache = ResourceCache::new(4096);
        let image = cache.insert(
            requests[0].key,
            RasterImage::from_rgba(image::RgbaImage::from_pixel(
                8,
                8,
                image::Rgba([240, 60, 80, 255]),
            ))
            .unwrap(),
        );
        media.accept(&[(requests[0].key, image)], &requests);
        assert_ne!(paint(&media), empty);
        assert_eq!(ui.composition.borrow().report()["layout_computations"], 0);
        media.clear();
        assert_eq!(paint(&media), empty);
    }

    #[test]
    fn image_damage_uses_the_same_scroll_clip_as_paint_and_input() {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((600, 400), 1.25);
        let clip = Rect {
            x: 50,
            y: 100,
            width: 200,
            height: 100,
        };
        ui.begin_scroll(clip, 150);
        ui.icon(
            Rect {
                x: 10,
                y: 140,
                width: 32,
                height: 32,
            },
            "mastery",
        );
        ui.end_scroll();
        ui.finish();
        let media = Media::default();
        let keys = ui.requests(&media).into_iter().map(|r| r.key).collect();
        assert_eq!(
            ui.image_damage(&keys, &media),
            retinue_render::damage::Damage::Rect(Rect {
                x: 60,
                y: 100,
                width: 32,
                height: 22
            })
        );
        assert_eq!(
            ui.image_damage(&Default::default(), &media),
            retinue_render::damage::Damage::None
        );
    }

    #[test]
    fn numeric_keys_respect_bounds_and_update_the_editor() {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((600, 400), 1.25);
        ui.number_field(
            "quantity",
            ui.rect(20.0, 20.0, 90.0, 30.0),
            "2",
            "Quantity",
            1,
            3,
        );
        ui.focus = Some("quantity".into());
        ui.key(Key::Named(NamedKey::ArrowUp), ModifiersState::empty());
        assert_eq!(ui.signals.last().unwrap().value.as_deref(), Some("3"));
        ui.key(Key::Named(NamedKey::ArrowUp), ModifiersState::empty());
        assert_eq!(ui.fields["quantity"].borrow().text(), "3");
    }

    fn modal(ui: &mut ControlHost) {
        ui.begin((600, 400), 1.25);
        ui.button(
            "behind",
            ui.rect(8.0, 8.0, 90.0, 30.0),
            "Behind",
            false,
            true,
        );
        ui.begin_modal();
        ui.choice(
            "choice",
            ui.rect(50.0, 50.0, 200.0, 32.0),
            "b",
            &[("a", "First"), ("b", "Second"), ("c", "Third")],
        );
        ui.finish();
    }

    #[test]
    fn modal_dropdown_survives_relayout_and_keeps_selected_value() {
        let mut ui = ControlHost::new().unwrap();
        modal(&mut ui);
        ui.activate("choice");
        assert_eq!(ui.popup.as_ref().unwrap().1, 1);
        modal(&mut ui);
        assert!(ui.popup_open());
        ui.key(Key::Named(NamedKey::ArrowDown), ModifiersState::empty());
        assert!(ui.signals.is_empty());
        ui.key(Key::Named(NamedKey::Enter), ModifiersState::empty());
        assert_eq!(ui.signals[0].value.as_deref(), Some("c"));
    }

    #[test]
    fn dropdown_outside_press_cannot_select_or_activate_on_release() {
        let mut ui = ControlHost::new().unwrap();
        modal(&mut ui);
        ui.activate("choice");
        let r = ui.popup_rows()[0].1;
        ui.pointer_move((0.0, 0.0));
        ui.pointer_button(true);
        assert!(!ui.popup_open());
        ui.pointer_move((r.x as f64 + 5.0, r.y as f64 + 5.0));
        ui.pointer_button(false);
        assert!(ui.signals.is_empty());
        ui.activate("choice");
        ui.pointer_move((r.x as f64 + 5.0, r.y as f64 + 5.0));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert_eq!(ui.signals.last().unwrap().value.as_deref(), Some("a"));
    }

    #[test]
    fn component_glow_reuses_shared_rendering_and_updates_on_scale() {
        let mut ui = ControlHost::new().unwrap();
        for scale in [1.0, 1.25, 2.0] {
            ui.begin((1000, 400), scale);
            for i in 0..10 {
                ui.component(ui.rect(i as f32 * 40.0, 10.0, 37.0, 37.0), None, true);
            }
            ui.finish();
            let mut pixels = vec![0; 1000 * 400 * 4];
            let mut painter = Painter::new(&mut pixels, 1000, 400).unwrap();
            ui.paint(&mut painter, &Media::default());
            painter.finish().unwrap();
            assert_eq!(ui.paint_stats().prepared_fragments, 1);
            assert!(ui.paint_stats().fragment_blits >= 10);
        }
    }

    #[test]
    fn modal_blocks_underlying_clicks_focus_and_accessibility() {
        let mut ui = ControlHost::new().unwrap();
        modal(&mut ui);
        ui.pointer_move((20.0, 20.0));
        ui.pointer_button(true);
        ui.pointer_button(false);
        assert!(ui.signals.is_empty());
        ui.focus_next(false);
        assert_eq!(ui.focus.as_deref(), Some("choice"));
        assert!(
            !ui.accessibility(1)
                .nodes
                .iter()
                .any(|(id, _)| *id == access_id(1, "behind"))
        );
    }

    #[test]
    fn disappearing_controls_drop_stale_selection() {
        let mut ui = ControlHost::new().unwrap();
        modal(&mut ui);
        ui.activate("choice");
        ui.focus = Some("choice".into());
        ui.begin((600, 400), 1.25);
        ui.finish();
        assert!(!ui.popup_open());
        assert!(ui.focus.is_none());
        ui.key(Key::Named(NamedKey::ArrowDown), ModifiersState::empty());
    }

    #[test]
    fn capture_bounds_follow_scroll_and_scale_once() {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((600, 400), 1.25);
        ui.begin_scroll(
            Rect {
                x: 20,
                y: 80,
                width: 500,
                height: 300,
            },
            50,
        );
        ui.region("card", ui.rect(16.0, 48.0, 200.0, 80.0));
        assert_eq!(
            ui.bounds("card"),
            Some(Rect {
                x: 40,
                y: 90,
                width: 250,
                height: 100
            })
        );
    }

    #[test]
    fn scrollbar_moves_without_activating_a_button() {
        let mut ui = ControlHost::new().unwrap();
        ui.begin((600, 400), 1.0);
        ui.scrollbar(
            "scroll",
            Rect {
                x: 0,
                y: 0,
                width: 400,
                height: 300,
            },
            600.0,
            0.0,
        );
        ui.finish();
        ui.pointer_move((396.0, 10.0));
        ui.pointer_button(true);
        ui.pointer_move((396.0, 110.0));
        ui.pointer_button(false);
        assert_eq!(ui.signals.last().unwrap().value.as_deref(), Some("300"));
    }
}
