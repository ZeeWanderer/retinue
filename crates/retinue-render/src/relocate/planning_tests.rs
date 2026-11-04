use super::*;
use crate::{
    Bounds, Color, Command, Key, PaintFrame, PaintInstance, PaintKey, ProgramStore, RasterMode,
    Renderer, ResidencyPolicy, painter::Painter,
};
use std::rc::Rc;

const SIZE: (u32, u32) = (440, 340);
const BACKGROUND: Color = Color::rgb(0x151a20);
const PAGE: Key = Key::new("page", 0);
const EDITOR: Key = Key::new("editor", 0);

#[test]
fn ranking_prefers_simple_repaint_and_relocation_for_expensive_replay() {
    use crate::cost::{CopyExecution, Execution, Work};
    let execution = Execution {
        threads: 4,
        copy: CopyExecution::Queued,
    };
    let benefit = |work, bytes, count| transfer_benefit(execution, work, bytes, count);
    let fill = Work {
        solid: 1_000_000.0,
        commands: 1.0,
        ..Work::default()
    };
    assert_eq!(benefit(fill, 4_000_000, 1), 0.0);
    let text = Work {
        glyphs: 500.0,
        commands: 100.0,
        ..fill
    };
    assert!(benefit(text, 4_000_000, 1) > 0.0);
    assert_eq!(benefit(text, 4_000_000, 400), 0.0);
    assert_eq!(benefit(Work::default(), 1, 1), 0.0);
    assert_eq!(
        benefit(
            Work {
                solid: -1_000_000.0,
                ..Work::default()
            },
            1,
            1
        ),
        0.0
    );
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn frames(
    root: &mut PaintFrame,
    page: &mut PaintFrame,
    store: &mut ProgramStore,
    step: u32,
    rounded: bool,
) {
    let mut make = |key: Key, origin, clip, commands| {
        PaintInstance::new(
            key,
            store.compile(
                PaintKey::new(key, 0),
                (440.0, 340.0),
                1.0,
                Rc::new(commands),
            ),
            origin,
            clip,
        )
    };
    let mut items = Vec::new();
    for n in 0..36 {
        if step >= 11 && n == 7 {
            continue;
        }
        let color = if step >= 5 && n == 5 {
            Color::rgba(180, 80, 20, 190)
        } else {
            Color::rgba(80, 120, 180, 190)
        };
        let commands = (0..12)
            .map(|cell| Command::Fill {
                bounds: Bounds::new((cell % 4 * 16) as f32, (cell / 4 * 12) as f32, 14.0, 10.0),
                radius: if rounded { 2.0 } else { 0.0 },
                color,
            })
            .collect();
        items.push(make(
            Key::new("card", n as u64),
            (
                4 + (n % 4) as i32 * 80,
                4 + (n / 4) as i32 * 48 - (step * 7) as i32,
            ),
            rect(0, 0, 326, 251),
            commands,
        ));
    }
    items.push(make(
        EDITOR,
        (0, 0),
        rect(0, 0, 326, 251),
        vec![Command::External {
            bounds: Bounds::new(50.0, 80.0, 38.0, 21.0),
            key: EDITOR,
        }],
    ));
    page.replace((326, 251), 1.0, items, Vec::new()).unwrap();
    let mut items = vec![
        make(
            1.into(),
            (0, 0),
            rect(0, 0, SIZE.0, SIZE.1),
            vec![
                Command::Fill {
                    bounds: Bounds::new(20.0, 10.0, 370.0, 290.0),
                    radius: if rounded { 12.0 } else { 0.0 },
                    color: Color::rgba(40, 60, 70, 180),
                },
                Command::Fill {
                    bounds: Bounds::new(20.0, 22.0, 370.0, 278.0),
                    radius: 0.0,
                    color: Color::rgba(20, 30, 40, 100),
                },
            ],
        ),
        make(
            PAGE,
            (0, 0),
            rect(0, 0, SIZE.0, SIZE.1),
            vec![Command::Clip {
                bounds: Bounds::new(28.0, 25.0, 320.0, 245.0),
                commands: Rc::new(vec![Command::External {
                    bounds: Bounds::new(28.0, 25.0, 326.0, 251.0),
                    key: PAGE,
                }]),
            }],
        ),
    ];
    if (3..15).contains(&step) {
        items.push(make(
            90.into(),
            (0, 0),
            rect(0, 0, SIZE.0, SIZE.1),
            vec![Command::Fill {
                bounds: Bounds::new(80.0 + (step % 3) as f32 * 3.0, 101.0, 75.0, 45.0),
                radius: if rounded { 4.0 } else { 0.0 },
                color: Color::rgba(200, 50, 30, 170),
            }],
        ));
    }
    root.replace(SIZE, 1.0, items, Vec::new()).unwrap();
}

fn record(root: &PaintFrame, page: &PaintFrame) -> Scene {
    let mut scene = Scene::new(SIZE, BACKGROUND);
    scene.record(root, |key, scene| {
        if key == PAGE {
            scene.record(page, |_, _| false);
            true
        } else {
            false
        }
    });
    scene
}

fn render(pixels: &mut [u8], root: &PaintFrame, page: &PaintFrame, repair: &Region, step: u32) {
    let mut a = Renderer::new(ResidencyPolicy::desktop(0));
    let mut b = Renderer::new(ResidencyPolicy::desktop(0));
    a.set_optimizations(false);
    b.set_optimizations(false);
    a.prepare(root, RasterMode::Replay);
    b.prepare(page, RasterMode::Replay);
    let mut painter = Painter::new(pixels, SIZE.0, SIZE.1).unwrap();
    painter.repair(repair);
    a.paint_with(
        root,
        &mut painter,
        repair.bounds(),
        BACKGROUND,
        |key, painter| {
            assert_eq!(key, PAGE);
            b.draw_with(page, painter, rect(0, 0, 326, 251), |key, painter| {
                assert_eq!(key, EDITOR);
                painter.fill_rounded_rect(0, 0, 38, 21, 0, [20, (step * 11) as u8, 180, 190]);
            });
        },
    );
    painter.finish().unwrap();
}

#[test]
fn nested_surface_scroll_hover_and_opaque_content_match_full_paint() {
    for rounded in [false, true] {
        for count in [1, 3] {
            let mut buffers = vec![vec![0; SIZE.0 as usize * SIZE.1 as usize * 4]; count];
            let mut states: Vec<Option<Rc<Scene>>> = (0..count).map(|_| None).collect();
            let mut root = PaintFrame::default();
            let mut page = PaintFrame::default();
            let mut store = ProgramStore::default();
            let mut copied = 0;
            for step in 0..24 {
                frames(&mut root, &mut page, &mut store, step, rounded);
                let scene = Rc::new(record(&root, &page));
                let index = step as usize % count;
                let full = Region::from(scene.bounds());
                let plan = Plan::new(&scene, states[index].as_deref(), &full);
                copied += plan.copied_pixels();
                plan.apply(&mut buffers[index]).unwrap();
                render(&mut buffers[index], &root, &page, &plan.repair, step);
                let mut expected = vec![0; buffers[index].len()];
                render(&mut expected, &root, &page, &full, step);
                let mismatch = buffers[index]
                    .chunks_exact(4)
                    .zip(expected.chunks_exact(4))
                    .enumerate()
                    // Native curve clipping can round a channel by one unit, also without relocation.
                    .find(|(_, (actual, expected))| {
                        actual
                            .iter()
                            .zip(*expected)
                            .any(|(a, b)| a.abs_diff(*b) > u8::from(rounded))
                    })
                    .map(|(pixel, (actual, expected))| {
                        (
                            pixel % SIZE.0 as usize,
                            pixel / SIZE.0 as usize,
                            actual,
                            expected,
                        )
                    });
                assert!(
                    mismatch.is_none(),
                    "buffers {count}, frame {step}: {mismatch:?}; copies {:?}, repair {:?}",
                    plan.copies,
                    plan.repair
                );
                states[index] = Some(scene);
            }
            assert!(copied > 0, "must exercise relocation");
        }
    }
}

#[test]
fn metadata_does_not_retain_frames_and_age_history_is_bounded() {
    let mut root = PaintFrame::default();
    let mut page = PaintFrame::default();
    frames(&mut root, &mut page, &mut ProgramStore::default(), 0, false);
    let weak = Rc::downgrade(&page.identity);
    let scene = Rc::new(record(&root, &page));
    drop(page);
    assert!(weak.upgrade().is_none());
    let mut history = History::default();
    for _ in 0..20 {
        history.submitted(scene.clone());
    }
    assert!(history.at_age(0).is_none());
    assert!(history.at_age(9).is_some());
    assert!(history.at_age(10).is_none());
    history.clear();
    assert!(history.latest().is_none());
}

#[test]
fn split_copy_orders_unread_sources_and_declines_cycles() {
    let copies = vec![
        Copy {
            source: rect(0, 0, 8, 4),
            destination: rect(0, 4, 8, 4),
        },
        Copy {
            source: rect(0, 4, 8, 4),
            destination: rect(0, 8, 8, 4),
        },
    ];
    let mut pixels: Vec<_> = (0..8 * 12 * 4).map(|n| (n % 251) as u8).collect();
    let source = pixels.clone();
    let plan = Plan {
        size: (8, 12),
        copies: ordered(copies).unwrap(),
        repair: Region::default(),
    };
    plan.apply(&mut pixels).unwrap();
    assert_eq!(&pixels[8 * 4 * 4..], &source[..8 * 8 * 4]);
    assert!(
        ordered(vec![
            Copy {
                source: rect(0, 0, 8, 4),
                destination: rect(0, 4, 8, 4)
            },
            Copy {
                source: rect(0, 4, 8, 4),
                destination: rect(0, 0, 8, 4)
            },
        ])
        .is_none()
    );
    let before = pixels.clone();
    assert!(plan.apply(&mut pixels[..16]).is_err());
    assert_eq!(pixels, before);
}
