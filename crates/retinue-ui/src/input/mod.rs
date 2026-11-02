//! Input routing is independent of desktop windows and game-overlay focus policy.

mod controls;
mod handler;
mod router;
mod target;

pub use controls::{ControlDispatch, ControlRegion, Controls};
pub use handler::EventHandler;
pub use router::{Dispatch, Router};
pub use target::{Activation, FocusPolicy, HitShape, InputOptions, Interaction, Target};

use crate::{Bounds, runtime::ViewId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

impl PointerId {
    pub const MOUSE: Self = Self(0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerPhase {
    Move,
    Down(Button),
    Up(Button),
    Leave,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Character(String),
    Tab,
    Enter,
    Space,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Native(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Pointer {
        id: PointerId,
        phase: PointerPhase,
        /// Logical window coordinates, independent of render scale.
        position: (f32, f32),
        modifiers: Modifiers,
    },
    Scroll {
        position: (f32, f32),
        delta: (f32, f32),
        modifiers: Modifiers,
    },
    Key {
        key: Key,
        pressed: bool,
        repeat: bool,
        modifiers: Modifiers,
    },
    Text(String),
    Preedit {
        text: String,
        selection: Option<(usize, usize)>,
    },
    WindowFocused(bool),
    Focused(bool),
    Hovered(bool),
    CaptureLost(PointerId),
}

impl Event {
    pub fn position(&self) -> Option<(f32, f32)> {
        match self {
            Self::Pointer { position, .. } | Self::Scroll { position, .. } => Some(*position),
            _ => None,
        }
    }

    pub fn pointer(&self) -> Option<PointerId> {
        match self {
            Self::Pointer { id, .. } => Some(*id),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Capture,
    Target,
    Bubble,
}

#[derive(Default)]
pub(super) struct Requests {
    handled: bool,
    stopped: bool,
    focus: Option<Option<ViewId>>,
    capture: Option<(PointerId, Option<ViewId>)>,
}

pub struct EventContext {
    pub target: ViewId,
    pub current: ViewId,
    pub phase: Phase,
    pub bounds: Bounds,
    pub position: Option<(f32, f32)>,
    pub(super) pointer: Option<PointerId>,
    pub(super) requests: Requests,
}

impl EventContext {
    pub fn local_position(&self) -> Option<(f32, f32)> {
        self.position
            .map(|(x, y)| (x - self.bounds.x, y - self.bounds.y))
    }

    /// Suppress default handling without preventing ancestor handlers.
    pub fn handle(&mut self) {
        self.requests.handled = true;
    }

    pub fn stop_propagation(&mut self) {
        self.requests.stopped = true;
    }

    pub fn focus(&mut self) {
        self.requests.focus = Some(Some(self.current));
        self.handle();
    }

    pub fn blur(&mut self) {
        self.requests.focus = Some(None);
        self.handle();
    }

    pub fn capture_pointer(&mut self) {
        if let Some(id) = self.pointer {
            self.requests.capture = Some((id, Some(self.current)));
        }
    }

    pub fn release_pointer(&mut self) {
        if let Some(id) = self.pointer {
            self.requests.capture = Some((id, None));
        }
    }
}
