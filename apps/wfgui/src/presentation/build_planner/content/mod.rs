mod content_view;
mod layout;
mod previews_view;
#[cfg(test)]
mod tests;
use super::*;

#[derive(Clone, PartialEq)]
enum Drawing {
    Text {
        text: String,
        size: f32,
        color: [u8; 4],
        bold: bool,
        link: Option<String>,
    },
    Rule,
    Slot(topology::Slot),
    Polarity(Value),
}
#[derive(Clone, PartialEq)]
struct Piece {
    key: Key,
    bounds: Bounds,
    drawing: Drawing,
}
impl Piece {
    fn frame(&self, m: &Metrics) -> Bounds {
        paint_frame(
            m,
            self.bounds,
            match &self.drawing {
                Drawing::Slot(slot) if slot.kind == topology::Kind::Arcane => 40.0,
                Drawing::Slot(_) => 12.0,
                _ => 0.0,
            },
        )
    }
    fn paint(&self, ui: &mut ControlHost, m: &Metrics, hidden: bool) {
        let frame = self.frame(m);
        let (x, y) = local_origin(m, frame, self.bounds);
        self.paint_at(ui, x, y, hidden);
    }
    fn paint_at(&self, ui: &mut ControlHost, x: f32, y: f32, hidden: bool) {
        let r = ui.rect(x, y, self.bounds.width, self.bounds.height);
        match &self.drawing {
            Drawing::Text {
                text,
                size,
                color,
                bold,
                link,
            } => {
                if *bold {
                    ui.bold(r, text, *size, *color);
                } else {
                    ui.text(r, text, *size, *color);
                }
                if let Some(url) = link {
                    ui.hit(format!("build:link:{url}"), r, url, true);
                }
            }
            Drawing::Rule => ui.fill(r, 0.0, MUTED),
            Drawing::Slot(slot) => {
                if !hidden {
                    slot.paint(ui, x, y);
                }
                ui.hit(
                    format!("build:mod:{}", text(&slot.slot, "id")),
                    r,
                    slot.tooltip(),
                    true,
                );
            }
            Drawing::Polarity(slot) => {
                let cell = self.bounds.width;
                ui.fill(ui.rect(x + 3.0, y, cell - 6.0, 58.0), 5.0, controls::CARD);
                ui.text(
                    ui.rect(x + 10.0, y + 3.0, cell - 20.0, 22.0),
                    text(slot, "label"),
                    12.0,
                    MUTED,
                );
                let changed = slot["changed"] == true;
                if changed {
                    topology::draw_polarity(
                        ui,
                        text(slot, "before"),
                        x + cell * 0.25,
                        y + 28.0,
                        MUTED,
                    );
                    ui.text(
                        ui.rect(x + cell * 0.43, y + 28.0, cell * 0.2, 24.0),
                        ">",
                        16.0,
                        MUTED,
                    );
                }
                topology::draw_polarity(
                    ui,
                    text(slot, "polarity"),
                    x + cell * if changed { 0.66 } else { 0.5 },
                    y + 28.0,
                    if changed { rgb(0xe8ca76) } else { GOOD },
                );
            }
        }
    }
}

// Keep the fractional origin inside a pixel-aligned Canvas. This preserves the
// artwork's original snapping when frames, text and clips share an anchor.
fn paint_frame(m: &Metrics, r: Bounds, pad: f32) -> Bounds {
    let pad = pad + 1.0 / m.scale;
    let x = ((r.x - pad).max(0.0) * m.scale).floor() / m.scale;
    let y = ((r.y - pad).max(0.0) * m.scale).floor() / m.scale;
    Bounds::new(
        x,
        y,
        ((r.x + r.width + pad) * m.scale).ceil() / m.scale - x,
        ((r.y + r.height + pad) * m.scale).ceil() / m.scale - y,
    )
}

fn local_origin(m: &Metrics, frame: Bounds, content: Bounds) -> (f32, f32) {
    (
        (content.x * m.scale - (frame.x * m.scale).round()) / m.scale,
        (content.y * m.scale - (frame.y * m.scale).round()) / m.scale,
    )
}

#[derive(PartialEq)]
struct Projection {
    model: Rc<build_planner_model::Model>,
    tab: String,
    width: f32,
    scale: f32,
}

pub(super) struct Content {
    enabled: State<bool>,
    mounted: Items<(Rc<Piece>, bool)>,
    previews: Items<topology::PreviewFrame>,
    offset: State<f32>,
    all: Vec<Rc<Piece>>,
    height: State<f32>,
    topology: Option<Bounds>,
    projection: Option<Projection>,
    notes: notes::Notes,
}
impl Content {
    pub fn new() -> Self {
        Self {
            enabled: State::new(false),
            mounted: Items::default(),
            previews: Items::default(),
            offset: State::new(0.0),
            all: vec![],
            height: State::new(0.0),
            topology: None,
            projection: None,
            notes: Default::default(),
        }
    }
    pub fn view(&self, metrics: State<Metrics>, state: State<PageState>) -> Element {
        content_view::ContentView {
            metrics,
            state,
            enabled: self.enabled.clone(),
            pieces: self.mounted.clone(),
            offset: self.offset.clone(),
            height: self.height.clone(),
        }
        .into_element()
    }
    pub fn previews_view(&self, metrics: State<Metrics>) -> Element {
        previews_view::PreviewsView {
            metrics,
            frames: self.previews.clone(),
        }
        .into_element()
    }
    pub fn sync(
        &mut self,
        ui: &ControlHost,
        m: &Metrics,
        model: &Rc<build_planner_model::Model>,
        b: &mut Builds,
        now: Instant,
    ) {
        let enabled = match b.tab.as_str() {
            "equipment" => model.equipment.selected,
            "discover" => model.discover.detail,
            _ => !model.groups.choices.is_empty() && model.groups.selected,
        };
        self.enabled.set(enabled);
        b.preview.begin();
        if !enabled {
            self.mounted.replace([]).unwrap();
            b.preview.clear();
            self.previews.replace([]).unwrap();
            return;
        }
        let projection = Projection {
            model: model.clone(),
            tab: b.tab.clone(),
            width: m.body_width(&b.tab),
            scale: m.scale,
        };
        if self.projection.as_ref() != Some(&projection) {
            let content = layout::build(ui, model, &b.tab, projection.width, &mut self.notes);
            self.all = content.pieces.into_iter().map(Rc::new).collect();
            self.height.set(content.height);
            self.topology = content.topology;
            self.projection = Some(projection);
        }
        let viewport = m.viewport(&b.tab);
        b.scroll.resize(
            ((self.height.get() + 20.0) * m.scale).round(),
            viewport.height * m.scale,
            now,
        );
        b.offset = b.scroll.offset(now).round() as u32;
        let offset = b.offset as f32 / m.scale;
        self.offset.set(offset);
        for p in &self.all {
            if let Drawing::Slot(slot) = &p.drawing {
                if slot.kind == topology::Kind::Mod {
                    let (x, y) = slot.mod_origin(p.bounds.x, p.bounds.y);
                    b.preview.card_at(
                        &slot.upgrade,
                        &slot.slot,
                        &slot.polarity,
                        (viewport.x + x, viewport.y + y - offset),
                        slot.scale,
                        slot.expanded_height,
                    );
                }
            }
        }
        b.preview.prepare(ui, b.dialog.is_none(), now);
        self.mounted
            .replace(
                self.all
                    .iter()
                    .filter(|p| {
                        let r = p.frame(m);
                        r.y + r.height >= offset && r.y <= offset + viewport.height
                    })
                    .map(|p| {
                        let hidden = match &p.drawing {
                            Drawing::Slot(s) if s.kind == topology::Kind::Mod => {
                                b.preview.covers(text(&s.slot, "id"), &s.upgrade)
                            }
                            _ => false,
                        };
                        (p.key, (p.clone(), hidden))
                    }),
            )
            .expect("unique content leaves");
        self.previews
            .replace(
                b.preview
                    .frames(ui)
                    .into_iter()
                    .map(|f| (key("build.preview", f.key()), f)),
            )
            .expect("unique animated cards");
    }
    pub fn bind(&self, ui: &mut ControlHost, b: &Builds) {
        if !self.enabled.get() {
            return;
        }
        let projection = self.projection.as_ref().unwrap();
        let m = Metrics {
            scale: ui.scale,
            ..Default::default()
        };
        for &id in self.mounted.order().keys() {
            let (p, hidden) = self.mounted.get(id).unwrap();
            ui.view_control(id, |ui, _| p.paint(ui, &m, hidden));
        }
        if let (Some(viewport), Some(t)) = (ui.view_bounds(BODY), self.topology) {
            ui.begin_scroll(viewport, b.offset);
            ui.region("build.topology", ui.rect(t.x, t.y, t.width, t.height));
            ui.end_scroll();
        }
        ui.view_control(SCROLL, |ui, r| {
            ui.scrollbar("build:scroll", r, b.scroll.maximum(), b.offset as f32)
        });
        for &id in self.previews.order().keys() {
            let f = self.previews.get(id).unwrap();
            let frame = paint_frame(&m, f.bounds, 8.0);
            ui.view_control(id, |ui, _| {
                let (x, y) = local_origin(&m, frame, f.bounds);
                f.paint(ui, x, y);
                if f.active {
                    let r = ui.rect(x, y, f.bounds.width, f.bounds.height);
                    ui.hit("build:mod-preview", r, "", true);
                    ui.region("build.mod-preview", r);
                }
            });
        }
        debug_assert_eq!(projection.tab, b.tab);
    }
}
