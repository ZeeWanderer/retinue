use super::{
    Activation, Button, Event, EventContext, FocusPolicy, Key, Phase, PointerId, PointerPhase,
    Requests, Target,
};
use crate::{focus_visible::FocusVisible, runtime::ViewId};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dispatch {
    pub handled: bool,
    pub focus_changed: bool,
    pub capture_changed: bool,
    pub target: Option<ViewId>,
    pub activated: Option<ViewId>,
}

pub struct Router {
    targets: Vec<Target>,
    index: HashMap<ViewId, usize>,
    focus: Option<ViewId>,
    capture: HashMap<PointerId, ViewId>,
    positions: HashMap<PointerId, (f32, f32)>,
    outside: HashSet<PointerId>,
    buttons: HashMap<PointerId, Vec<Button>>,
    pressed: HashMap<(PointerId, Button), ViewId>,
    hovered: HashSet<ViewId>,
    modality: FocusVisible,
    active: bool,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            index: HashMap::default(),
            focus: None,
            capture: HashMap::default(),
            positions: HashMap::default(),
            outside: HashSet::default(),
            buttons: HashMap::default(),
            pressed: HashMap::default(),
            hovered: HashSet::default(),
            modality: FocusVisible::default(),
            active: true,
        }
    }
}

impl Router {
    pub fn focused(&self) -> Option<ViewId> {
        self.focus
    }
    pub fn captured(&self, pointer: PointerId) -> Option<ViewId> {
        self.capture.get(&pointer).copied()
    }
    pub fn target(&self, id: ViewId) -> Option<&Target> {
        self.index.get(&id).map(|&index| &self.targets[index])
    }
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    pub fn hit_test(&self, position: (f32, f32)) -> Option<ViewId> {
        self.targets
            .iter()
            .rev()
            .find(|target| target.contains(position))
            .map(|target| target.id)
    }

    pub fn update(&mut self, targets: Vec<Target>) {
        let placement_only = targets.len() == self.targets.len()
            && targets.iter().zip(&self.targets).all(|(new, old)| {
                new.id == old.id
                    && new.parent == old.parent
                    && new.enabled == old.enabled
                    && new.input == old.input
            });
        if placement_only {
            self.targets = targets;
            self.refresh_hover();
            self.publish_status();
            return;
        }
        let index: HashMap<_, _> = targets
            .iter()
            .enumerate()
            .map(|(i, target)| (target.id, i))
            .collect();
        assert_eq!(index.len(), targets.len(), "duplicate input identity");
        let enabled = |id: ViewId| index.get(&id).is_some_and(|&i| targets[i].enabled);
        let lost: Vec<_> = self
            .capture
            .iter()
            .filter(|(_, id)| !enabled(**id))
            .map(|(&pointer, _)| pointer)
            .collect();
        for pointer in lost {
            self.release(pointer);
        }
        self.pressed.retain(|_, id| enabled(*id));
        if self
            .focus
            .is_some_and(|id| !enabled(id) || targets[index[&id]].input.focus == FocusPolicy::None)
        {
            self.set_focus(None);
        }
        for &id in &self.hovered {
            if !enabled(id) {
                self.notify(id, Event::Hovered(false));
            }
        }
        self.hovered.retain(|id| enabled(*id));
        for old in &self.targets {
            let retained_status = index
                .get(&old.id)
                .and_then(|&i| targets[i].input.interaction.as_ref());
            if let Some(status) = &old.input.interaction {
                if Some(status) != retained_status {
                    status.reset();
                }
            }
        }
        self.targets = targets;
        self.index = index;
        self.refresh_hover();
        self.publish_status();
    }

    pub fn focus(&mut self, id: Option<ViewId>) -> bool {
        let old = self.focus;
        self.set_focus(id);
        self.publish_status();
        old != self.focus
    }

    pub fn dispatch(&mut self, event: &Event) -> Dispatch {
        let old_focus = self.focus;
        let old_capture = self.capture.clone();
        let mut delivered = None;
        let mut activated = None;
        let handled = match event {
            Event::WindowFocused(active) => {
                self.set_active(*active);
                false
            }
            Event::Pointer {
                id,
                phase,
                position,
                ..
            } if self.active => {
                if !position.0.is_finite() || !position.1.is_finite() {
                    return Dispatch::default();
                }
                match phase {
                    PointerPhase::Leave => {
                        self.outside.insert(*id);
                    }
                    PointerPhase::Move | PointerPhase::Down(_) => {
                        self.outside.remove(id);
                    }
                    _ => {}
                }
                let previous = (!self.outside.contains(id))
                    .then(|| self.hit_test(*position))
                    .flatten();
                if matches!(phase, PointerPhase::Cancel | PointerPhase::Leave) {
                    self.positions.remove(id);
                } else if !self.outside.contains(id) {
                    self.positions.insert(*id, *position);
                }
                self.refresh_hover();
                if let PointerPhase::Down(button) = phase {
                    self.modality.pointer_down();
                    let buttons = self.buttons.entry(*id).or_default();
                    if !buttons.contains(button) {
                        buttons.push(*button);
                    }
                }
                let target = self.captured(*id).or(previous);
                delivered = target;
                let handled = self.route(target, event);
                if let PointerPhase::Down(Button::Primary) = phase {
                    if let Some(target) = target.filter(|target| {
                        self.target(*target)
                            .is_some_and(|t| t.enabled && t.input.activation != Activation::None)
                    }) {
                        self.pressed.insert((*id, Button::Primary), target);
                        self.capture.entry(*id).or_insert(target);
                        if self.target(target).unwrap().input.activation == Activation::Press {
                            activated = Some(target);
                        }
                    }
                }
                if matches!(phase, PointerPhase::Down(Button::Primary)) && !handled {
                    let focus = target.and_then(|id| {
                        self.path(id).into_iter().find(|id| {
                            self.target(*id)
                                .is_some_and(|target| target.input.focus != FocusPolicy::None)
                        })
                    });
                    self.set_focus(focus);
                }
                match phase {
                    PointerPhase::Up(button) => {
                        if let Some(pressed) = self.pressed.remove(&(*id, *button)) {
                            if previous == Some(pressed)
                                && self.target(pressed).is_some_and(|t| {
                                    t.enabled && t.input.activation == Activation::Release
                                })
                            {
                                activated = Some(pressed);
                            }
                        }
                        if let Some(buttons) = self.buttons.get_mut(id) {
                            buttons.retain(|b| b != button);
                        }
                        if self.buttons.get(id).is_none_or(Vec::is_empty) {
                            self.buttons.remove(id);
                            self.release(*id);
                        }
                    }
                    PointerPhase::Cancel => {
                        self.outside.remove(id);
                        self.pressed.retain(|(pointer, _), _| pointer != id);
                        self.buttons.remove(id);
                        self.release(*id);
                    }
                    _ => {}
                }
                handled
            }
            Event::Scroll { position, .. } if self.active => {
                self.route(self.hit_test(*position), event)
            }
            Event::Key {
                key,
                pressed,
                modifiers,
                ..
            } if self.active => {
                if *pressed {
                    self.modality.keyboard_input();
                }
                let handled = self.route(self.focus, event);
                if !handled
                    && *pressed
                    && *key == Key::Tab
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.super_key
                {
                    self.focus_next(modifiers.shift)
                } else {
                    handled
                }
            }
            Event::Text(_) | Event::Preedit { .. } if self.active => self.route(self.focus, event),
            _ => false,
        };
        self.publish_status();
        Dispatch {
            handled,
            focus_changed: old_focus != self.focus,
            capture_changed: old_capture != self.capture,
            target: delivered,
            activated,
        }
    }

    fn path(&self, mut id: ViewId) -> Vec<ViewId> {
        let mut path = Vec::new();
        while let Some(target) = self.target(id) {
            // Input descriptions are a tree; guard a malformed low-level host graph.
            if path.len() >= self.targets.len() {
                break;
            }
            path.push(id);
            match target.parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
        path
    }

    fn route(&mut self, target: Option<ViewId>, event: &Event) -> bool {
        let Some(target) = target else {
            return false;
        };
        let path = self.path(target);
        let visits = path
            .iter()
            .rev()
            .filter(|id| **id != target)
            .map(|id| (*id, Phase::Capture))
            .chain(std::iter::once((target, Phase::Target)))
            .chain(path.iter().skip(1).map(|id| (*id, Phase::Bubble)));
        let mut handled = false;
        for (id, phase) in visits {
            let Some(node) = self.target(id) else {
                continue;
            };
            if !node.enabled {
                continue;
            }
            let Some(handler) = node.input.handler.clone() else {
                continue;
            };
            let mut context = EventContext {
                target,
                current: id,
                phase,
                bounds: node.bounds,
                position: event.position(),
                pointer: event.pointer(),
                requests: Requests::default(),
            };
            handler.call(event, &mut context);
            handled |= context.requests.handled;
            if let Some(focus) = context.requests.focus {
                self.set_focus(focus);
            }
            if let Some((pointer, capture)) = context.requests.capture {
                if let Some(id) = capture {
                    if self.target(id).is_some_and(|target| target.enabled)
                        && self.captured(pointer) != Some(id)
                    {
                        self.release(pointer);
                        self.capture.insert(pointer, id);
                    }
                } else if self.captured(pointer) == Some(id) {
                    self.release(pointer);
                }
            }
            if context.requests.stopped {
                break;
            }
        }
        handled
    }

    fn notify(&self, id: ViewId, event: Event) {
        let Some(target) = self.target(id) else {
            return;
        };
        if let Some(handler) = &target.input.handler {
            let mut context = EventContext {
                target: id,
                current: id,
                phase: Phase::Target,
                bounds: target.bounds,
                position: event.position(),
                pointer: event.pointer(),
                requests: Requests::default(),
            };
            // Lifecycle notifications cannot recursively change focus or capture.
            handler.call(&event, &mut context);
        }
    }

    fn release(&mut self, pointer: PointerId) {
        if let Some(id) = self.capture.remove(&pointer) {
            self.notify(id, Event::CaptureLost(pointer));
        }
    }

    fn set_focus(&mut self, id: Option<ViewId>) {
        let id = id.filter(|id| {
            self.target(*id)
                .is_some_and(|t| t.enabled && t.input.focus != FocusPolicy::None)
        });
        if id == self.focus {
            return;
        }
        if self.active {
            if let Some(old) = self.focus {
                self.notify(old, Event::Focused(false));
            }
        }
        self.focus = id;
        if self.active {
            if let Some(id) = id {
                self.notify(id, Event::Focused(true));
            }
        }
    }

    fn focus_next(&mut self, reverse: bool) -> bool {
        let order: Vec<_> = self
            .targets
            .iter()
            .filter(|t| {
                t.enabled && t.input.focus.tab_stop() && t.clip.width > 0.0 && t.clip.height > 0.0
            })
            .map(|t| t.id)
            .collect();
        if order.is_empty() {
            return false;
        }
        let index = order.iter().position(|id| Some(*id) == self.focus);
        let next = match (index, reverse) {
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (Some(i), false) => (i + 1) % order.len(),
            (None, true) => order.len() - 1,
            (None, false) => 0,
        };
        self.set_focus(Some(order[next]));
        true
    }

    fn set_active(&mut self, active: bool) {
        if active == self.active {
            return;
        }
        self.active = active;
        self.modality.set_active(active);
        if !active {
            let captured: Vec<_> = self.capture.keys().copied().collect();
            for pointer in captured {
                let id = self.capture[&pointer];
                let position = self.positions.get(&pointer).copied().unwrap_or_default();
                self.notify(
                    id,
                    Event::Pointer {
                        id: pointer,
                        phase: PointerPhase::Cancel,
                        position,
                        modifiers: Default::default(),
                    },
                );
                self.release(pointer);
            }
            self.buttons.clear();
            self.pressed.clear();
            self.positions.clear();
            self.outside.clear();
            self.refresh_hover();
        }
        if let Some(id) = self.focus {
            self.notify(id, Event::Focused(active));
        }
    }

    fn refresh_hover(&mut self) {
        let hovered: HashSet<_> = self
            .positions
            .values()
            .filter_map(|&position| self.hit_test(position))
            .flat_map(|id| self.path(id))
            .collect();
        for &id in self.hovered.difference(&hovered) {
            self.notify(id, Event::Hovered(false));
        }
        for &id in hovered.difference(&self.hovered) {
            self.notify(id, Event::Hovered(true));
        }
        self.hovered = hovered;
    }

    fn publish_status(&self) {
        for target in &self.targets {
            if let Some(status) = &target.input.interaction {
                let focused = self.focus == Some(target.id);
                status.hovered.set(self.hovered.contains(&target.id));
                status.focused.set(focused && self.active);
                status.focus_visible.set(
                    self.modality
                        .shows(focused, target.input.focus == FocusPolicy::Text),
                );
                status
                    .captured
                    .set(self.capture.values().any(|id| *id == target.id));
            }
        }
    }
}

impl Drop for Router {
    fn drop(&mut self) {
        for target in &self.targets {
            if let Some(status) = &target.input.interaction {
                status.reset();
            }
        }
    }
}
