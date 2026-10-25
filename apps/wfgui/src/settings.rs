use crate::data::{Job, Snapshot, Update, Usage};
use retinue_ui::{
    Rect,
    damage::Damage,
    desktop::accesskit::{self, ActionRequest, Node, NodeId, Role, TreeInfo, TreeUpdate},
    focus_visible::FocusVisible,
    layout::UiTree,
    painter::{CachedFont, GlyphCache, Painter, TextLine, text_width, wrap_text},
    scroll::Scroll,
    taffy::prelude::*,
};
use std::time::Instant;

pub const BACKGROUND: [u8; 4] = rgb(0x181e2f);
const GROUP: [u8; 4] = rgb(0x20283e);
const TEXT: [u8; 4] = rgb(0xeff1f7);
const MUTED: [u8; 4] = rgb(0x979db0);
const RED: [u8; 4] = rgb(0xff8796);
const GREEN: [u8; 4] = rgb(0x7edb91);

pub const fn rgb(color: u32) -> [u8; 4] {
    [color as u8, (color >> 8) as u8, (color >> 16) as u8, 255]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Refresh,
    ClearMemory,
    ClearRendered,
    ClearSource,
    Cancel,
    Confirm,
}

impl Action {
    pub fn id(self) -> NodeId {
        NodeId(100 + self as u64)
    }
    fn label(self) -> &'static str {
        match self {
            Self::Refresh => "Refresh",
            Self::ClearMemory => "Clear rendering memory",
            Self::ClearRendered => "Clear rendered images",
            Self::ClearSource => "Clear source assets",
            Self::Cancel => "Cancel",
            Self::Confirm => "Clear",
        }
    }
    fn destructive(self) -> bool {
        matches!(
            self,
            Self::ClearMemory | Self::ClearRendered | Self::ClearSource | Self::Confirm
        )
    }
}

enum Kind {
    Container,
    Group,
    Text {
        value: String,
        size: f32,
        color: [u8; 4],
        heading: bool,
    },
    Button {
        action: Action,
        enabled: bool,
    },
    Separator,
}
struct Element {
    bounds: Rect,
    kind: Kind,
}

pub struct Settings {
    pub snapshot: Snapshot,
    pub busy: bool,
    pub ready: bool,
    pub error: Option<String>,
    pub preference_error: Option<String>,
    pub ui_scale: f32,
    pub pending: Option<Job>,
    pub focus: Option<Action>,
    pub modal: Option<Action>,
    pub hover: Option<Action>,
    pub memory_generation: u64,
    pressed: Option<Action>,
    size: (u32, u32),
    display_scale: f32,
    viewport: Rect,
    content_left: u32,
    content_height: u32,
    elements: Vec<Element>,
    scroll: Scroll,
    offset: f32,
    dragging_scroll: Option<f64>,
    pub regular: CachedFont,
    pub medium: CachedFont,
    glyphs: GlyphCache,
    memory_text: String,
    image_memory: (usize, usize),
}

impl Settings {
    pub fn new(snapshot: Option<Snapshot>, ui_scale: f32) -> Result<Self, String> {
        let glyphs = GlyphCache::new(16 * 1024 * 1024);
        let regular = CachedFont::from_bytes(
            include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf"),
            Default::default(),
            &glyphs,
        )?;
        let medium = CachedFont::from_bytes(
            include_bytes!("../../../assets/fonts/roboto/Roboto-Medium.ttf"),
            Default::default(),
            &glyphs,
        )?;
        let ready = snapshot.is_some();
        let mut view = Self {
            snapshot: snapshot.unwrap_or_default(),
            busy: !ready,
            ready,
            error: None,
            preference_error: None,
            ui_scale,
            pending: None,
            focus: None,
            modal: None,
            hover: None,
            memory_generation: 0,
            pressed: None,
            size: (1000, 760),
            display_scale: 1.0,
            viewport: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            content_left: 0,
            content_height: 0,
            elements: Vec::new(),
            scroll: Default::default(),
            offset: 0.0,
            dragging_scroll: None,
            regular,
            medium,
            glyphs,
            memory_text: String::new(),
            image_memory: (0, 0),
        };
        view.update_memory();
        view.layout()?;
        Ok(view)
    }

    fn scale(&self) -> f32 {
        self.ui_scale * self.display_scale
    }
    fn px(&self, value: f32) -> u32 {
        (value * self.scale()).round().max(1.0) as u32
    }

    pub fn resize(&mut self, size: (u32, u32), display_scale: f64) {
        if self.size == size && self.display_scale == display_scale as f32 {
            return;
        }
        let width = self.content_width();
        let scale_changed = self.display_scale != display_scale as f32;
        self.size = size;
        self.display_scale = display_scale as f32;
        if scale_changed || width != self.content_width() {
            self.relayout();
        } else {
            self.place_content();
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.ui_scale = scale;
        self.relayout();
    }

    pub fn capture_bounds(&self, target: &str) -> Result<Rect, String> {
        match target {
            "window" => Ok(Rect {
                x: 0,
                y: 0,
                width: self.size.0,
                height: self.size.1,
            }),
            "settings.daemon" | "settings.storage" => self
                .elements
                .iter()
                .filter(|e| matches!(e.kind, Kind::Group))
                .nth(usize::from(target == "settings.storage"))
                .map(|e| self.screen_bounds(e.bounds))
                .filter(|rect| !rect.is_empty())
                .ok_or_else(|| "Capture target is not visible".into()),
            _ => Err("Unknown capture target".into()),
        }
    }

    fn relayout(&mut self) {
        if let Err(error) = self.layout() {
            self.error = Some(error);
        }
    }

    pub fn update(&mut self, update: Update) {
        self.snapshot = update.snapshot;
        self.error = update.error;
        self.busy = false;
        self.ready = true;
        self.update_memory();
        self.relayout();
    }

    pub fn set_preference_error(&mut self, error: Option<String>) {
        self.preference_error = error;
        self.relayout();
    }

    fn update_memory(&mut self) {
        let stats = self.glyphs.stats();
        self.memory_text = format!(
            "{} used of {}",
            bytes((stats.live_bytes + self.image_memory.0) as u64),
            bytes((stats.budget_bytes + self.image_memory.1) as u64)
        );
    }

    pub fn set_image_memory(&mut self, usage: (usize, usize)) {
        if self.image_memory != usage {
            self.image_memory = usage;
            self.update_memory();
            self.relayout();
        }
    }

    fn content_width(&self) -> u32 {
        let margin = self.px(24.0).min(self.size.0 / 4);
        self.size.0.saturating_sub(2 * margin).min(self.px(960.0))
    }

    fn place_content(&mut self) {
        self.content_left = (self.size.0 - self.content_width()) / 2;
        self.viewport = Rect {
            x: 0,
            y: self.px(76.0),
            width: self.size.0,
            height: self.size.1.saturating_sub(self.px(76.0)),
        };
        self.scroll.resize(
            self.content_height as f32,
            self.viewport.height as f32,
            Instant::now(),
        );
        self.offset = self.scroll.offset(Instant::now()).round();
    }

    fn layout(&mut self) -> Result<(), String> {
        let scale = self.scale();
        let width = self.content_width();
        let mut builder = Builder {
            tree: UiTree::new(),
            kinds: Vec::new(),
            scale,
            font: &self.regular,
        };
        let mut rows = Vec::new();
        if let Some(error) = &self.preference_error {
            rows.push(builder.text(error, 14.0, RED, false, width as f32)?);
        }
        rows.push(builder.text("Daemon", 13.0, MUTED, true, width as f32)?);
        let daemon = if self.busy {
            "Checking..."
        } else if self.snapshot.source.is_ok() {
            "Available"
        } else {
            "Unavailable"
        };
        let description = self
            .snapshot
            .source
            .as_ref()
            .err()
            .map_or("Background data service", String::as_str);
        let daemon_color = if self.snapshot.source.is_ok() {
            GREEN
        } else if self.busy {
            MUTED
        } else {
            RED
        };
        let daemon_row = builder.row(
            width,
            "Retinue daemon",
            description,
            daemon,
            None,
            Some((Action::Refresh, !self.busy)),
            daemon_color,
        )?;
        rows.push(builder.group(width, &[daemon_row])?);
        rows.push(builder.text("Storage", 13.0, MUTED, true, width as f32)?);
        let memory_row = builder.row(
            width,
            "Rendering memory",
            "Glyphs and prepared images kept for this window.",
            &self.memory_text,
            None,
            Some((Action::ClearMemory, true)),
            TEXT,
        )?;
        let (usage_text, path, color) = usage(&self.snapshot.rendered);
        let rendered_row = builder.row(
            width,
            "Rendered images",
            "Resolution-specific images generated from source assets.",
            &usage_text,
            path.as_deref(),
            Some((
                Action::ClearRendered,
                !self.busy && self.snapshot.rendered.is_ok(),
            )),
            color,
        )?;
        let (usage_text, path, color) = usage(&self.snapshot.source);
        let source_row = builder.row(
            width,
            "Source assets",
            "Original images managed by Retinue daemon.",
            &usage_text,
            path.as_deref(),
            Some((
                Action::ClearSource,
                !self.busy && self.snapshot.source.is_ok(),
            )),
            color,
        )?;
        rows.push(builder.group(width, &[memory_row, rendered_row, source_row])?);
        if let Some(error) = &self.error {
            rows.push(builder.text(error, 14.0, RED, false, width as f32)?);
        }
        let root = builder.tree.column(
            usize::MAX,
            Style {
                size: Size {
                    width: length(width as f32),
                    height: auto(),
                },
                gap: Size {
                    width: length(0.0_f32),
                    height: length(12.0 * scale),
                },
                padding: retinue_ui::taffy::Rect {
                    bottom: length(24.0 * scale),
                    left: length(0.0_f32),
                    right: length(0.0_f32),
                    top: length(0.0_f32),
                },
                ..Default::default()
            },
            &rows,
        )?;
        let resolved = builder.tree.compute(root, (0, 0), (width, self.size.1))?;
        self.content_height = resolved.bounds(usize::MAX).height;
        self.elements = builder
            .kinds
            .into_iter()
            .enumerate()
            .map(|(id, kind)| Element {
                bounds: resolved.bounds(id),
                kind,
            })
            .collect();
        self.place_content();
        Ok(())
    }

    fn modal_bounds(&self) -> Rect {
        let width = self
            .px(440.0)
            .min(self.size.0.saturating_sub(self.px(32.0)));
        let height = self.px(198.0).min(self.size.1);
        Rect {
            x: self.size.0.saturating_sub(width) / 2,
            y: self.size.1.saturating_sub(height) / 2,
            width,
            height,
        }
    }

    fn modal_buttons(&self) -> [(Action, Rect); 2] {
        let rect = self.modal_bounds();
        let width = self.px(90.0);
        let height = self.px(34.0);
        let x = rect.x + rect.width - self.px(24.0) - width;
        let y = rect.y + rect.height - self.px(24.0) - height;
        [
            (
                Action::Cancel,
                Rect {
                    x: x - width - self.px(10.0),
                    y,
                    width,
                    height,
                },
            ),
            (
                Action::Confirm,
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            ),
        ]
    }

    pub fn buttons(&self) -> Vec<(Action, Rect, bool)> {
        if self.modal.is_some() {
            return self
                .modal_buttons()
                .into_iter()
                .map(|(a, r)| (a, r, true))
                .collect();
        }
        let mut buttons = Vec::new();
        for element in &self.elements {
            if let Kind::Button { action, enabled } = element.kind {
                buttons.push((action, self.screen_bounds(element.bounds), enabled));
            }
        }
        buttons
    }

    fn screen_bounds(&self, rect: Rect) -> Rect {
        let top = rect.y as f32 + self.viewport.y as f32 - self.offset;
        let bottom = top + rect.height as f32;
        Rect {
            x: rect.x + self.content_left,
            y: top.max(self.viewport.y as f32) as u32,
            height: (bottom.min((self.viewport.y + self.viewport.height) as f32)
                - top.max(self.viewport.y as f32))
            .max(0.0) as u32,
            ..rect
        }
    }

    fn hit(&self, position: (f64, f64)) -> Option<Action> {
        self.buttons()
            .into_iter()
            .find(|(_, rect, enabled)| *enabled && rect.contains(position.0, position.1))
            .map(|(action, _, _)| action)
    }

    pub fn pointer_move(&mut self, position: (f64, f64)) -> Damage {
        if let Some(grab) = self.dragging_scroll {
            let thumb = self.thumb().unwrap_or(self.viewport);
            let travel = self.viewport.height.saturating_sub(thumb.height) as f64;
            if travel > 0.0 {
                self.scroll.to(
                    ((position.1 - grab - self.viewport.y as f64) / travel) as f32
                        * self.scroll.maximum(),
                    false,
                    Instant::now(),
                );
                return self.tick(Instant::now());
            }
        }
        let hover = self.hit(position);
        if hover == self.hover {
            return Damage::None;
        }
        let old = self.hover;
        self.hover = hover;
        self.buttons()
            .into_iter()
            .filter(|(a, _, _)| Some(*a) == old || Some(*a) == hover)
            .fold(Damage::None, |damage, (_, rect, _)| {
                damage.union(Damage::rect(rect.expanded(self.px(3.0))))
            })
    }

    pub fn pointer_button(&mut self, position: (f64, f64), down: bool) -> Damage {
        if down && self.modal.is_none() {
            if let Some(thumb) = self.thumb() {
                if position.0 >= (self.size.0 - self.px(16.0)) as f64
                    && self.viewport.contains(position.0, position.1)
                {
                    if thumb.contains(position.0, position.1) {
                        self.dragging_scroll = Some(position.1 - thumb.y as f64);
                    } else {
                        self.wheel(
                            if position.1 < thumb.y as f64 {
                                -(self.viewport.height as f32)
                            } else {
                                self.viewport.height as f32
                            },
                            true,
                        );
                    }
                    return Damage::rect(self.viewport);
                }
            }
        }
        if down {
            self.pressed = self.hit(position);
            self.focus = self.pressed;
        } else {
            self.dragging_scroll = None;
            if let Some(action) = self
                .pressed
                .take()
                .filter(|action| Some(*action) == self.hit(position))
            {
                self.activate(action);
            }
        }
        Damage::Full
    }

    pub fn cancel_pointer(&mut self) {
        self.hover = None;
        self.pressed = None;
        self.dragging_scroll = None;
    }

    pub fn focus_next(&mut self, backwards: bool) {
        let order = self
            .buttons()
            .into_iter()
            .filter(|(_, _, enabled)| *enabled)
            .map(|(a, _, _)| a)
            .collect::<Vec<_>>();
        if order.is_empty() {
            return;
        }
        let current = order.iter().position(|action| Some(*action) == self.focus);
        let next = match (current, backwards) {
            (Some(index), true) => (index + order.len() - 1) % order.len(),
            (Some(index), false) => (index + 1) % order.len(),
            (None, true) => order.len() - 1,
            (None, false) => 0,
        };
        self.focus_action(order[next]);
    }

    pub fn focus_action(&mut self, action: Action) {
        self.focus = Some(action);
        if self.modal.is_none() {
            if let Some(element) = self.elements.iter().find(
                |e| matches!(e.kind, Kind::Button { action: candidate, .. } if candidate == action),
            ) {
                self.scroll.reveal(
                    element.bounds.y as f32 - 4.0,
                    (element.bounds.y + element.bounds.height) as f32 + 4.0,
                    self.viewport.height as f32,
                    Instant::now(),
                );
                self.offset = self.scroll.offset(Instant::now()).round();
            }
        }
    }

    pub fn activate(&mut self, action: Action) {
        if !self
            .buttons()
            .iter()
            .any(|(a, _, enabled)| *a == action && *enabled)
        {
            return;
        }
        match action {
            Action::Refresh => {
                self.pending = Some(Job::Refresh);
                self.busy = true;
                self.update_memory();
                self.relayout();
            }
            Action::ClearMemory => {
                self.glyphs.clear();
                self.memory_generation += 1;
                self.update_memory();
                self.relayout();
            }
            Action::ClearRendered | Action::ClearSource => {
                self.modal = Some(action);
                self.focus = Some(Action::Cancel);
                self.hover = None;
            }
            Action::Cancel => {
                self.focus = self.modal.take();
                self.hover = None;
            }
            Action::Confirm => {
                if let Some(target) = self.modal.take() {
                    self.focus = Some(target);
                    self.pending = Some(if target == Action::ClearSource {
                        Job::ClearSource
                    } else {
                        Job::ClearRendered
                    });
                    self.busy = true;
                    self.hover = None;
                    self.relayout();
                }
            }
        }
    }

    pub fn wheel(&mut self, delta: f32, smooth: bool) {
        if self.modal.is_none() {
            self.scroll.by(delta, smooth, Instant::now());
        }
    }

    pub fn tick(&mut self, now: Instant) -> Damage {
        let offset = self.scroll.offset(now).round();
        if self.offset == offset {
            return Damage::None;
        }
        self.offset = offset;
        self.hover = None;
        Damage::rect(self.viewport)
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.scroll.deadline(Instant::now())
    }

    fn thumb(&self) -> Option<Rect> {
        let maximum = self.scroll.maximum();
        if maximum <= 0.0 {
            return None;
        }
        let height = (self.viewport.height as f32 * self.viewport.height as f32
            / (self.viewport.height as f32 + maximum))
            .max(self.px(24.0) as f32) as u32;
        Some(Rect {
            x: self.size.0.saturating_sub(self.px(10.0)),
            y: self.viewport.y
                + ((self.viewport.height.saturating_sub(height)) as f32 * self.offset / maximum)
                    .round() as u32,
            width: self.px(5.0),
            height,
        })
    }

    pub fn paint(&self, painter: &mut Painter<'_>, focus: FocusVisible) {
        painter.draw_text(
            &self.medium,
            self.content_left,
            self.px(23.0),
            24.0 * self.scale(),
            "Settings",
            TEXT,
        );
        painter.viewport(
            self.viewport,
            (
                self.content_left as i32,
                self.viewport.y as i32 - self.offset as i32,
            ),
            |painter| {
                for element in &self.elements {
                    if matches!(element.kind, Kind::Group) {
                        fill(painter, element.bounds, self.px(8.0), GROUP);
                    }
                }
                for element in &self.elements {
                    let rect = element.bounds;
                    match &element.kind {
                        Kind::Text {
                            value,
                            size,
                            color,
                            heading,
                        } => painter.draw_text(
                            if *heading {
                                &self.medium
                            } else {
                                &self.regular
                            },
                            rect.x,
                            rect.y,
                            *size,
                            value,
                            *color,
                        ),
                        Kind::Button { action, enabled } => {
                            self.button(painter, *action, rect, *enabled, focus)
                        }
                        Kind::Separator => fill(painter, rect, 0, rgb(0x313a58)),
                        Kind::Group | Kind::Container => (),
                    }
                }
            },
        );
        if let Some(thumb) = self.thumb() {
            fill(painter, thumb, self.px(3.0), rgb(0x6a738a));
        }
        if let Some(target) = self.modal {
            fill(
                painter,
                Rect {
                    x: 0,
                    y: 0,
                    width: self.size.0,
                    height: self.size.1,
                },
                0,
                [0, 0, 0, 170],
            );
            let rect = self.modal_bounds();
            fill(painter, rect, self.px(8.0), GROUP);
            painter.draw_text(
                &self.medium,
                rect.x + self.px(24.0),
                rect.y + self.px(22.0),
                18.0 * self.scale(),
                target.label(),
                TEXT,
            );
            let message = if target == Action::ClearSource {
                "Remove downloaded source assets? They will be fetched again when needed."
            } else {
                "Remove rendered images? They will be generated again when needed."
            };
            let lines = wrap_text(
                &self.regular,
                message,
                14.0 * self.scale(),
                rect.width.saturating_sub(self.px(48.0)) as f32,
            )
            .join("\n");
            painter.draw_text(
                &self.regular,
                rect.x + self.px(24.0),
                rect.y + self.px(60.0),
                14.0 * self.scale(),
                &lines,
                MUTED,
            );
            for (action, rect) in self.modal_buttons() {
                self.button(painter, action, rect, true, focus);
            }
        }
    }

    fn button(
        &self,
        painter: &mut Painter<'_>,
        action: Action,
        rect: Rect,
        enabled: bool,
        focus: FocusVisible,
    ) {
        let hovered = self.hover == Some(action) || self.pressed == Some(action);
        let color = if !enabled {
            rgb(0x60677c)
        } else if action.destructive() {
            RED
        } else {
            TEXT
        };
        fill(
            painter,
            rect,
            self.px(6.0),
            if action.destructive() && enabled {
                RED
            } else {
                rgb(0x424c6b)
            },
        );
        fill(
            painter,
            rect.inset(self.px(1.0)),
            self.px(5.0),
            if enabled && hovered {
                rgb(0x323c58)
            } else {
                GROUP
            },
        );
        if enabled && focus.shows(self.focus == Some(action), false) {
            crate::style::focus_ring(painter, rect, self.scale());
        }
        {
            let label = if matches!(
                action,
                Action::ClearMemory | Action::ClearRendered | Action::ClearSource
            ) {
                "Clear"
            } else {
                action.label()
            };
            let width = text_width(&self.regular, label, 14.0 * self.scale()).round() as u32;
            painter.draw_text_vertically_centered(
                &self.regular,
                TextLine::new(
                    rect.x + rect.width.saturating_sub(width) / 2,
                    rect.y,
                    rect.height,
                ),
                14.0 * self.scale(),
                label,
                color,
            );
        }
    }

    pub fn accessibility(&self) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (action, rect, enabled) in self.buttons() {
            let mut node = Node::new(Role::Button);
            node.set_label(action.label());
            node.set_bounds(access_rect(rect));
            if enabled {
                node.add_action(accesskit::Action::Click);
                node.add_action(accesskit::Action::Focus);
            } else {
                node.set_disabled();
            }
            children.push(action.id());
            nodes.push((action.id(), node));
        }
        if self.modal.is_none() {
            for (index, element) in self.elements.iter().enumerate() {
                if let Kind::Text { value, heading, .. } = &element.kind {
                    let id = NodeId(1000 + index as u64);
                    let mut node = Node::new(if *heading { Role::Heading } else { Role::Label });
                    node.set_label(value.clone());
                    node.set_bounds(access_rect(self.screen_bounds(element.bounds)));
                    children.push(id);
                    nodes.push((id, node));
                }
            }
        }
        let mut root = Node::new(if self.modal.is_some() {
            Role::Dialog
        } else {
            Role::GenericContainer
        });
        root.set_label(self.modal.map_or("Warframe settings", Action::label));
        if self.modal.is_some() {
            root.set_modal();
        }
        root.set_children(children);
        nodes.push((NodeId(2), root));
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(2))),
            tree_id: accesskit::TreeId::ROOT,
            focus: self.focus.map_or(NodeId(2), Action::id),
        }
    }

    pub fn access(&mut self, request: ActionRequest) {
        if let Some((action, _, _)) = self
            .buttons()
            .into_iter()
            .find(|(a, _, enabled)| a.id() == request.target_node && *enabled)
        {
            match request.action {
                accesskit::Action::Click => self.activate(action),
                accesskit::Action::Focus => {
                    self.focus_action(action);
                }
                _ => (),
            }
        }
    }
}

struct Builder<'a> {
    tree: UiTree<usize>,
    kinds: Vec<Kind>,
    scale: f32,
    font: &'a CachedFont,
}

impl Builder<'_> {
    fn group(&mut self, width: u32, rows: &[NodeIdTaffy]) -> Result<NodeIdTaffy, String> {
        let mut children = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if index != 0 {
                let id = self.kinds.len();
                self.kinds.push(Kind::Separator);
                children.push(self.tree.leaf(
                    id,
                    Style {
                        size: Size {
                            width: length(width as f32 - 32.0 * self.scale),
                            height: length(self.scale),
                        },
                        margin: retinue_ui::taffy::Rect {
                            left: length(16.0 * self.scale),
                            right: length(0.0_f32),
                            top: length(0.0_f32),
                            bottom: length(0.0_f32),
                        },
                        ..Default::default()
                    },
                )?);
            }
            children.push(*row);
        }
        let id = self.kinds.len();
        self.kinds.push(Kind::Group);
        self.tree.column(
            id,
            Style {
                size: Size {
                    width: length(width as f32),
                    height: auto(),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
            &children,
        )
    }
    fn text(
        &mut self,
        value: &str,
        size: f32,
        color: [u8; 4],
        heading: bool,
        width: f32,
    ) -> Result<NodeIdTaffy, String> {
        let size = size * self.scale;
        let value = wrap_text(self.font, value, size, width).join("\n");
        let metrics = self
            .font
            .horizontal_line_metrics(size)
            .ok_or("Missing font metrics")?;
        let height = metrics.new_line_size * value.lines().count().max(1) as f32;
        let id = self.kinds.len();
        self.kinds.push(Kind::Text {
            value,
            size,
            color,
            heading,
        });
        self.tree.leaf(
            id,
            Style {
                size: Size {
                    width: length(width),
                    height: length(height.ceil()),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        width: u32,
        title: &str,
        description: &str,
        value: &str,
        path: Option<&str>,
        button: Option<(Action, bool)>,
        color: [u8; 4],
    ) -> Result<NodeIdTaffy, String> {
        let text_width = width as f32 - 136.0 * self.scale;
        let mut labels = vec![
            self.text(title, 16.0, TEXT, true, text_width)?,
            self.text(description, 13.0, MUTED, false, text_width)?,
            self.text(value, 14.0, color, false, text_width)?,
        ];
        if let Some(path) = path {
            labels.push(self.text(path, 12.0, MUTED, false, text_width)?);
        }
        let text = self.tree.column(
            usize::MAX - self.kinds.len(),
            Style {
                gap: Size {
                    width: length(0.0_f32),
                    height: length(5.0 * self.scale),
                },
                ..Default::default()
            },
            &labels,
        )?;
        let mut children = vec![text];
        if let Some((action, enabled)) = button {
            let id = self.kinds.len();
            self.kinds.push(Kind::Button { action, enabled });
            children.push(self.tree.leaf(
                id,
                Style {
                    size: Size {
                        width: length(80.0 * self.scale),
                        height: length(34.0 * self.scale),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )?);
        }
        let id = self.kinds.len();
        self.kinds.push(Kind::Container);
        self.tree.row(
            id,
            Style {
                size: Size {
                    width: length(width as f32),
                    height: auto(),
                },
                align_items: Some(AlignItems::CENTER),
                justify_content: Some(JustifyContent::SPACE_BETWEEN),
                padding: retinue_ui::taffy::Rect {
                    left: length(16.0 * self.scale),
                    right: length(16.0 * self.scale),
                    top: length(16.0 * self.scale),
                    bottom: length(16.0 * self.scale),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
            &children,
        )
    }
}

type NodeIdTaffy = retinue_ui::taffy::NodeId;

fn fill(painter: &mut Painter<'_>, rect: Rect, radius: u32, color: [u8; 4]) {
    painter.fill_rounded_rect(rect.x, rect.y, rect.width, rect.height, radius, color);
}

fn access_rect(rect: Rect) -> accesskit::Rect {
    accesskit::Rect {
        x0: rect.x as f64,
        y0: rect.y as f64,
        x1: (rect.x + rect.width) as f64,
        y1: (rect.y + rect.height) as f64,
    }
}

fn bytes(value: u64) -> String {
    if value >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024 * 1024 {
        format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0))
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}

fn usage(result: &Result<Usage, String>) -> (String, Option<String>, [u8; 4]) {
    match result {
        Ok(usage) => (
            format!("{} in {} images", bytes(usage.bytes), usage.objects),
            Some(usage.cache_root.display().to_string()),
            TEXT,
        ),
        Err(error) => (error.clone(), None, RED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_content_moves_without_relayout_and_hits_follow_it() {
        let mut view = Settings::new(Some(Snapshot::fixture()), 1.25).unwrap();
        view.resize((1600, 1400), 1.0);
        let elements = view.elements.as_ptr();
        let before = view.buttons()[0].1;
        view.resize((1700, 1350), 1.0);
        assert_eq!(view.elements.as_ptr(), elements);
        let after = view.buttons()[0].1;
        assert_eq!(
            after,
            Rect {
                x: before.x + 50,
                ..before
            }
        );
        assert_eq!(
            view.hit((after.x as f64 + 1.0, after.y as f64 + 1.0)),
            Some(Action::Refresh)
        );
        let tree = view.accessibility();
        let node = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == Action::Refresh.id())
            .unwrap();
        assert_eq!(node.1.bounds(), Some(access_rect(after)));
        view.resize((900, 500), 1.0);
        assert_ne!(view.elements.as_ptr(), elements);
        view.focus_action(Action::ClearSource);
        assert!(view.offset > 0.0);
        view.resize((900, 2000), 1.0);
        assert_eq!(view.offset, 0.0);
    }

    #[test]
    fn destructive_actions_require_confirmation_and_modal_traps_focus() {
        let mut view = Settings::new(Some(Snapshot::fixture()), 1.0).unwrap();
        view.activate(Action::ClearSource);
        assert_eq!(view.modal, Some(Action::ClearSource));
        assert!(view.pending.is_none());
        assert_eq!(view.focus, Some(Action::Cancel));
        view.activate(Action::Refresh);
        assert!(view.pending.is_none());
        view.focus_next(false);
        assert_eq!(view.focus, Some(Action::Confirm));
        view.focus_next(false);
        assert_eq!(view.focus, Some(Action::Cancel));
        view.activate(Action::Cancel);
        assert!(view.pending.is_none());
        view.activate(Action::ClearRendered);
        view.activate(Action::Confirm);
        assert!(matches!(view.pending, Some(Job::ClearRendered)));
    }

    #[test]
    fn layout_scales_wraps_and_reveals_keyboard_focus() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut view = Settings::new(Some(Snapshot::fixture()), scale).unwrap();
            view.resize((900, 620), 1.0);
            for element in &view.elements {
                assert!(
                    element.bounds.x + element.bounds.width <= 900,
                    "scale {scale}"
                );
            }
            for _ in 0..8 {
                view.focus_next(false);
                if view.focus == Some(Action::ClearSource) {
                    break;
                }
            }
            assert_eq!(view.focus, Some(Action::ClearSource));
            let rect = view
                .buttons()
                .into_iter()
                .find(|(action, _, _)| *action == Action::ClearSource)
                .unwrap()
                .1;
            assert_eq!(rect.height, view.px(34.0));
            assert!(rect.y >= view.viewport.y && rect.y + rect.height <= view.size.1);
            assert_eq!(view.deadline(), None);
        }
    }

    #[test]
    fn release_outside_does_not_activate() {
        let mut view = Settings::new(Some(Snapshot::fixture()), 1.0).unwrap();
        let rect = view.buttons()[0].1;
        view.pointer_button((rect.x as f64 + 3.0, rect.y as f64 + 3.0), true);
        view.pointer_button((0.0, 0.0), false);
        assert!(view.pending.is_none());
    }
}
