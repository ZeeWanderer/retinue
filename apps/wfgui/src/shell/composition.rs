use super::{Control, Geometry};
use retinue_ui::{
    Rect,
    composition::{Element, Key, Paint, RasterMode, RecordingMode, Renderer, Scene},
    damage::Damage,
    taffy::{Dimension, LengthPercentage, LengthPercentageAuto, Position, Size, Style},
};
use std::cell::RefCell;

pub const TITLE: Key = Key::new("shell", 1);
pub const LEFT: Key = Key::new("shell", 2);
pub const CENTER: Key = Key::new("shell", 3);
pub const RIGHT: Key = Key::new("shell", 4);
pub const TOOLTIP: Key = Key::new("shell", 5);

pub fn control_key(control: Control) -> Key {
    Key::new("shell.control", control as u64)
}

pub const CONTROLS: [Control; 15] = [
    Control::Left,
    Control::Close,
    Control::Maximize,
    Control::Minimize,
    Control::Right,
    Control::Larger,
    Control::Smaller,
    Control::Settings,
    Control::Player,
    Control::Foundry,
    Control::Mastery,
    Control::Inventory,
    Control::Relics,
    Control::Market,
    Control::Builds,
];

#[derive(Clone, Copy, PartialEq)]
pub struct Layout {
    pub size: (u32, u32),
    pub scale: f32,
    pub left: f32,
    pub right: f32,
    pub right_fits: bool,
    pub tooltip: bool,
}

pub struct Composition {
    pub scene: RefCell<Scene>,
    pub renderer: RefCell<Renderer>,
    layout: Option<Layout>,
}

impl Default for Composition {
    fn default() -> Self {
        Self {
            scene: RefCell::new(Scene::default()),
            renderer: RefCell::new(Renderer::new(0)),
            layout: None,
        }
    }
}

impl Composition {
    pub fn update(&mut self, layout: Layout) -> Damage {
        if self.layout == Some(layout) {
            return Damage::None;
        }
        let mut root = layout.describe();
        fn retain_paint(node: &mut Element, scene: &Scene) {
            if let Some(paint) = scene.paint(node.key) {
                node.paint = Some(paint.clone());
            }
            for child in &mut node.children {
                retain_paint(child, scene);
            }
        }
        retain_paint(&mut root, self.scene.get_mut());
        let damage = self
            .scene
            .get_mut()
            .update(root, layout.size, layout.scale, RecordingMode::Retained)
            .expect("unique shell composition identities");
        self.renderer
            .get_mut()
            .prepare(self.scene.get_mut(), RasterMode::Replay);
        self.layout = Some(layout);
        damage
    }

    pub fn rect(&self, key: Key) -> Rect {
        self.scene
            .borrow()
            .bounds(key)
            .map(|node| node.hit)
            .unwrap_or_default()
    }

    pub fn geometry(&self) -> Geometry {
        Geometry {
            title: self.rect(TITLE),
            left: self.rect(LEFT),
            center: self.rect(CENTER),
            right: self.rect(RIGHT),
        }
    }
}

impl Layout {
    fn snap(self, value: f32) -> f32 {
        (value * self.scale).round() / self.scale
    }

    fn describe(self) -> Element {
        let q = |v| self.snap(v);
        let width = self.size.0 as f32 / self.scale;
        let height = self.size.1 as f32 / self.scale;
        let left_width = q(52.0 + 90.0 * self.left);
        let right_width = q(400.0 * self.right);
        let body_height = (height - q(46.0)).max(0.0);
        let mut title = leaf(TITLE, width, q(40.0));
        title
            .children
            .push(control(Control::Left, q(36.0), q(5.0), q(30.0), q(30.0)));
        let mut trailing = Element::row(Key::new("shell", 6), 0.0, Vec::new());
        trailing.style.position = Position::Absolute;
        trailing.style.inset.right = LengthPercentageAuto::length(q(10.0));
        trailing.style.size.height = Dimension::length(q(40.0));
        for (button, width, gap) in [
            (Control::Smaller, 25.0, 38.0),
            (Control::Larger, 25.0, 14.0),
            (Control::Right, 30.0, 7.0),
            (Control::Minimize, 30.0, 0.0),
            (Control::Maximize, 30.0, 0.0),
            (Control::Close, 30.0, 0.0),
        ] {
            let mut button = leaf(control_key(button), q(width), q(width));
            button.interactive = true;
            button.style.margin.top = LengthPercentageAuto::length(q((40.0 - width) / 2.0));
            button.style.margin.right = LengthPercentageAuto::length(q(gap));
            trailing.children.push(button);
        }
        title.children.push(trailing);

        let mut navigation = leaf(LEFT, left_width, body_height);
        navigation.clip = true;
        let openness = ((left_width - 52.0) / 90.0).clamp(0.0, 1.0);
        let identity_height = q(49.0 + 31.0 * openness);
        navigation.children.push(control(
            Control::Player,
            q(3.0),
            q(8.0),
            (left_width - q(6.0)).max(0.0),
            identity_height,
        ));
        let start = q(8.0) + identity_height + q(if left_width > q(100.0) { 22.0 } else { 8.0 });
        let available = ((body_height - q(60.0) - start).max(0.0) * self.scale).round() as u32;
        let step = (available / 6).min((49.0 * self.scale).round() as u32) as f32 / self.scale;
        let mut nav = Element::column(Key::new("shell", 7), (step - q(40.0)).max(0.0), Vec::new());
        nav.style.position = Position::Absolute;
        nav.style.inset.left = LengthPercentageAuto::length(q(8.0));
        nav.style.inset.top = LengthPercentageAuto::length(start);
        for button in [
            Control::Foundry,
            Control::Mastery,
            Control::Inventory,
            Control::Relics,
            Control::Market,
            Control::Builds,
        ] {
            let mut element = Element::new(
                control_key(button),
                fixed((left_width - q(16.0)).max(0.0), step.min(q(40.0))),
            );
            element.interactive = true;
            nav.children.push(element);
        }
        navigation.children.push(nav);
        navigation.children.push(control(
            Control::Settings,
            q(8.0),
            (body_height - q(52.0)).max(0.0),
            (left_width - q(16.0)).max(0.0),
            q(40.0),
        ));
        navigation.style.margin.right = LengthPercentageAuto::length(q(7.0));

        let mut center = leaf(CENTER, 0.0, body_height);
        center.style.flex_grow = 1.0;
        center.style.min_size.width = Dimension::length(0.0);
        center.clip = true;
        let mut right = leaf(RIGHT, right_width, body_height);
        right.clip = true;
        if self.right_fits {
            right.style.margin.left = LengthPercentageAuto::length(q(7.0 * self.right));
        } else {
            right.style.position = Position::Absolute;
            right.style.inset.right = LengthPercentageAuto::length(q(6.0));
        }
        let mut body = Element::row(Key::new("shell", 8), 0.0, vec![navigation, center, right]);
        body.style.size = Size {
            width: Dimension::length(width),
            height: Dimension::length(body_height),
        };
        body.style.padding.left = LengthPercentage::length(q(6.0));
        body.style.padding.right = LengthPercentage::length(q(6.0));
        body.style.flex_shrink = 0.0;
        let mut root = Element::column(Key::new("shell", 0), q(6.0), vec![title, body]);
        root.style.size = Size {
            width: Dimension::length(width),
            height: Dimension::length(height),
        };
        if self.tooltip {
            let mut tooltip = leaf(TOOLTIP, width, height);
            tooltip.style.position = Position::Absolute;
            root.children.push(tooltip);
        }
        root
    }
}

fn fixed(width: f32, height: f32) -> Style {
    Style {
        size: Size {
            width: Dimension::length(width),
            height: Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Style::default()
    }
}

fn leaf(key: Key, width: f32, height: f32) -> Element {
    let mut element = Element::new(key, fixed(width, height));
    if matches!(key, CENTER | RIGHT) {
        element.paint = Some(Paint::external(key));
    }
    element
}

fn control(control: Control, x: f32, y: f32, width: f32, height: f32) -> Element {
    let key = control_key(control);
    let mut element = Element::new(key, fixed(width, height));
    element.style.position = Position::Absolute;
    element.style.inset.left = LengthPercentageAuto::length(x);
    element.style.inset.top = LengthPercentageAuto::length(y);
    element.interactive = true;
    element
}
