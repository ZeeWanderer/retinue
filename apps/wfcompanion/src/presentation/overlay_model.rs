#[derive(Clone, Debug, PartialEq)]
pub(super) enum Scene {
    Relic {
        content: crate::relic::Scene,
        view: RelicViewState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RelicViewState {
    pub(super) suggestion_offset: usize,
    pub(super) interaction_active: bool,
    pub(super) close_hovered: bool,
}

#[derive(Default)]
pub(super) struct Presentation {
    suggestion_offset: usize,
    close_hovered: bool,
    scroll_accumulator: f64,
}

impl Presentation {
    pub(super) fn scene(&self, content: crate::relic::Scene, interaction_active: bool) -> Scene {
        Scene::Relic {
            content,
            view: RelicViewState {
                suggestion_offset: self.suggestion_offset,
                interaction_active,
                close_hovered: self.close_hovered,
            },
        }
    }

    pub(super) fn reset_suggestions(&mut self) {
        self.suggestion_offset = 0;
        self.close_hovered = false;
        self.scroll_accumulator = 0.0;
    }

    pub(super) fn interaction_changed(&mut self) {
        self.close_hovered = false;
        self.scroll_accumulator = 0.0;
    }

    pub(super) fn close_hovered(&self) -> bool {
        self.close_hovered
    }

    pub(super) fn set_close_hovered(&mut self, hovered: bool) -> bool {
        if hovered == self.close_hovered {
            return false;
        }
        self.close_hovered = hovered;
        true
    }

    pub(super) fn scroll_suggestions(&mut self, item_count: usize, delta: f64) -> bool {
        self.scroll_accumulator += delta;
        let rows = self.scroll_accumulator.trunc() as isize;
        if rows == 0 {
            return false;
        }
        self.scroll_accumulator -= rows as f64;
        let next = self
            .suggestion_offset
            .saturating_add_signed(rows.saturating_mul(2))
            .min(max_suggestion_offset(item_count));
        if next == self.suggestion_offset {
            return false;
        }
        self.suggestion_offset = next;
        true
    }
}

pub(super) fn max_suggestion_offset(item_count: usize) -> usize {
    item_count.saturating_sub(3) / 2 * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_scroll_stays_on_complete_rows() {
        assert_eq!(max_suggestion_offset(0), 0);
        assert_eq!(max_suggestion_offset(4), 0);
        assert_eq!(max_suggestion_offset(5), 2);
        assert_eq!(max_suggestion_offset(6), 2);
        assert_eq!(max_suggestion_offset(7), 4);
        assert_eq!(max_suggestion_offset(32), 28);
    }

    #[test]
    fn fractional_scroll_accumulates_without_changing_page() {
        let mut state = Presentation::default();
        assert!(!state.scroll_suggestions(8, 0.4));
        assert!(!state.scroll_suggestions(8, 0.4));
        assert!(state.scroll_suggestions(8, 0.4));
        let Scene::Relic { view, .. } = state.scene(crate::relic::Scene::Reading, false);
        assert_eq!(view.suggestion_offset, 2);
    }
}
