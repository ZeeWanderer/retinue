use super::Change;
use std::ops::{BitOr, BitOrAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Work(u8);

impl Work {
    pub const LAYOUT: Self = Self(1);
    pub const PAINT: Self = Self(2);
    pub const GEOMETRY: Self = Self(4);
    pub const INPUT: Self = Self(8);
    pub const STRUCTURE: Self = Self(16);
    pub const RENDER_CONTEXT: Self = Self(32);
    pub const ALL: Self = Self(63);

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn for_change(change: &Change) -> Self {
        match change {
            Change::Layout(..)
            | Change::LayoutValues(..)
            | Change::Placement(..)
            | Change::LayoutKey(..) => Self::LAYOUT,
            Change::Paint(..) => Self::PAINT,
            Change::Offset(..) | Change::Clip(..) => Self::GEOMETRY,
            Change::Interactive(..)
            | Change::HitTesting(..)
            | Change::Role(..)
            | Change::WindowAction(..) => Self::INPUT,
            Change::Reuse(..) => Self::STRUCTURE,
        }
    }
}

impl BitOr for Work {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Work {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Default)]
pub(super) struct Changes(Vec<(usize, Work)>);

impl Changes {
    pub fn mark(&mut self, slot: usize, work: Work) {
        if work != Work::default() {
            self.0.push((slot, work));
        }
    }

    pub fn coalesce(&mut self) {
        self.0.sort_unstable_by_key(|&(slot, _)| slot);
        self.0.dedup_by(|a, b| {
            if a.0 == b.0 {
                b.1 |= a.1;
                true
            } else {
                false
            }
        });
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, index: usize) -> (usize, Work) {
        self.0[index]
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, Work)> + '_ {
        self.0.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Element, Paint, RecordingMode, Scene};
    use crate::{
        Bounds, Key,
        layout::{Layout, LayoutAlgorithm, LayoutError, LayoutSize, Proposal, Subviews},
        semantics::{Role, WindowAction},
    };
    use retinue_render::{Color, Command, damage::Damage};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn coalescing_preserves_all_dirty_domains() {
        let mut changes = Changes::default();
        changes.mark(5, Work::INPUT);
        changes.mark(2, Work::PAINT);
        changes.mark(5, Work::GEOMETRY);
        changes.mark(5, Work::LAYOUT);
        changes.mark(9, Work::default());
        changes.coalesce();
        assert_eq!(
            changes.iter().collect::<Vec<_>>(),
            [
                (2, Work::PAINT),
                (5, Work::INPUT | Work::GEOMETRY | Work::LAYOUT),
            ]
        );
        changes.coalesce();
        assert_eq!(changes.len(), 2);
        changes.clear();
        assert_eq!(changes.len(), 0);
    }

    fn painted(calls: &Rc<Cell<usize>>) -> Element {
        let mut element = Element::at(1, Bounds::new(0.0, 0.0, 80.0, 40.0));
        element.interactive = true;
        let calls = calls.clone();
        element.paint = Some(Paint::new(1, move |size| {
            calls.set(calls.get() + 1);
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                radius: 0.0,
                color: Color::rgb(0x123456),
            }]
        }));
        element
    }

    #[derive(PartialEq)]
    struct MeasuredPaint(u32);

    impl Layout for MeasuredPaint {
        type Cache = u32;
        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut u32,
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(80.0, 40.0))
        }
        fn place(
            &self,
            _: Bounds,
            _: &mut Subviews<'_, '_>,
            cache: &mut u32,
        ) -> Result<(), LayoutError> {
            *cache = self.0;
            Ok(())
        }
    }

    #[test]
    fn completed_layout_changes_repaint_even_when_outer_size_and_paint_key_do_not() {
        for reconcile in [false, true] {
            let mut element = Element::new(1, taffy::Style::default());
            element.algorithm =
                super::super::Algorithm::Custom(LayoutAlgorithm::new(MeasuredPaint(0x112233)));
            element.paint = Some(Paint::with_layout(1, |size, result| {
                vec![Command::Fill {
                    bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                    radius: 0.0,
                    color: Color::rgb(*result.get::<MeasuredPaint>().unwrap()),
                }]
            }));
            let mut scene = Scene::default();
            scene
                .update(element.clone(), (200, 100), 1.2, RecordingMode::Retained)
                .unwrap();
            let before = scene.frame.instances()[0].program.id();
            element.algorithm =
                super::super::Algorithm::Custom(LayoutAlgorithm::new(MeasuredPaint(0x445566)));
            let damage = if reconcile {
                scene
                    .update(element, (200, 100), 1.2, RecordingMode::Retained)
                    .unwrap()
            } else {
                scene
                    .patch([Change::Layout(1.into(), element.style, element.algorithm)])
                    .unwrap()
            };
            assert!(!matches!(damage, Damage::None));
            assert_ne!(scene.frame.instances()[0].program.id(), before);
            assert_eq!(scene.stats.recordings, 1);
            assert!(matches!(scene.frame.instances()[0].program.commands()[0],
                Command::Fill { color, .. } if color == Color::rgb(0x445566)));
            scene.resize((200, 100), 1.2).unwrap();
            assert_eq!(scene.stats.recordings, 0);
        }
    }

    #[test]
    fn input_only_changes_publish_new_hit_metadata_without_visual_work() {
        for reconcile in [false, true] {
            let calls = Rc::new(Cell::new(0));
            let mut root = painted(&calls);
            let mut scene = Scene::default();
            scene
                .update(root.clone(), (200, 100), 1.2, RecordingMode::Retained)
                .unwrap();
            let revision = scene.input_revision();
            let program = scene.frame.instances()[0].program.id();
            root.role = Role::Title;
            root.window_action = Some(WindowAction::Move);
            let damage = if reconcile {
                scene
                    .update(root, (200, 100), 1.2, RecordingMode::Retained)
                    .unwrap()
            } else {
                scene
                    .patch([
                        Change::Role(Key::from(1), Role::Title),
                        Change::WindowAction(Key::from(1), Some(WindowAction::Move)),
                    ])
                    .unwrap()
            };
            assert!(matches!(damage, Damage::None));
            assert_eq!(scene.input_revision(), revision + 1);
            assert_eq!(scene.stats.recordings, 0);
            assert_eq!(scene.stats.layout_computations, 0);
            assert_eq!(calls.get(), 1);
            assert_eq!(scene.frame.instances()[0].program.id(), program);
            let geometry = scene.bounds(Key::from(1)).unwrap();
            assert_eq!(geometry.role, Role::Title);
            assert_eq!(geometry.window_action, Some(WindowAction::Move));
        }
    }

    #[test]
    fn combined_content_transform_and_input_changes_keep_every_domain() {
        let calls = Rc::new(Cell::new(0));
        let mut root = painted(&calls);
        let mut scene = Scene::default();
        scene
            .update(root.clone(), (200, 100), 1.2, RecordingMode::Retained)
            .unwrap();
        let old = scene.frame.instances()[0].program.id();
        let recorded = calls.clone();
        root.paint = Some(Paint::new(2, move |size| {
            recorded.set(recorded.get() + 1);
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
                radius: 0.0,
                color: Color::rgb(0x654321),
            }]
        }));
        scene
            .patch([
                Change::Offset(Key::from(1), (10.0, 4.0)),
                Change::Paint(Key::from(1), root.paint),
                Change::HitTesting(Key::from(1), false),
            ])
            .unwrap();
        assert_eq!(scene.stats.layout_computations, 0);
        assert_eq!(scene.stats.recordings, 1);
        assert_eq!(calls.get(), 2);
        assert_ne!(scene.frame.instances()[0].program.id(), old);
        assert_eq!(scene.bounds(Key::from(1)).unwrap().layout.x, 10.0);
        assert_eq!(scene.hit_test((15.0, 10.0)), None);
    }

    #[test]
    fn whole_element_properties_match_sparse_patches_and_keep_children() {
        let root = painted(&Rc::new(Cell::new(0)))
            .with_children([Element::at(2, Bounds::new(2.0, 3.0, 8.0, 9.0))]);
        let mut whole = Scene::default();
        let mut sparse = Scene::default();
        for scene in [&mut whole, &mut sparse] {
            scene
                .update(root.clone(), (200, 100), 1.0, RecordingMode::Retained)
                .unwrap();
        }
        let mut property = root;
        property.children.clear();
        property.style.size.width = taffy::Dimension::length(95.0);
        property.offset = (12.0, 8.0);
        property.clip = true;
        property.interactive = false;
        property.hit_testing = false;
        property.role = Role::Title;
        property.window_action = Some(WindowAction::Move);
        property.cache_hint = retinue_render::CacheHint::Stable;
        property.paint = Some(Paint::new(2, |_| {
            vec![Command::Fill {
                bounds: Bounds::new(0.0, 0.0, 95.0, 40.0),
                radius: 3.0,
                color: Color::rgb(0x765432),
            }]
        }));
        sparse
            .patch([
                Change::Layout(
                    property.key,
                    property.style.clone(),
                    property.algorithm.clone(),
                ),
                Change::Paint(property.key, property.paint.clone()),
                Change::Offset(property.key, property.offset),
                Change::Clip(property.key, property.clip),
                Change::Interactive(property.key, property.interactive),
                Change::HitTesting(property.key, property.hit_testing),
                Change::Role(property.key, property.role),
                Change::WindowAction(property.key, property.window_action),
                Change::Reuse(property.key, property.cache_hint),
            ])
            .unwrap();
        whole
            .patch_elements(vec![property.clone()], (200, 100), 1.0)
            .unwrap();
        for key in [Key::from(1), Key::from(2)] {
            let a = whole.bounds(key).unwrap();
            let b = sparse.bounds(key).unwrap();
            assert_eq!(
                (
                    a.layout,
                    a.paint,
                    a.clip,
                    a.interactive,
                    a.role,
                    a.window_action
                ),
                (
                    b.layout,
                    b.paint,
                    b.clip,
                    b.interactive,
                    b.role,
                    b.window_action
                )
            );
        }
        assert!(
            whole.frame.instances()[0].program.commands()
                == sparse.frame.instances()[0].program.commands()
        );
        let revision = whole.input_revision();
        assert!(matches!(
            whole
                .patch_elements(vec![property], (200, 100), 1.0)
                .unwrap(),
            Damage::None
        ));
        assert_eq!(whole.stats.layout_computations, 0);
        assert_eq!(whole.stats.recordings, 0);
        assert_eq!(whole.input_revision(), revision);
    }

    #[test]
    fn invalid_property_batch_cannot_partly_mutate_the_tree() {
        let mut scene = Scene::default();
        let root = painted(&Rc::new(Cell::new(0)));
        scene
            .update(root.clone(), (200, 100), 1.0, RecordingMode::Retained)
            .unwrap();
        let original = scene.bounds(root.key).unwrap().layout;
        for invalid in [
            Element::at(99, Bounds::default()),
            root.clone()
                .with_children([Element::at(2, Bounds::default())]),
            {
                let mut invalid = root.clone();
                invalid.offset.0 = f32::NAN;
                invalid
            },
        ] {
            let mut moved = root.clone();
            moved.offset = (25.0, 5.0);
            assert!(
                scene
                    .patch_elements(vec![moved, invalid], (200, 100), 1.0)
                    .is_err()
            );
            assert_eq!(scene.bounds(root.key).unwrap().layout, original);
            scene.resize((200, 100), 1.0).unwrap();
            assert_eq!(scene.bounds(root.key).unwrap().layout, original);
        }
    }

    #[derive(PartialEq)]
    struct Grouped;

    impl Layout for Grouped {
        type Cache = ();

        fn render_context(&self, _: &Subviews<'_, '_>) -> retinue_render::OptimizationContext {
            retinue_render::OptimizationContext::default().with(retinue_render::CacheHint::Stable)
        }

        fn measure(
            &self,
            _: Proposal,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<LayoutSize, LayoutError> {
            Ok(LayoutSize::new(80.0, 40.0))
        }

        fn place(
            &self,
            _: Bounds,
            _: &mut Subviews<'_, '_>,
            _: &mut (),
        ) -> Result<(), LayoutError> {
            Ok(())
        }
    }

    #[test]
    fn same_size_layout_replacement_can_remove_render_context() {
        for reconcile in [false, true] {
            let calls = Rc::new(Cell::new(0));
            let mut root = painted(&calls);
            root.algorithm = crate::scene::Algorithm::Custom(LayoutAlgorithm::new(Grouped));
            let mut scene = Scene::default();
            scene
                .update(root.clone(), (200, 100), 1.0, RecordingMode::Retained)
                .unwrap();
            assert_eq!(scene.frame.groups().len(), 1);
            let geometry = scene.bounds(Key::from(1)).unwrap().layout;
            root.algorithm = crate::scene::Algorithm::Flex;
            if reconcile {
                scene
                    .update(root, (200, 100), 1.0, RecordingMode::Retained)
                    .unwrap();
            } else {
                scene
                    .patch([Change::Layout(Key::from(1), root.style, root.algorithm)])
                    .unwrap();
            }
            assert!(scene.frame.groups().is_empty());
            assert_eq!(scene.bounds(Key::from(1)).unwrap().layout, geometry);
            assert_eq!(calls.get(), 1);
        }
    }
}
