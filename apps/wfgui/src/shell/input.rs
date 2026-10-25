use super::*;
use retinue_ui::desktop::winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::{Key, NamedKey},
    window::{CursorIcon, Window},
};

impl Shell {
    pub(super) fn wheel_delta(&self, delta: MouseScrollDelta) -> (f32, bool) {
        match delta {
            MouseScrollDelta::LineDelta(_, y) => (-y * 60.0 * self.scale(), true),
            MouseScrollDelta::PixelDelta(delta) => (-delta.y as f32, false),
        }
    }

    fn hit(&self, position: (f64, f64)) -> Option<Control> {
        if self.settings.modal.is_some() || self.foundry.popup_open() || self.pages.ui.modal_open()
        {
            return None;
        }
        self.controls()
            .into_iter()
            .find(|(_, r, _)| r.contains(position.0, position.1))
            .map(|(c, _, _)| c)
    }

    fn page_position(&self) -> (f64, f64) {
        (
            self.pointer.0 - self.geometry.center.x as f64,
            self.pointer.1 - self.geometry.center.y as f64,
        )
    }

    fn dismiss_popup_press(&mut self) -> bool {
        let rail = (
            self.pointer.0 - self.geometry.right.x as f64,
            self.pointer.1 - self.geometry.right.y as f64,
        );
        if self.pages.dismiss_popups_at(self.page_position(), rail)
            || (self.page == Page::Foundry && self.foundry.dismiss_popup_at(self.page_position()))
        {
            self.pressed = None;
            self.page_pressed = false;
            self.rail_pressed = false;
            if self.page == Page::Foundry {
                self.focus = self.foundry.focus.map(Focus::Foundry);
            }
            true
        } else {
            false
        }
    }

    pub(super) fn open_context_menu(&mut self, keyboard: bool) -> bool {
        let opened = if keyboard {
            if self.rail_focus {
                return false;
            }
            if self.page == Page::Foundry {
                self.foundry.keyboard_context_menu()
            } else {
                self.pages.keyboard_context_menu()
            }
        } else {
            self.geometry
                .center
                .contains(self.pointer.0, self.pointer.1)
                && if self.page == Page::Foundry {
                    self.foundry.context_menu(self.page_position())
                } else {
                    self.pages.context_menu()
                }
        };
        if opened {
            self.rail_focus = false;
            self.pages.rail.focus = None;
            self.focus = if self.page == Page::Foundry {
                self.foundry.focus.map(Focus::Foundry)
            } else {
                None
            };
        }
        opened
    }

    pub(super) fn set_focus(&mut self, focus: Option<Focus>) {
        self.focus = focus;
        if focus.is_some() {
            self.rail_focus = false;
            self.pages.ui.focus = None;
            self.pages.rail.focus = None;
        }
        match focus {
            Some(Focus::Page(action)) => self.settings.focus_action(action),
            _ => self.settings.focus = None,
        }
        self.foundry.focus_action(match focus {
            Some(Focus::Foundry(action)) => Some(action),
            _ => None,
        });
    }

    fn focus_next(&mut self, backwards: bool) {
        self.focus_visible.keyboard_input();
        if self.settings.modal.is_some() {
            self.settings.focus_next(backwards);
            return;
        }
        if self.pages.ui.modal_open() {
            self.pages.ui.focus_next(backwards);
            return;
        }
        let controls = self.controls();
        let mut order: Vec<_> = [
            Control::Left,
            Control::Smaller,
            Control::Larger,
            Control::Right,
            Control::Minimize,
            Control::Maximize,
            Control::Close,
            Control::Settings,
            Control::Foundry,
            Control::Player,
            Control::Mastery,
            Control::Inventory,
            Control::Relics,
            Control::Market,
            Control::Builds,
        ]
        .into_iter()
        .filter(|c| {
            controls
                .iter()
                .any(|(control, _, enabled)| control == c && *enabled)
        })
        .map(Focus::Chrome)
        .collect();
        if self.page == Page::Settings {
            order.extend(
                self.settings
                    .buttons()
                    .into_iter()
                    .filter(|(_, _, enabled)| *enabled)
                    .map(|(a, _, _)| Focus::Page(a)),
            );
        } else if self.page == Page::Foundry {
            order.extend(
                self.foundry
                    .controls()
                    .into_iter()
                    .map(|(a, _)| Focus::Foundry(a)),
            );
            order.extend(
                self.foundry
                    .item_controls()
                    .into_iter()
                    .map(|(a, _)| Focus::Foundry(a)),
            );
        }
        let index = order.iter().position(|f| Some(*f) == self.focus);
        let next = match (index, backwards) {
            (Some(i), true) => (i + order.len() - 1) % order.len(),
            (Some(i), false) => (i + 1) % order.len(),
            (None, true) => order.len() - 1,
            (None, false) => 0,
        };
        self.set_focus(Some(order[next]));
    }

    pub fn resize_direction(&self, position: (f64, f64)) -> Option<ResizeDirection> {
        if self.maximized || self.fullscreen {
            return None;
        }
        let (x, y) = position;
        let edge = self.px(5.0) as f64;
        let corner = self.px(10.0) as f64;
        let w = self.size.0 as f64;
        let h = self.size.1 as f64;
        if x < 0.0 || y < 0.0 || x >= w || y >= h {
            return None;
        }
        use ResizeDirection::*;
        match (x < corner, x >= w - corner, y < corner, y >= h - corner) {
            (true, _, true, _) if x < edge || y < edge => Some(NorthWest),
            (_, true, true, _) if x >= w - edge || y < edge => Some(NorthEast),
            (true, _, _, true) if x < edge || y >= h - edge => Some(SouthWest),
            (_, true, _, true) if x >= w - edge || y >= h - edge => Some(SouthEast),
            _ if x < edge => Some(West),
            _ if x >= w - edge => Some(East),
            _ if y < edge => Some(North),
            _ if y >= h - edge => Some(South),
            _ => None,
        }
    }

    fn cursor(&self) -> CursorIcon {
        if self.foundry.popup_open() {
            return self.foundry.cursor();
        }
        use ResizeDirection::*;
        match self.resize_direction(self.pointer) {
            Some(North | South) => CursorIcon::NsResize,
            Some(East | West) => CursorIcon::EwResize,
            Some(NorthEast | SouthWest) => CursorIcon::NeswResize,
            Some(NorthWest | SouthEast) => CursorIcon::NwseResize,
            _ if self.hover.is_some_and(|hover| {
                self.controls()
                    .iter()
                    .any(|(c, _, enabled)| *c == hover && *enabled)
            }) || self.settings.hover.is_some() =>
            {
                CursorIcon::Pointer
            }
            _ if self.page == Page::Foundry
                && self
                    .geometry
                    .center
                    .contains(self.pointer.0, self.pointer.1) =>
            {
                self.foundry.cursor()
            }
            _ if self.geometry.right.contains(self.pointer.0, self.pointer.1) => {
                self.pages.rail.cursor()
            }
            _ if !matches!(self.page, Page::Foundry | Page::Settings)
                && self
                    .geometry
                    .center
                    .contains(self.pointer.0, self.pointer.1) =>
            {
                self.pages.ui.cursor()
            }
            _ => CursorIcon::Default,
        }
    }

    fn cancel_pointer(&mut self) {
        self.hover = None;
        self.pressed = None;
        self.page_pressed = false;
        self.tooltip = false;
        self.settings.cancel_pointer();
        self.foundry.cancel_pointer();
        self.pages.cancel_pointer();
        self.rail_pressed = false;
    }

    pub(super) fn pointer_move(&mut self, position: (f64, f64)) -> Damage {
        self.pointer = position;
        let hover = if self.resize_direction(position).is_none() {
            self.hit(position)
        } else {
            None
        };
        let changed = hover != self.hover;
        let old_hover = self.hover;
        let was_tooltip = self.tooltip;
        if changed {
            self.hover = hover;
            self.hover_since = Instant::now();
            self.tooltip = false;
        }
        let page = match self.page {
            Page::Settings => self.settings.pointer_move(self.page_position()),
            Page::Foundry => self.foundry.pointer_move(self.page_position()),
            _ => {
                if self.pages.pointer(false, self.page_position()) {
                    Damage::Full
                } else {
                    Damage::None
                }
            }
        };
        let rail = self.pages.pointer(
            true,
            (
                position.0 - self.geometry.right.x as f64,
                position.1 - self.geometry.right.y as f64,
            ),
        );
        if changed && was_tooltip {
            return Damage::Full;
        }
        let chrome = if changed {
            self.controls()
                .into_iter()
                .filter(|(c, _, _)| Some(*c) == old_hover || Some(*c) == hover)
                .fold(Damage::None, |damage, (control, rect, _)| {
                    let rect = if control.page().is_some() {
                        Rect {
                            x: self.geometry.left.x,
                            width: self.geometry.left.width,
                            ..rect
                        }
                    } else {
                        rect
                    };
                    damage.union(Damage::rect(rect.expanded(self.px(2.0))))
                })
        } else {
            Damage::None
        };
        chrome.union(self.page_damage(page)).union(if rail {
            Damage::rect(self.geometry.right)
        } else {
            Damage::None
        })
    }

    pub(super) fn pointer_button(&mut self, down: bool, now: Instant) -> Damage {
        if down {
            self.focus_visible.pointer_down();
            if self.dismiss_popup_press() {
                return Damage::Full;
            }
        }
        if (self.settings.modal.is_some() || self.pages.ui.modal_open())
            && !self.page_pressed
            && !self
                .geometry
                .center
                .contains(self.pointer.0, self.pointer.1)
        {
            return Damage::None;
        }
        if !down && self.pressed.is_some() {
            if let Some(control) = self
                .pressed
                .take()
                .filter(|c| Some(*c) == self.hit(self.pointer))
            {
                self.activate(control, now);
            }
            return Damage::Full;
        }
        if self.rail_pressed || self.geometry.right.contains(self.pointer.0, self.pointer.1) {
            self.rail_focus = true;
            self.rail_pressed = down;
            self.focus = None;
            self.pages.ui.focus = None;
            self.pages.button(true, down);
            return Damage::Full;
        }
        if self.page_pressed
            || self.foundry.popup_open()
            || self
                .geometry
                .center
                .contains(self.pointer.0, self.pointer.1)
        {
            if down {
                self.page_pressed = true;
                self.rail_focus = false;
                self.pages.rail.focus = None;
            }
            let damage = match self.page {
                Page::Settings => {
                    let damage = self.settings.pointer_button(self.page_position(), down);
                    self.focus = self.settings.focus.map(Focus::Page);
                    damage
                }
                Page::Foundry => {
                    let damage = self.foundry.pointer_button(self.page_position(), down);
                    self.focus = self.foundry.focus.map(Focus::Foundry);
                    damage
                }
                _ => {
                    self.focus = None;
                    self.pages.button(false, down);
                    Damage::Full
                }
            };
            if !down {
                self.page_pressed = false;
            }
            return self.page_damage(damage);
        }
        if down {
            self.pressed = self.hit(self.pointer).filter(|control| {
                self.controls()
                    .iter()
                    .any(|(c, _, enabled)| c == control && *enabled)
            });
            if let Some(control) = self.pressed {
                self.set_focus(Some(Focus::Chrome(control)));
            }
        } else if let Some(control) = self
            .pressed
            .take()
            .filter(|c| Some(*c) == self.hit(self.pointer))
        {
            self.activate(control, now);
        }
        Damage::Full
    }

    pub fn apply_window_action(&mut self, window: &Window) {
        match self.window_action.take() {
            Some(Control::Minimize) => window.set_minimized(true),
            Some(Control::Maximize) => window.set_maximized(!window.is_maximized()),
            Some(Control::Smaller | Control::Larger) => {
                let (width, height) = self.minimum_size();
                let minimum = retinue_ui::desktop::winit::dpi::LogicalSize::new(width, height);
                window.set_min_inner_size(Some(minimum));
                let size = window.inner_size().to_logical::<u32>(window.scale_factor());
                if !window.is_maximized() && (size.width < width || size.height < height) {
                    let _ = window.request_inner_size(
                        retinue_ui::desktop::winit::dpi::LogicalSize::new(
                            size.width.max(width),
                            size.height.max(height),
                        ),
                    );
                }
            }
            _ => (),
        }
    }

    pub fn event(&mut self, event: &WindowEvent, window: &Window) -> Damage {
        let previous = self.focus_visible;
        match event {
            WindowEvent::Focused(active) => {
                if self.window_focused && !active {
                    self.foundry.close_popup();
                    self.pages.dismiss_context_menu();
                    if self.page == Page::Foundry {
                        self.focus = self.foundry.focus.map(Focus::Foundry);
                    }
                }
                self.window_focused = *active;
                self.focus_visible.set_active(*active);
                self.pages.ui.focus_visible.set_active(*active);
                self.pages.rail.focus_visible.set_active(*active);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => self.focus_visible.pointer_down(),
            WindowEvent::Touch(touch) if touch.phase == TouchPhase::Started => {
                self.focus_visible.pointer_down()
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed => {
                if event.logical_key.as_ref() == Key::Named(NamedKey::F6) {
                    if self.pages.ui.modal_open() {
                        self.rail_focus = false;
                        self.focus = None;
                        self.pages.ui.focus_next(false);
                        return Damage::Full;
                    }
                    if self.rail_focus {
                        self.set_focus(Some(Focus::Chrome(Control::Foundry)));
                    } else if self.focus.is_some_and(|f| matches!(f, Focus::Chrome(_))) {
                        self.set_focus(None);
                        match self.page {
                            Page::Foundry => {
                                self.foundry
                                    .key(Key::Named(NamedKey::Tab), Default::default());
                                self.focus = self.foundry.focus.map(Focus::Foundry);
                            }
                            Page::Settings => {
                                self.settings.focus_next(false);
                                self.focus = self.settings.focus.map(Focus::Page);
                            }
                            _ => self.pages.ui.focus_next(false),
                        }
                    } else {
                        self.set_focus(None);
                        self.rail_focus = true;
                        self.pages.ui.focus = None;
                        self.pages.rail.focus_next(false);
                    }
                    return Damage::Full;
                }
                self.note_keyboard(event.logical_key.as_ref())
            }
            _ => (),
        }
        let damage = self.handle_event(event, window);
        if previous != self.focus_visible {
            Damage::Full
        } else {
            damage
        }
    }

    pub(super) fn note_keyboard(&mut self, key: Key<&str>) {
        if !self
            .modifiers
            .intersects(ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SUPER)
            && !matches!(
                key,
                Key::Named(
                    NamedKey::Shift
                        | NamedKey::Control
                        | NamedKey::Alt
                        | NamedKey::AltGraph
                        | NamedKey::Super
                        | NamedKey::Meta
                )
            )
        {
            self.focus_visible.keyboard_input();
        }
    }

    fn handle_event(&mut self, event: &WindowEvent, window: &Window) -> Damage {
        let now = Instant::now();
        self.maximized = window.is_maximized();
        self.fullscreen = window.fullscreen().is_some();
        let damage = match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                window.set_cursor(self.cursor());
                Damage::Full
            }
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = self.cursor();
                let damage = self.pointer_move((position.x, position.y));
                if cursor != self.cursor() {
                    window.set_cursor(self.cursor());
                }
                damage
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => {
                if self.dismiss_popup_press() {
                    return Damage::Full;
                }
                if self.open_context_menu(false) {
                    Damage::rect(self.geometry.center)
                } else {
                    Damage::None
                }
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                if matches!(event, WindowEvent::Focused(false)) {
                    self.modifiers = ModifiersState::empty();
                }
                self.cancel_pointer();
                self.last_title_press = None;
                Damage::Full
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                Damage::None
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let down = *state == ElementState::Pressed;
                if down && !self.foundry.popup_open() {
                    if let Some(direction) = self.resize_direction(self.pointer) {
                        self.cancel_pointer();
                        if let Err(error) = window.drag_resize_window(direction) {
                            eprintln!("wfgui: cannot resize window: {error}");
                        }
                        return Damage::None;
                    }
                    if self.settings.modal.is_none()
                        && self.geometry.title.contains(self.pointer.0, self.pointer.1)
                        && self.hit(self.pointer).is_none()
                    {
                        let double = self.last_title_press.is_some_and(|(time, p)| {
                            now.saturating_duration_since(time) <= Duration::from_millis(450)
                                && (p.0 - self.pointer.0).hypot(p.1 - self.pointer.1)
                                    <= self.px(5.0) as f64
                        });
                        self.last_title_press = if double {
                            None
                        } else {
                            Some((now, self.pointer))
                        };
                        if double {
                            window.set_maximized(!window.is_maximized());
                        } else if let Err(error) = window.drag_window() {
                            eprintln!("wfgui: cannot move window: {error}");
                        }
                        return Damage::None;
                    }
                }
                self.pointer_button(down, now)
            }
            WindowEvent::MouseWheel { delta, .. }
                if self
                    .geometry
                    .center
                    .contains(self.pointer.0, self.pointer.1)
                    && !self.geometry.right.contains(self.pointer.0, self.pointer.1) =>
            {
                let (delta, smooth) = self.wheel_delta(*delta);
                let damage = match self.page {
                    Page::Settings => {
                        self.settings.wheel(delta, smooth);
                        self.settings.tick(now)
                    }
                    Page::Foundry => {
                        self.foundry.wheel(delta, smooth);
                        self.foundry.tick(now)
                    }
                    _ => {
                        self.pages.wheel(false, delta, smooth);
                        self.pages.tick(now).0
                    }
                };
                self.page_damage(damage)
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.geometry.right.contains(self.pointer.0, self.pointer.1) =>
            {
                let (delta, smooth) = self.wheel_delta(*delta);
                self.pages.wheel(true, delta, smooth);
                self.pages.tick(now);
                Damage::rect(self.geometry.right)
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed => {
                if event.logical_key.as_ref() == Key::Named(NamedKey::ContextMenu)
                    || (self.modifiers.shift_key()
                        && event.logical_key.as_ref() == Key::Named(NamedKey::F10))
                {
                    return if self.open_context_menu(true) {
                        Damage::rect(self.geometry.center)
                    } else {
                        Damage::None
                    };
                }
                if (self.rail_focus
                    || (!matches!(self.page, Page::Foundry | Page::Settings)
                        && self.focus.is_none()))
                    && self
                        .pages
                        .key(self.rail_focus, event.logical_key.as_ref(), self.modifiers)
                {
                    return Damage::Full;
                }
                if event.repeat
                    && self.foundry.popup_open()
                    && matches!(
                        event.logical_key.as_ref(),
                        Key::Named(NamedKey::Enter | NamedKey::Space) | Key::Character(" ")
                    )
                {
                    return Damage::None;
                }
                if self.page == Page::Foundry
                    && self.foundry.key(event.logical_key.as_ref(), self.modifiers)
                {
                    self.focus = self.foundry.focus.map(Focus::Foundry);
                    return self.page_damage(Damage::Full);
                }
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Tab) => self.focus_next(self.modifiers.shift_key()),
                    Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                        if self.settings.modal.is_some() {
                            if let Some(action) = self.settings.focus {
                                self.settings.activate(action);
                            }
                        } else {
                            match self.focus {
                                Some(Focus::Chrome(control)) => self.activate(control, now),
                                Some(Focus::Page(action)) => self.settings.activate(action),
                                Some(Focus::Foundry(action)) => {
                                    self.foundry.activate(action);
                                    self.focus = self.foundry.focus.map(Focus::Foundry);
                                }
                                _ => (),
                            }
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.settings.activate(settings::Action::Cancel)
                    }
                    Key::Named(
                        NamedKey::PageDown
                        | NamedKey::PageUp
                        | NamedKey::ArrowDown
                        | NamedKey::ArrowUp
                        | NamedKey::Home
                        | NamedKey::End,
                    ) if self.page == Page::Foundry => {
                        let delta = match event.logical_key.as_ref() {
                            Key::Named(NamedKey::PageDown) => {
                                self.geometry.center.height as f32 * 0.8
                            }
                            Key::Named(NamedKey::PageUp) => {
                                -(self.geometry.center.height as f32) * 0.8
                            }
                            Key::Named(NamedKey::ArrowDown) => 48.0 * self.scale(),
                            Key::Named(NamedKey::ArrowUp) => -48.0 * self.scale(),
                            Key::Named(NamedKey::Home) => -f32::MAX,
                            _ => f32::MAX,
                        };
                        self.foundry.wheel(delta, true);
                    }
                    Key::Named(NamedKey::PageDown) => self
                        .settings
                        .wheel(self.geometry.center.height as f32 * 0.8, true),
                    Key::Named(NamedKey::PageUp) => self
                        .settings
                        .wheel(-(self.geometry.center.height as f32) * 0.8, true),
                    Key::Character("r") if self.modifiers.control_key() => {
                        if self.page == Page::Foundry {
                            self.foundry.activate(foundry::Action::Refresh);
                        } else {
                            self.settings.activate(settings::Action::Refresh);
                        }
                    }
                    _ => return Damage::None,
                }
                Damage::Full
            }
            _ => Damage::None,
        };
        self.apply_window_action(window);
        damage
    }
}
