use crate::semantics::{Role, WindowAction};
use retinue_render::CacheHint;

#[derive(Clone, PartialEq)]
pub(crate) struct Properties {
    pub offset: (f32, f32),
    pub clip: bool,
}

#[derive(Clone, PartialEq)]
pub(crate) struct Attributes {
    pub hit: bool,
    pub hit_testing: bool,
    pub role: Option<Role>,
    pub window_action: Option<WindowAction>,
    pub reuse: Option<CacheHint>,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            offset: (0.0, 0.0),
            clip: false,
        }
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            hit: false,
            hit_testing: true,
            role: None,
            window_action: None,
            reuse: None,
        }
    }
}

impl Properties {
    pub fn apply(&self, element: &mut crate::scene::Element) {
        element.offset = self.offset;
        element.clip = self.clip;
    }
}

impl Attributes {
    pub fn apply(&self, element: &mut crate::scene::Element) {
        element.interactive |= self.hit;
        element.hit_testing &= self.hit_testing;
        if let Some(role) = self.role {
            element.role = role;
        }
        if let Some(action) = self.window_action {
            element.window_action = Some(action);
        }
        if let Some(reuse) = self.reuse {
            element.cache_hint = reuse;
        }
    }
}
