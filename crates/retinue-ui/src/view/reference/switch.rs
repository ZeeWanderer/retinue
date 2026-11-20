use super::{Element, IntoElement, LayoutView, View, ViewContext, ViewExt, ZStack};
use crate::{
    Bounds, Key,
    layout::{Layout, LayoutError, LayoutSize, Proposal, Subviews},
    motion::{ReduceMotion, TranslationTransition, switch::ContentState},
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, PartialEq)]
struct Page {
    key: Key,
    content: Element,
}

/// Keeps the current subtree and, while transitioning, at most one outgoing subtree.
#[derive(Clone, Default, PartialEq)]
pub struct Switch {
    selected: Option<Page>,
    transition: Option<TranslationTransition>,
}

impl Switch {
    pub fn new(key: impl Into<Key>, content: impl IntoElement) -> Self {
        Self {
            selected: Some(Page {
                key: key.into(),
                content: content.into_element(),
            }),
            transition: None,
        }
    }

    pub fn vacant() -> Self {
        Self::default()
    }

    pub fn transition(mut self, transition: TranslationTransition) -> Self {
        assert!(transition.valid(), "invalid switch transition");
        self.transition = Some(transition);
        self
    }
}

struct Retained {
    motion: ContentState,
    selected: Option<Page>,
    departing: Option<Page>,
}

impl View for Switch {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        let key = self.selected.as_ref().map(|page| page.key);
        let retained = cx.owned("switch", || {
            Rc::new(RefCell::new(Retained {
                motion: ContentState::new(key),
                selected: self.selected.clone(),
                departing: None,
            }))
        });
        let enabled = cx.motion_enabled
            && !cx.environment().get::<ReduceMotion>()
            && !cx
                .mutation()
                .is_some_and(|mutation| mutation.transaction.disables_animations);
        let now = cx.time.instant;
        let mut retained = retained.borrow_mut();
        retained.motion.select(key, self.transition, now, enabled);
        retained.departing = retained.motion.departing.and_then(|key| {
            retained
                .selected
                .iter()
                .chain(retained.departing.iter())
                .find(|page| page.key == key)
                .cloned()
        });
        retained.selected = self.selected.clone();
        cx.deadlines.motion |= retained.motion.moving();
        cx.deadlines
            .request(now, retained.motion.deadline(now, cx.frame_interval));
        let children = retained
            .departing
            .iter()
            .chain(retained.selected.iter())
            .map(|page| {
                let offset = retained.motion.offset(page.key, now);
                ZStack::new((page.content.clone(),))
                    .offset(offset.0, offset.1)
                    .hit_testing(Some(page.key) == key)
                    .id(page.key)
            })
            .collect::<Vec<_>>();
        LayoutView::new(SwitchLayout { selected: key }, children)
    }
}

#[derive(PartialEq)]
struct SwitchLayout {
    selected: Option<Key>,
}

impl Layout for SwitchLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let selected = self
            .selected
            .and_then(|key| children.index(key))
            .or_else(|| (!children.is_empty()).then_some(0));
        selected.map_or(Ok(LayoutSize::ZERO), |index| {
            children.measure(index, proposal)
        })
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        for index in 0..children.len() {
            children.place(index, bounds)?;
        }
        Ok(())
    }
}
