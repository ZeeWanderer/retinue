use retinue_render::{RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter};
use retinue_ui::{Key, prelude::*, runtime::ViewTree};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
    time::{Duration, Instant, SystemTime},
};

type Lifetimes = Rc<RefCell<HashMap<u64, Weak<()>>>>;

struct Page {
    number: u64,
    lives: Lifetimes,
}

impl PartialEq for Page {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number && Rc::ptr_eq(&self.lives, &other.lives)
    }
}

impl View for Page {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        let life = cx.owned("life", || Rc::new(()));
        self.lives
            .borrow_mut()
            .insert(self.number, Rc::downgrade(&life));
        Rectangle::new(Color::rgba(30, self.number as u8 * 40, 200, 170))
            .frame(140.0, 60.0)
            .input(InputOptions::default())
            .id(Key::new("hit", self.number))
    }
}

fn transition() -> TranslationTransition {
    TranslationTransition {
        enter: (100.0, 0.0),
        exit: (-100.0, 0.0),
        duration: Duration::from_millis(1000),
        easing: Easing::Linear,
    }
}

fn page(number: Option<u64>, lives: &Lifetimes) -> Switch {
    number
        .map_or_else(Switch::vacant, |number| {
            Switch::new(
                Key::new("page", number),
                Page {
                    number,
                    lives: lives.clone(),
                },
            )
        })
        .transition(transition())
}

fn start() -> FrameTime {
    FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH,
    }
}

fn at(tree: &mut ViewTree, time: FrameTime, ms: u64) -> Damage {
    tree.update_at(
        (400, 160),
        1.0,
        time.at(time.instant + Duration::from_millis(ms)),
    )
    .unwrap()
}

fn alive(lives: &Lifetimes, number: u64) -> bool {
    lives
        .borrow()
        .get(&number)
        .is_some_and(|life| life.strong_count() > 0)
}

fn x(tree: &ViewTree, number: u64) -> f32 {
    let id = tree.unique(Key::new("hit", number)).unwrap().unwrap();
    tree.input()
        .targets()
        .iter()
        .find(|target| target.id == id)
        .unwrap()
        .bounds
        .x
}

#[test]
fn first_page_is_immediate_then_departure_is_bounded_and_not_interactive() {
    let lives = Lifetimes::default();
    let time = start();
    let mut tree = ViewTree::new(page(Some(1), &lives), || {});
    at(&mut tree, time, 0);
    assert!(alive(&lives, 1));
    assert_eq!(tree.next_deadline(), None);
    let initial = x(&tree, 1);
    tree.set_root(page(Some(2), &lives));
    at(&mut tree, time, 0);
    assert!(alive(&lives, 1) && alive(&lives, 2));
    assert_eq!(
        tree.input()
            .targets()
            .iter()
            .filter(|target| target.enabled)
            .count(),
        1
    );
    at(&mut tree, time, 500);
    assert_eq!(x(&tree, 1), initial - 50.0);
    assert_eq!(tree.scene().stats.recordings, 0);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    at(&mut tree, time, 1000);
    assert!(!alive(&lives, 1) && alive(&lives, 2));
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn reversing_keeps_identity_and_presentation_but_rapid_navigation_drops_oldest() {
    let lives = Lifetimes::default();
    let time = start();
    let mut tree = ViewTree::new(page(Some(1), &lives), || {});
    at(&mut tree, time, 0);
    let first = tree.unique(Key::new("page", 1)).unwrap();
    tree.set_root(page(Some(2), &lives));
    at(&mut tree, time, 0);
    at(&mut tree, time, 250);
    let positions = [x(&tree, 1), x(&tree, 2)];
    tree.set_root(page(Some(1), &lives));
    at(&mut tree, time, 250);
    assert_eq!(tree.unique(Key::new("page", 1)).unwrap(), first);
    assert_eq!([x(&tree, 1), x(&tree, 2)], positions);
    assert_eq!(tree.scene().stats.recordings, 0);
    tree.set_root(page(Some(3), &lives));
    at(&mut tree, time, 300);
    assert!(alive(&lives, 1) && alive(&lives, 3));
    assert!(!alive(&lives, 2));
    at(&mut tree, time, 1300);
    assert!(!alive(&lives, 1) && alive(&lives, 3));
}

#[test]
fn vacancy_unmount_reduced_motion_and_disabled_transactions_release_exits() {
    let lives = Lifetimes::default();
    let time = start();
    let mut tree = ViewTree::new(page(Some(1), &lives), || {});
    at(&mut tree, time, 0);
    tree.set_root(page(None, &lives));
    at(&mut tree, time, 0);
    assert!(alive(&lives, 1));
    assert!(tree.input().targets().iter().all(|target| !target.enabled));
    at(&mut tree, time, 1000);
    assert!(!alive(&lives, 1));
    assert_eq!(tree.next_deadline(), None);
    tree.set_root(page(Some(2), &lives));
    at(&mut tree, time, 1000);
    tree.environment().set::<ReduceMotion>(true);
    at(&mut tree, time, 1001);
    assert_eq!(tree.next_deadline(), None);
    tree.environment().set::<ReduceMotion>(false);
    with_transaction(
        Transaction {
            disables_animations: true,
            ..Transaction::default()
        },
        || {
            tree.set_root(page(Some(3), &lives));
        },
    );
    at(&mut tree, time, 1002);
    assert!(!alive(&lives, 2));
    assert_eq!(tree.next_deadline(), None);
    tree.set_root(page(Some(4), &lives));
    at(&mut tree, time, 1100);
    tree.set_root(empty());
    at(&mut tree, time, 1200);
    assert!(!alive(&lives, 3) && !alive(&lives, 4));
    assert_eq!(tree.next_deadline(), None);
}

fn paint(tree: &ViewTree, renderer: &mut Renderer, data: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(data, size.0, size.1, 4.try_into().unwrap()).unwrap();
    painter.repair(&damage.regions(size));
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x202020));
    }
    painter.finish().unwrap();
}

#[test]
fn outgoing_transition_repairs_match_full_drawing_at_each_scale() {
    let time = start();
    for scale in [1.0, 1.25, 2.0] {
        let lives = Lifetimes::default();
        let size = ((400.0 * scale) as u32, (160.0 * scale) as u32);
        let mut tree = ViewTree::new(page(Some(1), &lives), || {});
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        tree.update_at(size, scale, time).unwrap();
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        tree.set_root(page(Some(2), &lives));
        for ms in [0, 125, 250, 500, 750, 1000] {
            let now = time.at(time.instant + Duration::from_millis(ms));
            let damage = tree.update_at(size, scale, now).unwrap();
            paint(&tree, &mut renderer, &mut pixels, damage);
            let mut full = vec![0; pixels.len()];
            paint(
                &tree,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut full,
                Damage::Full,
            );
            assert_eq!(pixels, full, "repair differs at {ms}ms, scale {scale}");
        }
    }
}
