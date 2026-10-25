use super::*;
use retinue_ui::transition::{Easing, ScalarTransition};

const MOTION: Duration = Duration::from_millis(170);

pub(super) struct CategoryMotion {
    pub weights: [f32; 8],
    from: [f32; 8],
    target: usize,
    progress: ScalarTransition,
}

impl CategoryMotion {
    pub fn new(selected: usize) -> Self {
        let weights = std::array::from_fn(|i| if i == selected { 1.0 } else { 0.0 });
        Self {
            weights,
            from: weights,
            target: selected,
            progress: ScalarTransition::default(),
        }
    }

    pub fn select(&mut self, selected: usize, now: Instant, animate: bool) {
        self.sample(now);
        self.from = self.weights;
        self.target = selected;
        self.progress.set(0.0, now, Duration::ZERO);
        self.progress.set_eased(
            1.0,
            now,
            if animate { MOTION } else { Duration::ZERO },
            Easing::InOutCubic,
        );
        self.sample(now);
    }

    pub fn sample(&mut self, now: Instant) -> bool {
        let progress = self.progress.value(now);
        let weights = std::array::from_fn(|i| {
            self.from[i] + ((if i == self.target { 1.0 } else { 0.0 }) - self.from[i]) * progress
        });
        let changed = weights != self.weights;
        self.weights = weights;
        changed
    }

    pub fn deadline(&self, now: Instant) -> Option<Instant> {
        self.progress.deadline(now, Duration::from_micros(8_333))
    }
}

impl Foundry {
    pub fn select_category(&mut self, category: usize, now: Instant) {
        let category = category.min(7);
        if self.category == category {
            return;
        }
        self.category_motion.select(category, now, true);
        self.category = category;
        self.filter(true);
    }

    pub fn controls(&self) -> Vec<(Action, Rect)> {
        let mut controls = Vec::new();
        let left = self.px(10.0);
        let right = self.size.0.saturating_sub(left);
        let available = right.saturating_sub(left);
        let base = self.px(36.0);
        let gap = self.px(4.0);
        let extras: [u32; 8] = std::array::from_fn(|i| {
            self.px(44.0 + text_width(&self.regular, CATEGORIES[i], 14.0))
                .saturating_sub(base)
        });
        let mut y = self.px(12.0);
        let mut start = 0;
        let mut reserved = 0;
        while start < CATEGORIES.len() {
            let mut end = start;
            let mut extra = 0;
            // Reserve the widest possible selection per row; motion must not rewrap the toolbar.
            while end < CATEGORIES.len() {
                let next_extra = extra.max(extras[end]);
                let count = (end - start + 1) as u32;
                let width = base * count + gap * (count - 1) + next_extra;
                if end > start && width > available {
                    break;
                }
                reserved = width;
                extra = next_extra;
                end += 1;
            }
            let mut x = left;
            let mut position = left as f32;
            for (i, extra) in extras.iter().enumerate().take(end).skip(start) {
                let edge =
                    (position + base as f32 + *extra as f32 * self.category_motion.weights[i])
                        .round() as u32;
                controls.push((
                    Action::Category(i),
                    Rect {
                        x,
                        y,
                        width: edge.min(right).saturating_sub(x),
                        height: self.px(32.0),
                    },
                ));
                position +=
                    base as f32 + *extra as f32 * self.category_motion.weights[i] + gap as f32;
                x = position.round() as u32;
            }
            start = end;
            if end < CATEGORIES.len() {
                y += self.px(38.0);
            }
        }
        let x = if available.saturating_sub(reserved + gap) < self.px(292.0) {
            y += self.px(38.0);
            left
        } else {
            left + reserved + gap
        };
        let search_end = right.saturating_sub(self.px(84.0));
        let search_width = self
            .px(self.search_motion.width)
            .min(search_end.saturating_sub(x));
        let search = Rect {
            x: search_end.saturating_sub(search_width),
            y,
            width: search_width,
            height: self.px(32.0),
        };
        controls.extend([
            (
                Action::Refresh,
                Rect {
                    x: right.saturating_sub(self.px(32.0)),
                    y,
                    width: self.px(32.0),
                    height: self.px(32.0),
                },
            ),
            (Action::Search, search),
            (
                Action::Filters,
                Rect {
                    x: right.saturating_sub(self.px(74.0)),
                    y,
                    width: self.px(32.0),
                    height: self.px(32.0),
                },
            ),
        ]);
        if !self.search.borrow().text().is_empty() && search.width >= self.px(80.0) {
            controls.push((
                Action::ClearSearch,
                Rect {
                    x: search_end.saturating_sub(self.px(30.0)),
                    y,
                    width: self.px(28.0),
                    height: search.height,
                },
            ));
        }
        controls
    }
}
