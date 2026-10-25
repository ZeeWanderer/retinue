use super::*;
use retinue_ui::damage::AgeRepaintTracker;

#[test]
fn scaled_minimum_keeps_the_mastery_grid_visible() {
    for scale in [1.0, 1.25, 2.0] {
        let mut shell = view(scale, (2560, 1440));
        let minimum = shell.minimum_size();
        assert_eq!(minimum, ((1024.0 * scale) as u32, (600.0 * scale) as u32));
        shell.resize(minimum, 1.0).unwrap();
        shell.pages.fixture();
        shell.set_page(Page::Mastery);
        let grid = shell.pages.ui.bounds("page.grid").unwrap();
        assert!(grid.height >= shell.px(100.0), "{scale}: {grid:?}");
        shell.resize((minimum.0 * 2, minimum.1 * 2), 2.0).unwrap();
        assert_eq!(shell.minimum_size(), minimum, "window minimum is logical");
    }
}

#[test]
fn explicit_market_navigation_opens_a_drawer_when_the_rail_does_not_fit() {
    let mut shell = view(1.25, (1200, 800));
    assert!(!shell.right_available);
    let center = shell.geometry.center;
    shell.show_market_rail();
    shell.tick(Instant::now() + MOTION);
    assert!(shell.right_available);
    assert_eq!(shell.geometry.center, center);
    assert_eq!(shell.geometry.right.width, shell.px(400.0));
    shell.activate(Control::Right, Instant::now());
    shell.tick(Instant::now() + MOTION);
    assert_eq!(shell.geometry.right.width, 0);
}

#[test]
fn wheel_step_scales_but_touchpad_pixels_do_not() {
    use retinue_ui::desktop::winit::{dpi::PhysicalPosition, event::MouseScrollDelta};
    for scale in [1.0, 1.25, 2.0] {
        let shell = view(scale, (2560, 1440));
        assert_eq!(
            shell.wheel_delta(MouseScrollDelta::LineDelta(0.0, -1.0)),
            (60.0 * scale, true)
        );
        assert_eq!(
            shell.wheel_delta(MouseScrollDelta::LineDelta(0.0, 0.5)),
            (-30.0 * scale, true)
        );
        assert_eq!(
            shell.wheel_delta(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                0.0, -7.25
            ))),
            (7.25, false)
        );
    }
}

fn render(shell: &Shell) -> Vec<u8> {
    let (width, height) = shell.size;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut painter = Painter::new(&mut pixels, width, height).unwrap();
    painter.fill_rounded_rect(0, 0, width, height, 0, BACKGROUND);
    shell.paint(&mut painter);
    painter.finish().unwrap();
    pixels
}

#[test]
fn filter_dismissal_consumes_chrome_press_and_release() {
    for control in [Control::Left, Control::Settings, Control::Larger] {
        let mut shell = view(1.25, (1920, 1080));
        shell.set_page(Page::Foundry);
        shell.foundry.update(foundry::model::fixture());
        shell.foundry.open_filters();
        let (_, r, _) = shell
            .controls()
            .into_iter()
            .find(|(c, _, _)| *c == control)
            .unwrap();
        shell.pointer_move(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64));
        shell.pointer_button(true, Instant::now());
        shell.pointer_button(false, Instant::now());
        assert!(!shell.foundry.filters_open());
        assert_eq!(shell.page, Page::Foundry);
        assert!(!shell.left_collapsed);
        assert_eq!(shell.scale(), 1.25);
        assert_eq!(shell.focus, Some(Focus::Foundry(foundry::Action::Filters)));
    }
}

#[test]
fn listing_menu_dismisses_across_rails_without_activating_the_background() {
    for control in [Control::Left, Control::Settings, Control::Larger] {
        let mut shell = view(1.25, (1920, 1080));
        shell.pages.fixture();
        shell.set_page(Page::Inventory);
        let card = shell.pages.ui.bounds("page.card").unwrap();
        let center = shell.geometry.center;
        shell.pointer_move((
            (center.x + card.x + 5) as f64,
            (center.y + card.y + 5) as f64,
        ));
        assert!(shell.pages.context_menu());
        assert!(shell.pages.ui.bounds("page.menu").is_some());
        let (_, r, _) = shell
            .controls()
            .into_iter()
            .find(|(c, _, _)| *c == control)
            .unwrap();
        shell.pointer_move(((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64));
        shell.pointer_button(true, Instant::now());
        shell.pointer_button(false, Instant::now());
        assert!(shell.pages.ui.bounds("page.menu").is_none());
        assert_eq!(shell.page, Page::Inventory);
        assert!(!shell.left_collapsed);
        assert_eq!(shell.scale(), 1.25);
    }
}

#[test]
fn title_scale_edits_shared_preferences_and_warning_survives_data_refresh() {
    let mut shell = view(1.25, (1920, 1080));
    shell.activate(Control::Larger, Instant::now());
    assert_eq!(shell.preferences.scale(), 1.3);
    assert_eq!(shell.settings.ui_scale, 1.3);
    assert!(!shell.preferences.take_changes().is_empty());
    shell
        .settings
        .set_preference_error(Some("Preferences not saved".into()));
    shell.settings.update(crate::data::Update {
        snapshot: Snapshot::fixture(),
        error: None,
    });
    assert!(shell.settings.preference_error.is_some());
    assert!(
        shell
            .label(Control::Settings)
            .contains("could not be saved")
    );
    shell.settings.set_preference_error(None);
    assert_eq!(shell.label(Control::Settings), "Settings");
}

fn capture_focus(name: &str, shell: &Shell, pixels: &[u8], bounds: Rect) {
    let Some(directory) = std::env::var_os("GUI_FOCUS_CAPTURES") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let image = retinue_ui::image::RgbImage::from_fn(bounds.width, bounds.height, |x, y| {
        let i = ((y + bounds.y) as usize * shell.size.0 as usize + (x + bounds.x) as usize) * 4;
        retinue_ui::image::Rgb([pixels[i + 2], pixels[i + 1], pixels[i]])
    });
    image
        .save(directory.join(format!("{name}-{}.png", (shell.scale() * 100.0).round())))
        .unwrap();
}

#[test]
fn pointer_focus_is_invisible_and_keyboard_rings_are_shared_across_pages() {
    use retinue_ui::desktop::winit::keyboard::{Key, NamedKey};
    for scale in [1.0, 1.25, 2.0] {
        let mut shell = view(scale, (1920, 1080));
        for (name, focus) in [
            ("title", Focus::Chrome(Control::Close)),
            ("settings", Focus::Page(settings::Action::ClearMemory)),
            ("foundry", Focus::Foundry(foundry::Action::Filters)),
        ] {
            if matches!(focus, Focus::Foundry(_)) {
                shell.set_page(Page::Foundry);
                shell.foundry.update(foundry::model::fixture());
            }
            let baseline = render(&shell);
            shell.focus_visible.pointer_down();
            shell.set_focus(Some(focus));
            let pointer = render(&shell);
            assert!(
                pointer == baseline,
                "mouse focus drew a ring: {name}/{scale}"
            );
            shell.note_keyboard(Key::Named(NamedKey::Tab));
            let keyboard = render(&shell);
            assert!(keyboard != pointer, "missing keyboard ring: {name}/{scale}");
            assert!(keyboard.chunks_exact(4).any(|pixel| pixel == rgb(0xaa9af9)));
            let bounds = if name == "title" {
                shell.geometry.title
            } else {
                shell.geometry.center
            };
            capture_focus(&format!("{name}-pointer"), &shell, &pointer, bounds);
            capture_focus(&format!("{name}-keyboard"), &shell, &keyboard, bounds);
            shell.set_focus(None);
        }
        shell.focus_visible.pointer_down();
        shell.set_focus(Some(Focus::Foundry(foundry::Action::Search)));
        assert!(
            render(&shell)
                .chunks_exact(4)
                .any(|pixel| pixel == rgb(0xaa9af9))
        );
    }
}

#[test]
fn keyboard_modality_survives_hover_and_focus_transfer_but_ignores_shortcuts() {
    use retinue_ui::desktop::winit::keyboard::{Key, NamedKey};
    let mut shell = view(1.0, (1440, 900));
    shell.pointer_move((50.0, 20.0));
    shell.pointer_button(true, Instant::now());
    shell.pointer_move((300.0, 20.0));
    shell.pointer_button(false, Instant::now());
    assert!(!shell.focus_visible.shows(true, false));
    shell.note_keyboard(Key::Named(NamedKey::Shift));
    assert!(!shell.focus_visible.shows(true, false));
    for modifiers in [
        ModifiersState::CONTROL,
        ModifiersState::ALT,
        ModifiersState::SUPER,
    ] {
        shell.modifiers = modifiers;
        shell.note_keyboard(Key::Character("r"));
        assert!(!shell.focus_visible.shows(true, false));
    }
    shell.modifiers = ModifiersState::SHIFT;
    shell.note_keyboard(Key::Named(NamedKey::Tab));
    assert!(shell.focus_visible.shows(true, false));
    shell.pointer_move((400.0, 40.0));
    shell.set_focus(Some(Focus::Page(settings::Action::ClearRendered)));
    assert!(shell.focus_visible.shows(true, false));
    shell.settings.activate(settings::Action::ClearRendered);
    assert_eq!(shell.settings.focus, Some(settings::Action::Cancel));
    assert!(shell.focus_visible.shows(true, false));
    shell.settings.activate(settings::Action::Cancel);
    assert_eq!(shell.settings.focus, Some(settings::Action::ClearRendered));
    assert!(shell.focus_visible.shows(true, false));
}

#[test]
fn rail_and_window_buttons_have_matching_highlight_bounds() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let mut shell = view(scale, (2560, 1440));
        let controls = shell.controls();
        let reference = controls
            .iter()
            .find(|(c, _, _)| *c == Control::Close)
            .unwrap()
            .1;
        for control in [
            Control::Left,
            Control::Right,
            Control::Minimize,
            Control::Maximize,
        ] {
            let bounds = controls.iter().find(|(c, _, _)| *c == control).unwrap().1;
            assert_eq!(
                (bounds.y, bounds.width, bounds.height),
                (reference.y, reference.width, reference.height)
            );
            if std::env::var_os("GUI_FOCUS_CAPTURES").is_some() {
                shell.pointer_move((
                    bounds.x as f64 + bounds.width as f64 / 2.0,
                    bounds.y as f64 + bounds.height as f64 / 2.0,
                ));
                let pixels = render(&shell);
                capture_focus(
                    &format!("hover-{control:?}"),
                    &shell,
                    &pixels,
                    bounds.expanded(shell.px(4.0)),
                );
            }
        }
        assert_eq!(
            shell.icons.get(Icon::Left, true).width,
            (16.0 * scale).round() as u32
        );
        assert_eq!(
            shell.icons.get(Icon::Right, true).width,
            (16.0 * scale).round() as u32
        );
    }
}

#[test]
fn rail_damage_repairs_alternating_buffers_like_a_full_repaint() {
    let size = (1440, 900);
    for scale in [1.0, 1.25] {
        let mut shell = view(scale, size);
        let mut buffers = [
            vec![0_u32; (size.0 * size.1) as usize],
            vec![0_u32; (size.0 * size.1) as usize],
        ];
        let mut expected = buffers[0].clone();
        let mut damage = AgeRepaintTracker::default();
        let origin = Instant::now();
        for (cycle, control) in [Control::Left, Control::Right, Control::Left, Control::Right]
            .into_iter()
            .enumerate()
        {
            let now = origin + Duration::from_secs(cycle as u64);
            shell.activate(control, now);
            damage.invalidate(Damage::Full);
            for frame in 0..=5 {
                damage.invalidate(shell.tick(now + MOTION.mul_f32(frame as f32 / 5.0)));
                let age = if cycle == 0 && frame < 2 { 0 } else { 2 };
                let Some(plan) = damage.plan(age, size) else {
                    continue;
                };
                let pixels = &mut buffers[frame % 2];
                let paint = |pixels: &mut [u32], clip: Rect| {
                    let mut painter =
                        Painter::new(bytemuck::cast_slice_mut(pixels), size.0, size.1).unwrap();
                    painter.clip_to_rect(clip);
                    fill(&mut painter, clip, 0, BACKGROUND);
                    shell.paint(&mut painter);
                    painter.finish().unwrap();
                };
                paint(pixels, plan.repair);
                paint(&mut expected, Damage::Full.bounds(size).unwrap());
                let mismatch = pixels.iter().zip(&expected).position(|(a, b)| a != b);
                assert!(pixels.iter().all(|pixel| pixel >> 24 == 255));
                assert_eq!(
                    mismatch, None,
                    "scale {scale}, cycle {cycle}, frame {frame}"
                );
                damage.submitted();
            }
        }
    }
}

fn view(scale: f32, size: (u32, u32)) -> Shell {
    let mut view = Shell::new(Some(Snapshot::fixture()), scale).unwrap();
    view.resize(size, 1.0).unwrap();
    view
}

#[test]
fn rails_reverse_without_jumping_and_preserve_responsive_preferences() {
    let mut shell = view(1.0, (1440, 900));
    let now = Instant::now();
    assert_eq!(shell.geometry.left.width, 142);
    assert_eq!(shell.geometry.right.width, 400);
    shell.activate(Control::Left, now);
    shell.tick(now + MOTION / 2);
    assert_eq!(shell.geometry.left.width, 97);
    let before = shell.geometry;
    shell.activate(Control::Left, now + MOTION / 2);
    shell.tick(now + MOTION / 2);
    assert_eq!(shell.geometry, before);
    shell.tick(now + MOTION * 2);
    assert_eq!(shell.geometry.left.width, 142);
    assert_eq!(shell.left.deadline(now + MOTION * 2, FRAME), None);
    shell.set_collapsed(true, false);
    shell.resize((960, 600), 1.0).unwrap();
    assert_eq!(shell.geometry.right.width, 0);
    shell.resize((1440, 900), 1.0).unwrap();
    assert_eq!(shell.geometry.left.width, 52);
    assert_eq!(shell.geometry.right.width, 400);
    shell.set_collapsed(true, true);
    shell.resize((1920, 1080), 1.0).unwrap();
    assert_eq!(shell.geometry.right.width, 0);
    shell.resize((1024, 700), 1.0).unwrap();
    shell.set_collapsed(false, false);
    let now = Instant::now();
    shell.activate(Control::Left, now);
    for elapsed in [Duration::ZERO, MOTION / 2, MOTION] {
        shell.tick(now + elapsed);
        assert_eq!(shell.geometry.right.width, 0);
    }
}

#[test]
fn scaled_chrome_does_not_overlap_and_edges_survive_restore() {
    for (scale, size) in [
        (1.0, (1440, 900)),
        (1.25, (1920, 1080)),
        (1.5, (2560, 1440)),
        (2.0, (960, 600)),
    ] {
        let mut shell = view(scale, size);
        let g = shell.geometry;
        assert_eq!(g.center.y + g.center.height, size.1);
        assert!(g.left.x + g.left.width < g.center.x);
        assert!(g.center.width >= shell.px(360.0));
        assert!(g.center.x + g.center.width <= g.right.x);
        let controls = shell.controls();
        for (i, (_, rect, _)) in controls.iter().enumerate() {
            assert!(rect.x + rect.width <= size.0);
            for (_, other, _) in &controls[i + 1..] {
                assert!(rect.intersection(*other).is_empty());
            }
        }
        let close = controls
            .iter()
            .find(|(c, _, _)| *c == Control::Close)
            .unwrap()
            .1;
        assert_eq!(
            shell.resize_direction((close.x as f64 + 1.0, close.y as f64 + 1.0)),
            None
        );
        assert_eq!(
            shell.resize_direction((1.0, 1.0)),
            Some(ResizeDirection::NorthWest)
        );
        shell.maximized = true;
        assert_eq!(shell.resize_direction((1.0, 1.0)), None);
        shell.maximized = false;
        assert_eq!(
            shell.resize_direction((1.0, 1.0)),
            Some(ResizeDirection::NorthWest)
        );
        shell.activate(Control::Larger, Instant::now());
        if scale < 2.0 {
            assert!((shell.settings.ui_scale - (scale + 0.05)).abs() < 0.001);
        }
    }
}

#[test]
fn shell_accessibility_uses_window_coordinates_and_modal_focus_is_isolated() {
    let mut shell = view(1.25, (1920, 1080));
    shell.set_focus(Some(Focus::Page(settings::Action::ClearRendered)));
    let tree = shell.accessibility();
    assert_eq!(tree.tree.unwrap().root, NodeId(1));
    let node = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == settings::Action::ClearRendered.id())
        .unwrap();
    assert!(node.1.bounds().unwrap().x0 >= shell.geometry.center.x as f64);
    shell.settings.activate(settings::Action::ClearRendered);
    shell.activate(Control::Left, Instant::now());
    assert!(!shell.left_collapsed);
    let tree = shell.accessibility();
    assert!(!tree.nodes.iter().any(|(id, _)| *id == Control::Left.id()));
    assert_eq!(tree.focus, settings::Action::Cancel.id());
}

#[test]
fn chrome_release_on_page_does_not_activate_and_disabled_controls_do_not_focus() {
    let mut shell = view(1.0, (1000, 760));
    let now = Instant::now();
    shell.pointer_move((50.0, 20.0));
    shell.pointer_button(true, now);
    shell.pointer_move((500.0, 300.0));
    shell.pointer_button(false, now);
    assert!(!shell.left_collapsed);
    assert_eq!(shell.pressed, None);
    let rect = shell
        .controls()
        .into_iter()
        .find(|(c, _, _)| *c == Control::Right)
        .unwrap()
        .1;
    shell.set_focus(None);
    shell.pointer_move((rect.x as f64 + 3.0, rect.y as f64 + 3.0));
    shell.pointer_button(true, now);
    assert_eq!(shell.focus, None);
}

#[test]
fn shell_renders_all_rail_states_at_fractional_scales() {
    for scale in [1.0, 1.25, 1.5] {
        let mut shell = view(scale, (1920, 1080));
        let now = Instant::now();
        shell.activate(Control::Left, now);
        shell.activate(Control::Right, now);
        for elapsed in [Duration::ZERO, MOTION / 2, MOTION] {
            shell.tick(now + elapsed);
            let mut pixels = vec![0_u8; 1920 * 1080 * 4];
            let mut painter = Painter::new(&mut pixels, 1920, 1080).unwrap();
            fill(
                &mut painter,
                Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                },
                0,
                BACKGROUND,
            );
            shell.paint(&mut painter);
            painter.finish().unwrap();
            assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
