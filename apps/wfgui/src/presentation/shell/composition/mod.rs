use super::{Control, Geometry, Page, ShellView, shell_model::ShellModel, shell_model::Viewport};
use retinue_render::{RasterMode, Renderer, damage::Damage};
use retinue_ui::{
    Key, Rect, drawing::DrawList, prelude::*, runtime::ViewTree, semantics::WindowAction,
    semantics::WindowCapabilities,
};
use std::{cell::Cell, cell::RefCell, time::Duration, time::Instant};

pub const TITLE: Key = Key::new("shell", 1);
pub const LEFT: Key = Key::new("shell", 2);
pub const CENTER: Key = Key::new("shell", 3);
pub const RIGHT: Key = Key::new("shell", 4);
pub const TOOLTIP: Key = Key::new("shell", 5);
pub const BODY: Key = Key::new("shell", 8);
const PAGES: [Page; 7] = [
    Page::Settings,
    Page::Foundry,
    Page::Mastery,
    Page::Inventory,
    Page::Relics,
    Page::Market,
    Page::Builds,
];

pub fn page_key(page: Page) -> Key {
    Key::new("shell.page", page as u64)
}
pub fn page_for_key(key: Key) -> Option<Page> {
    PAGES.into_iter().find(|page| page_key(*page) == key)
}
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
    pub compact_left: bool,
    pub right_available: bool,
    pub tooltip: bool,
    pub page: Page,
}

pub struct Composition {
    pub tree: RefCell<ViewTree>,
    pub renderer: RefCell<Renderer>,
    model: ShellModel,
    time: Cell<FrameTime>,
}

impl Default for Composition {
    fn default() -> Self {
        let model = ShellModel::default();
        Self {
            tree: RefCell::new(ViewTree::new(ShellView(model.clone()), || {})),
            renderer: RefCell::new(Renderer::new(retinue_render::ResidencyPolicy::desktop(0))),
            model,
            time: Cell::new(FrameTime::now()),
        }
    }
}

impl Composition {
    pub fn update_at(&mut self, layout: Layout, now: Instant) -> Damage {
        self.model.viewport.set(Viewport {
            size: layout.size,
            scale: layout.scale,
        });
        self.model.left.set(layout.left);
        self.model.right.set(layout.right);
        self.model.right_fits.set(layout.right_fits);
        self.model.compact_left.set(layout.compact_left);
        self.model.right_available.set(layout.right_available);
        self.model.tooltip.set(layout.tooltip);
        self.model.page.set(layout.page);
        self.time.set(self.time.get().at(now));
        let damage = self.flush();
        if damage != Damage::None {
            self.renderer.get_mut().prepare_for(
                self.tree.borrow().frame(),
                RasterMode::Replay,
                &Default::default(),
            );
        }
        damage
    }

    fn flush(&self) -> Damage {
        let v = self.model.viewport.get();
        self.tree
            .borrow_mut()
            .update_at(v.size, v.scale, self.time.get())
            .expect("valid shell view")
    }

    pub fn record(&self, paints: impl IntoIterator<Item = (Key, Vec<retinue_render::Command>)>) {
        for (key, commands) in paints {
            self.model.paint(key).set(DrawList::from(commands));
        }
        self.flush();
    }

    pub fn set_motion_enabled(&mut self, enabled: bool) {
        self.tree.get_mut().set_motion_enabled(enabled);
    }
    pub fn deadline(&self, _: Instant, _: Duration) -> Option<Instant> {
        self.tree.borrow().next_deadline()
    }
    pub fn rail_animating(&self, _: Key) -> bool {
        let tree = self.tree.borrow();
        tree.unique(BODY)
            .expect("one shell body")
            .is_some_and(|id| tree.animating(id))
    }
    pub fn rect(&self, key: Key) -> Rect {
        let tree = self.tree.borrow();
        tree.unique(key)
            .expect("unique shell name")
            .and_then(|id| tree.geometry(id))
            .map(|geometry| geometry.hit)
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

    pub fn invalidate(&self, key: Key, damage: Damage) -> Damage {
        let tree = self.tree.borrow();
        tree.unique(key)
            .expect("unique shell name")
            .and_then(|id| tree.geometry(id))
            .map_or(Damage::None, |node| {
                tree.scene().invalidate(node.key, damage)
            })
    }
    pub fn hit_test(&self, position: (f64, f64)) -> Option<Key> {
        let tree = self.tree.borrow();
        let scale = tree.frame().scale();
        tree.hit_test((position.0 as f32 / scale, position.1 as f32 / scale))
    }

    pub fn pointer(
        &self,
        position: (f64, f64),
        phase: retinue_ui::input::PointerPhase,
    ) -> (Option<Control>, Option<Control>) {
        use retinue_ui::{input::Event, input::PointerId};
        let mut tree = self.tree.borrow_mut();
        let scale = tree.frame().scale();
        let result = tree.dispatch(&Event::Pointer {
            id: PointerId::MOUSE,
            phase,
            position: (position.0 as f32 / scale, position.1 as f32 / scale),
            modifiers: Default::default(),
        });
        let control = |id: Option<retinue_ui::runtime::ViewId>| {
            id.and_then(|id| tree.input_key(id))
                .and_then(|key| CONTROLS.into_iter().find(|c| control_key(*c) == key))
        };
        (control(result.target), control(result.activated))
    }
    pub fn window_action(
        &self,
        position: (f64, f64),
        capabilities: WindowCapabilities,
    ) -> Option<(Key, WindowAction)> {
        let tree = self.tree.borrow();
        let (key, action) = tree.scene().window_action(position, capabilities)?;
        Some((tree.authored_key(key)?, action))
    }
    pub fn declaration_report(&self) -> serde_json::Value {
        let tree = self.tree.borrow();
        serde_json::json!({
            "bodies_evaluated": tree.stats.bodies, "motion_samples": tree.stats.motion_samples,
            "mounted": tree.stats.mounted, "unmounted": tree.stats.unmounted,
            "pages": PAGES.map(|page| format!("{page:?}")),
            "nodes": tree.inspect().into_iter().map(|node| serde_json::json!({
                "id": node.id.0, "parent": node.parent.map(|id| id.0),
                "key": node.key.map(|key| (key.namespace, key.value)), "kind": node.kind,
                "dependencies": node.dependencies, "animating": node.animating,
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests;
