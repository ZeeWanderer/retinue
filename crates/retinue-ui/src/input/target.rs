use super::EventHandler;
use crate::{Bounds, runtime::ViewId, state::State};
use std::sync::Arc;

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
    /// Precise fragments for an owner without a single layout box. `bounds`
    /// remains the event-coordinate envelope, not a clickable union rectangle.
    pub regions: Option<Arc<[HitRegion]>>,
    pub enabled: bool,
    pub input: InputOptions,
}

impl Target {
    pub(crate) fn place(&mut self, mut regions: impl ExactSizeIterator<Item = HitRegion>) {
        if regions.len() <= 1 {
            let region = regions.next();
            self.bounds = region.map(|r| r.bounds).unwrap_or_default();
            self.clip = region.map(|r| r.clip).unwrap_or_default();
            self.regions = None;
            return;
        }
        if let Some(old) = &mut self.regions
            && old.len() == regions.len()
        {
            let fragments = Arc::make_mut(old);
            for (dst, src) in fragments.iter_mut().zip(regions) {
                *dst = src;
            }
        } else {
            self.regions = Some(regions.collect());
        }
        let fragments = self.regions.as_deref().unwrap();
        self.bounds = fragments[0].bounds;
        self.clip = fragments[0].clip;
        for fragment in &fragments[1..] {
            self.bounds = self.bounds.union(fragment.bounds);
            self.clip = self.clip.union(fragment.clip);
        }
    }

    #[inline]
    pub fn contains(&self, point: (f32, f32)) -> bool {
        (self.enabled || self.input.blocks_when_disabled)
            && self.regions.as_ref().map_or_else(
                || {
                    HitShape::Rectangle.contains(self.clip, point)
                        && self.input.shape.contains(self.bounds, point)
                },
                |regions| {
                    regions
                        .iter()
                        .any(|region| region.contains(self.input.shape, point))
                },
            )
    }

    pub(super) fn visible(&self) -> bool {
        self.regions.as_ref().map_or_else(
            || self.clip.width > 0.0 && self.clip.height > 0.0,
            |regions| {
                regions.iter().any(|region| {
                    let visible = region.bounds.intersection(region.clip);
                    visible.width > 0.0 && visible.height > 0.0
                })
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitRegion {
    pub bounds: Bounds,
    pub clip: Bounds,
}

impl HitRegion {
    #[inline]
    fn contains(&self, shape: HitShape, point: (f32, f32)) -> bool {
        HitShape::Rectangle.contains(self.clip, point) && shape.contains(self.bounds, point)
    }
}
