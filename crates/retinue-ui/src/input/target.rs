use super::EventHandler;
use crate::{Bounds, runtime::ViewId, state::State};

#[derive(Clone, Default, PartialEq)]
pub struct Interaction {
    pub(super) hovered: State<bool>,
    pub(super) focused: State<bool>,
    pub(super) focus_visible: State<bool>,
    pub(super) captured: State<bool>,
}

impl Interaction {
    pub fn hovered(&self) -> bool {
        self.hovered.get()
    }
    pub fn focused(&self) -> bool {
        self.focused.get()
    }
    pub fn focus_visible(&self) -> bool {
        self.focus_visible.get()
    }
    pub fn captured(&self) -> bool {
        self.captured.get()
    }

    pub(super) fn reset(&self) {
        self.hovered.set(false);
        self.focused.set(false);
        self.focus_visible.set(false);
        self.captured.set(false);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FocusPolicy {
    #[default]
    None,
    Programmatic,
    Tab,
    Text,
}

impl FocusPolicy {
    pub fn tab_stop(self) -> bool {
        matches!(self, Self::Tab | Self::Text)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum HitShape {
    #[default]
    Rectangle,
    Rounded(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Automatic primary-button activation. Other gestures use the event handler.
pub enum Activation {
    #[default]
    None,
    Press,
    Release,
}

impl HitShape {
    pub fn contains(self, bounds: Bounds, point: (f32, f32)) -> bool {
        let (x, y) = (point.0 - bounds.x, point.1 - bounds.y);
        if !(x >= 0.0 && y >= 0.0 && x < bounds.width && y < bounds.height) {
            return false;
        }
        let Self::Rounded(radius) = self else {
            return true;
        };
        let radius = radius.max(0.0).min(bounds.width.min(bounds.height) * 0.5);
        let dx = x - x.clamp(radius, bounds.width - radius);
        let dy = y - y.clamp(radius, bounds.height - radius);
        dx * dx + dy * dy <= radius * radius
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct InputOptions {
    pub handler: Option<EventHandler>,
    pub focus: FocusPolicy,
    pub interaction: Option<Interaction>,
    pub shape: HitShape,
    pub activation: Activation,
    pub blocks_when_disabled: bool,
}

impl InputOptions {
    pub fn new(handler: EventHandler) -> Self {
        Self {
            handler: Some(handler),
            ..Self::default()
        }
    }
    pub fn focusable(mut self, focus: FocusPolicy) -> Self {
        self.focus = focus;
        self
    }
    pub fn interaction(mut self, interaction: Interaction) -> Self {
        self.interaction = Some(interaction);
        self
    }
    pub fn shape(mut self, shape: HitShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }
}

/// In paint order, with the nearest input ancestor rather than every layout box.
#[derive(Clone)]
pub struct Target {
    pub id: ViewId,
    pub parent: Option<ViewId>,
    pub bounds: Bounds,
    pub clip: Bounds,
    pub enabled: bool,
    pub input: InputOptions,
}

impl Target {
    pub fn contains(&self, point: (f32, f32)) -> bool {
        (self.enabled || self.input.blocks_when_disabled)
            && HitShape::Rectangle.contains(self.clip, point)
            && self.input.shape.contains(self.bounds, point)
    }
}
