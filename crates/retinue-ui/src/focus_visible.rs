/// Input modality is window-wide; changing the focused widget does not reset it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusVisible {
    keyboard: bool,
    active: bool,
}

impl Default for FocusVisible {
    fn default() -> Self {
        Self {
            keyboard: true,
            active: true,
        }
    }
}

impl FocusVisible {
    pub fn pointer_down(&mut self) {
        self.keyboard = false;
    }

    pub fn keyboard_input(&mut self) {
        self.keyboard = true;
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn shows(self, focused: bool, text_input: bool) -> bool {
        self.active && focused && (self.keyboard || text_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_focus_is_quiet_but_text_editing_and_keyboard_focus_remain_visible() {
        let mut focus = FocusVisible::default();
        assert!(!focus.shows(false, true));
        assert!(focus.shows(true, false));
        focus.pointer_down();
        assert!(!focus.shows(true, false));
        assert!(focus.shows(true, true));
        focus.keyboard_input();
        assert!(focus.shows(true, false));
    }

    #[test]
    fn window_activation_preserves_modality_without_showing_inactive_focus() {
        for keyboard in [false, true] {
            let mut focus = FocusVisible::default();
            if !keyboard {
                focus.pointer_down();
            }
            focus.set_active(false);
            assert!(!focus.shows(true, false));
            assert!(!focus.shows(true, true));
            focus.set_active(true);
            assert_eq!(focus.shows(true, false), keyboard);
        }
    }
}
