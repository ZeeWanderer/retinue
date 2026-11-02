use retinue_ui::{
    motion::{Completion, PlaybackStatus},
    prelude::*,
    runtime::ViewTree,
    state::Observation,
};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant, SystemTime},
};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
fn linear(n: u64) -> Animation {
    Animation::new(ms(n)).easing(Easing::Linear)
}

#[test]
fn tracks_hold_jump_interpolate_and_parallel_tracks_use_one_playhead() {
    let x = Keyframes::new(0.0_f32)
        .to(10.0, linear(100))
        .hold(ms(100))
        .jump(30.0)
        .to(40.0, linear(100));
    for (at, expected) in [
        (0, 0.0),
        (50, 5.0),
        (100, 10.0),
        (150, 10.0),
        (200, 30.0),
        (250, 35.0),
        (300, 40.0),
    ] {
        assert_eq!(x.sample(ms(at)), expected);
    }
    assert_eq!(x.next_sample(ms(120), true, ms(16)), Some(ms(200)));
    assert_eq!(x.next_sample(ms(200), false, ms(16)), Some(ms(184)));
    let y = Keyframes::new(2.0_f32).to(4.0, linear(50));
    let parallel = x.parallel(y);
    assert_eq!(parallel.duration(), ms(300));
    assert_eq!(parallel.sample(ms(50)), (5.0, 4.0));
    assert_eq!(parallel.sample(ms(150)), (10.0, 4.0));
    assert_eq!(
        parallel.next_sample(ms(150), false, ms(16)),
        Some(ms(100) - Duration::from_nanos(1))
    );
}

#[derive(Clone, PartialEq)]
struct Reader {
    player: Playback<Keyframes<f32>>,
    calls: Rc<Cell<u32>>,
}
impl View for Reader {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        self.calls.set(self.calls.get() + 1);
        Rectangle::new(Color::rgb(0x99bbcc)).frame(cx.playback(&self.player), 20.0)
    }
}

#[test]
fn pause_seek_reverse_completion_and_holds_share_the_view_clock() {
    let t = FrameTime {
        instant: Instant::now(),
        wall_time: SystemTime::UNIX_EPOCH,
    };
    let player = Playback::new(
        Keyframes::new(10.0)
            .to(110.0, linear(100))
            .hold(ms(200))
            .to(210.0, linear(100)),
    );
    let reader = Reader {
        player: player.clone(),
        calls: Rc::default(),
    };
    let mut tree = ViewTree::new(reader.clone(), || {});
    tree.set_frame_interval(ms(10));
    tree.update_at((300, 100), 1.0, t).unwrap();
    assert_eq!(tree.next_deadline(), None);
    let run = player.play(t.instant);
    tree.update_at((300, 100), 1.0, t).unwrap();
    assert_eq!(tree.next_deadline(), Some(t.instant + ms(10)));
    player.pause(t.instant + ms(40));
    tree.update_at((300, 100), 1.0, t.at(t.instant + ms(40)))
        .unwrap();
    assert_eq!(tree.next_deadline(), None);
    assert!((player.value(t.instant + ms(500)) - 50.0).abs() < 0.0001);
    assert_eq!(player.status(), PlaybackStatus::Paused);
    player.seek(ms(150), t.instant + ms(500));
    player.play(t.instant + ms(500));
    tree.update_at((300, 100), 1.0, t.at(t.instant + ms(500)))
        .unwrap();
    assert_eq!(
        tree.next_deadline(),
        Some(t.instant + ms(650)),
        "hold skips intermediate frames"
    );
    player.reverse(t.instant + ms(500));
    tree.update_at((300, 100), 1.0, t.at(t.instant + ms(500)))
        .unwrap();
    assert!(tree.next_deadline().unwrap() <= t.instant + ms(551));
    tree.update_at((300, 100), 1.0, t.at(t.instant + ms(650)))
        .unwrap();
    assert_eq!(run.result(), Some(Completion::Finished));
    assert_eq!(player.status(), PlaybackStatus::Finished);
    assert_eq!(tree.next_deadline(), None);
    assert_eq!(player.value(t.instant + ms(650)), 10.0);
    let calls = reader.calls.get();
    tree.update_at((300, 100), 1.0, t.at(t.instant + ms(900)))
        .unwrap();
    assert_eq!(reader.calls.get(), calls);
}

#[test]
fn runs_are_observable_and_replacement_cancellation_and_drop_settle_them() {
    let player = Playback::new(Keyframes::new(0.0).to(1.0, linear(100)));
    let now = Instant::now();
    let first = player.play(now);
    let mut observed = Observation::new(|| {});
    observed.track(|| first.result());
    let second = player.restart(now + ms(20));
    assert!(observed.is_dirty());
    assert_eq!(first.result(), Some(Completion::Superseded));
    player.cancel(now + ms(40));
    assert_eq!(second.result(), Some(Completion::Cancelled));
    let last = player.play(now + ms(60));
    drop(player);
    assert_eq!(last.result(), Some(Completion::Cancelled));
}

#[test]
fn reduced_motion_zero_duration_and_owner_unmount_leave_no_timer() {
    let t = FrameTime::now();
    let owner = PlaybackOwner::new(Keyframes::new(10.0).to(110.0, linear(100)));
    let player = owner.playback();
    let mut tree = ViewTree::new(
        Reader {
            player: player.clone(),
            calls: Rc::default(),
        },
        || {},
    );
    let run = player.play(t.instant);
    tree.environment().set::<ReduceMotion>(true);
    tree.update_at((300, 100), 1.0, t).unwrap();
    assert_eq!(run.result(), Some(Completion::Finished));
    assert_eq!(player.value(t.instant), 110.0);
    assert_eq!(tree.next_deadline(), None);
    tree.environment().set::<ReduceMotion>(false);
    let run = player.restart(t.instant);
    tree.update_at((300, 100), 1.0, t).unwrap();
    drop(owner);
    assert_eq!(run.result(), Some(Completion::Cancelled));
    tree.update_at((300, 100), 1.0, t).unwrap();
    assert_eq!(tree.next_deadline(), None);

    let zero = Playback::new(Keyframes::new(10.0).jump(110.0));
    let run = zero.play(t.instant);
    tree.set_root(Reader {
        player: zero,
        calls: Rc::default(),
    });
    tree.update_at((300, 100), 1.0, t).unwrap();
    assert_eq!(run.result(), Some(Completion::Finished));
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn view_owned_playback_cancels_even_when_controls_escape_the_view() {
    #[derive(ViewProperties)]
    struct Fields {
        #[owned(PlaybackOwner::new(Keyframes::new(10.0_f32).to(110.0, linear(100))))]
        owner: PlaybackOwner<Keyframes<f32>>,
    }
    #[derive(PartialEq)]
    struct OwnerView(Rc<std::cell::RefCell<Option<Playback<Keyframes<f32>>>>>);
    impl View for OwnerView {
        fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
            let fields = cx.properties::<Fields>();
            let player = fields.owner.playback();
            *self.0.borrow_mut() = Some(player.clone());
            Rectangle::new(Color::rgb(0xffffff)).frame(cx.playback(&player), 20.0)
        }
    }
    let controls = Rc::default();
    let mut tree = ViewTree::new(OwnerView(Rc::clone(&controls)), || {});
    tree.update((300, 100), 1.0).unwrap();
    let player = controls.borrow().clone().unwrap();
    let run = player.play(Instant::now());
    tree.update((300, 100), 1.0).unwrap();
    assert!(tree.next_deadline().is_some());
    tree.set_root(empty());
    tree.update((300, 100), 1.0).unwrap();
    assert_eq!(run.result(), Some(Completion::Cancelled));
    assert_eq!(tree.next_deadline(), None);
}

#[test]
fn keyframe_repairs_equal_an_independently_sampled_scene_at_each_scale() {
    use retinue_render::{RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter};
    let t = FrameTime::now();
    for scale in [1.0, 1.25, 2.0] {
        let program = Keyframes::new(10.0_f32)
            .to(120.0, linear(100))
            .hold(ms(100))
            .jump(40.0)
            .to(100.0, linear(100));
        let player = Playback::new(program.clone());
        let mut tree = ViewTree::new(
            Reader {
                player: player.clone(),
                calls: Rc::default(),
            },
            || {},
        );
        let size = ((200.0 * scale) as u32, (100.0 * scale) as u32);
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        let draw = |tree: &ViewTree, renderer: &mut Renderer, pixels: &mut [u8], damage: Damage| {
            renderer.prepare(tree.frame(), RasterMode::Replay);
            let mut painter =
                Painter::with_threads(pixels, size.0, size.1, 2.try_into().unwrap()).unwrap();
            if let Some(rect) = damage.bounds(size) {
                renderer.paint(tree.frame(), &mut painter, rect, Color::rgb(0x121212));
            }
            painter.finish().unwrap();
        };
        tree.update_at(size, scale, t).unwrap();
        draw(&tree, &mut renderer, &mut pixels, Damage::Full);
        player.play(t.instant);
        for at in [0, 30, 60, 100, 160, 200, 230, 300] {
            let damage = tree
                .update_at(size, scale, t.at(t.instant + ms(at)))
                .unwrap();
            draw(&tree, &mut renderer, &mut pixels, damage);
            let mut reference = ViewTree::new(
                Rectangle::new(Color::rgb(0x99bbcc)).frame(program.sample(ms(at)), 20.0),
                || {},
            );
            reference.update_at(size, scale, t).unwrap();
            let mut expected = vec![0; pixels.len()];
            draw(
                &reference,
                &mut Renderer::new(ResidencyPolicy::desktop(0)),
                &mut expected,
                Damage::Full,
            );
            assert!(pixels == expected, "scale={scale}, at={at}");
        }
    }
}
