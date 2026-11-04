use super::*;
mod queued;
use crate::{
    Bounds, Color, Command, Key, PaintFrame, PaintInstance, PaintKey, ProgramStore, RasterMode,
    Rect, Renderer, ResidencyPolicy, painter::Painter,
};

const SIZE: (u32, u32) = (240, 190);
const BACKGROUND: Color = Color::rgb(0x182026);

#[test]
fn independent_and_snapshot_transfers_match_full_repair_across_target_ages() {
    use super::experiment::{Transfer, Trial};
    for transfer in [
        Transfer::Repair,
        Transfer::NoRelocate,
        Transfer::InPlace,
        Transfer::Previous,
        Transfer::PreviousRegions,
        Transfer::PreviousCpu,
        Transfer::PreviousCpuParallel,
        Transfer::SnapshotFull,
        Transfer::SnapshotRegions,
    ] {
        for threads in [1, 4] {
            let mut trial = Trial::new(SIZE, 3, transfer);
            let mut frame = PaintFrame::default();
            let mut store = ProgramStore::default();
            let mut copied = 0;
            for (step, slot) in [0, 1, 2, 0, 2, 2, 1, 0, 2].into_iter().enumerate() {
                let snapshot = prepare(&mut frame, &mut store, step as u32);
                let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
                let sample = trial
                    .paint(
                        slot,
                        snapshot,
                        std::num::NonZeroU32::new(threads).unwrap(),
                        |p, r| {
                            renderer.prepare_for(&frame, RasterMode::Replay, r);
                            renderer.paint(&frame, p, r.bounds(), BACKGROUND);
                            Ok(())
                        },
                    )
                    .unwrap()
                    .unwrap();
                copied += sample.copied_pixels;
                let mut oracle = vec![0; trial.pixels(slot).len()];
                paint(&frame, &mut oracle, &Damage::Full.regions(SIZE)).unwrap();
                let mismatch = trial
                    .pixels(slot)
                    .iter()
                    .zip(&oracle)
                    .position(|(a, b)| a != b);
                assert!(
                    mismatch.is_none(),
                    "{transfer:?}, threads={threads}, step={step}, byte={mismatch:?}"
                );
            }
            assert_eq!(
                copied > 0,
                !matches!(transfer, Transfer::Repair | Transfer::NoRelocate),
                "{transfer:?}"
            );
        }
    }
}

#[test]
fn independent_transfer_does_not_publish_a_failed_target() {
    use super::experiment::{Transfer, Trial};
    for transfer in [
        Transfer::Previous,
        Transfer::PreviousRegions,
        Transfer::PreviousCpu,
        Transfer::PreviousCpuParallel,
        Transfer::SnapshotRegions,
    ] {
        let mut trial = Trial::new(SIZE, 2, transfer);
        let mut frame = PaintFrame::default();
        let mut store = ProgramStore::default();
        for step in 0..5 {
            let snapshot = prepare(&mut frame, &mut store, step);
            let slot = step as usize % 2;
            let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
            let result = trial.paint(
                slot,
                snapshot,
                std::num::NonZeroU32::new(4).unwrap(),
                |p, r| {
                    renderer.prepare_for(&frame, RasterMode::Replay, r);
                    renderer.paint(&frame, p, r.bounds(), BACKGROUND);
                    if step == 2 {
                        Err("abandoned frame".into())
                    } else {
                        Ok(())
                    }
                },
            );
            if step == 2 {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_some());
                let mut oracle = vec![0; trial.pixels(slot).len()];
                paint(&frame, &mut oracle, &Damage::Full.regions(SIZE)).unwrap();
                let mismatch = trial
                    .pixels(slot)
                    .iter()
                    .zip(&oracle)
                    .position(|(a, b)| a != b);
                assert!(
                    mismatch.is_none(),
                    "{transfer:?}, step={step}, byte={mismatch:?}"
                );
            }
        }
    }
}

#[test]
fn disabling_relocation_keeps_damage_and_history_correct() {
    let mut host = Buffered::default();
    host.add_buffer();
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
    for step in 0..5 {
        host.set_relocation(if step == 2 {
            Relocation::Disabled
        } else {
            Relocation::Auto
        });
        let snapshot = prepare(&mut frame, &mut store, step);
        let submission = host
            .render(0, SIZE, Some(snapshot), &mut pixels, |pixels, r| {
                paint(&frame, pixels, r)
            })
            .unwrap()
            .unwrap();
        if step == 2 {
            assert_eq!(submission.copied_pixels, 0);
        }
        if step == 3 {
            assert!(submission.copied_pixels > 0);
        }
        host.submitted(submission);
        let mut oracle = vec![0; pixels.len()];
        paint(&frame, &mut oracle, &Damage::Full.regions(SIZE)).unwrap();
        assert_eq!(pixels, oracle);
    }
}

#[test]
fn expanded_surface_leaves_do_not_inherit_parent_work() {
    let mut store = ProgramStore::default();
    let mut commands: Vec<_> = (0..20)
        .map(|n| Command::Fill {
            bounds: Bounds::new(n as f32 * 11.0, 0.0, 10.0, 10.0),
            radius: 0.0,
            color: BACKGROUND,
        })
        .collect();
    commands.push(Command::External {
        key: Key::new("child", 0),
        bounds: Bounds::new(0.0, 20.0, 30.0, 30.0),
    });
    let mut frame = PaintFrame::default();
    let key = Key::new("root", 0);
    frame
        .replace(
            SIZE,
            1.0,
            vec![PaintInstance::new(
                key,
                store.compile(
                    PaintKey::new(key, 0),
                    (240.0, 190.0),
                    1.0,
                    Rc::new(commands),
                ),
                (0, 0),
                Rect {
                    width: SIZE.0,
                    height: SIZE.1,
                    ..Rect::default()
                },
            )],
            Vec::new(),
        )
        .unwrap();
    let mut surface = Snapshot::new(SIZE, BACKGROUND);
    surface.record(&frame, |_, _| false);
    let repair = Region::from(Rect {
        width: 10,
        height: 10,
        ..Rect::default()
    });
    assert_eq!(surface.repair_work(&repair).solid, 200.0);
}

fn prepare(frame: &mut PaintFrame, store: &mut ProgramStore, step: u32) -> Snapshot {
    let items = (0..16)
        .map(|n| {
            let key = Key::new("card", n);
            let commands = (0..12)
                .map(|k| Command::Fill {
                    bounds: Bounds::new((k % 4 * 13) as f32, (k / 4 * 12) as f32, 11.0, 10.0),
                    radius: 0.0,
                    color: Color::rgba(60 + n as u8, 130, 170, 180),
                })
                .collect();
            PaintInstance::new(
                key,
                store.compile(PaintKey::new(key, 0), (52.0, 36.0), 1.0, Rc::new(commands)),
                (
                    (n % 4) as i32 * 58 + 4,
                    (n / 4) as i32 * 45 + 4 - step as i32 * 3,
                ),
                Rect {
                    width: SIZE.0,
                    height: SIZE.1,
                    ..Rect::default()
                },
            )
        })
        .collect();
    frame.replace(SIZE, 1.0, items, Vec::new()).unwrap();
    let mut snapshot = Snapshot::new(SIZE, BACKGROUND);
    snapshot.record(frame, |_, _| false);
    snapshot
}

fn paint(frame: &PaintFrame, pixels: &mut [u8], repair: &Region) -> Result<(), String> {
    let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
    renderer.prepare_for(frame, RasterMode::Replay, repair);
    let mut painter = Painter::new(pixels, SIZE.0, SIZE.1).unwrap();
    painter.repair(repair);
    renderer.paint(frame, &mut painter, repair.bounds(), BACKGROUND);
    painter.finish().map_err(|e| e.to_string())
}

enum Host {
    Slots(Buffered),
    Age(Aged),
}
impl Host {
    fn new(aged: bool) -> Self {
        if aged {
            Self::Age(Aged::default())
        } else {
            let mut buffers = Buffered::default();
            for _ in 0..3 {
                buffers.add_buffer();
            }
            Self::Slots(buffers)
        }
    }
    fn render(
        &mut self,
        slot: usize,
        age: u8,
        snapshot: Snapshot,
        pixels: &mut [u8],
        draw: impl FnOnce(&mut [u8], &Region) -> Result<(), String>,
    ) -> Result<Option<Submission>, String> {
        match self {
            Self::Slots(host) => host.render(slot, SIZE, Some(snapshot), pixels, draw),
            Self::Age(host) => host.render(age, SIZE, Some(snapshot), pixels, draw),
        }
    }
    fn submitted(&mut self, submission: Submission) {
        match self {
            Self::Slots(host) => host.submitted(submission),
            Self::Age(host) => host.submitted(submission),
        }
    }
}

#[test]
fn shared_hosts_relocate_actual_acquired_generations_without_changing_present_damage() {
    for aged in [false, true] {
        let mut host = Host::new(aged);
        let mut buffers = vec![vec![0; (SIZE.0 * SIZE.1 * 4) as usize]; 3];
        let mut last = [None; 3];
        let mut frame = PaintFrame::default();
        let mut store = ProgramStore::default();
        let mut previous = None;
        let mut copied = 0;
        for (step, slot) in [0, 1, 2, 1, 0, 2, 0, 0, 2].into_iter().enumerate() {
            let snapshot = prepare(&mut frame, &mut store, step as u32);
            let expected_damage = snapshot.damage_since(previous.as_ref()).regions(SIZE);
            previous = Some(prepare(&mut frame, &mut store, step as u32));
            let age = last[slot].map_or(0, |last| (step - last) as u8);
            let submission = host
                .render(slot, age, snapshot, &mut buffers[slot], |pixels, repair| {
                    paint(&frame, pixels, repair)
                })
                .unwrap()
                .unwrap();
            assert_eq!(submission.damage, expected_damage);
            copied += submission.copied_pixels;
            let mut expected = vec![0; buffers[slot].len()];
            paint(&frame, &mut expected, &Damage::Full.regions(SIZE)).unwrap();
            assert!(buffers[slot] == expected, "aged={aged}, step={step}");
            host.submitted(submission);
            last[slot] = Some(step);
        }
        assert!(copied > 0, "must use the optimization without a view hint");
    }
}

#[test]
fn failed_or_unsubmitted_work_never_reuses_old_buffer_evidence() {
    for aged in [false, true] {
        let mut host = Host::new(aged);
        let mut frame = PaintFrame::default();
        let mut store = ProgramStore::default();
        let mut pixels = vec![0; (SIZE.0 * SIZE.1 * 4) as usize];
        let snapshot = prepare(&mut frame, &mut store, 0);
        let submitted = host
            .render(0, 0, snapshot, &mut pixels, |pixels, repair| {
                paint(&frame, pixels, repair)
            })
            .unwrap()
            .unwrap();
        host.submitted(submitted);

        let snapshot = prepare(&mut frame, &mut store, 1);
        let discarded = host
            .render(0, 1, snapshot, &mut pixels, |pixels, repair| {
                paint(&frame, pixels, repair)
            })
            .unwrap()
            .unwrap();
        assert!(discarded.copied_pixels > 0);
        drop(discarded);
        pixels.fill(77);

        let snapshot = prepare(&mut frame, &mut store, 2);
        let next = host
            .render(0, 1, snapshot, &mut pixels, |pixels, repair| {
                paint(&frame, pixels, repair)
            })
            .unwrap()
            .unwrap();
        assert_eq!(next.copied_pixels, 0);
        assert_eq!(next.repaired_pixels, u64::from(SIZE.0) * u64::from(SIZE.1));
        host.submitted(next);

        let snapshot = prepare(&mut frame, &mut store, 3);
        assert!(
            host.render(0, 1, snapshot, &mut pixels, |pixels, _| {
                pixels.fill(44);
                Err("native completion failed".into())
            })
            .is_err()
        );
        let snapshot = prepare(&mut frame, &mut store, 4);
        let next = host
            .render(0, 1, snapshot, &mut pixels, |pixels, repair| {
                paint(&frame, pixels, repair)
            })
            .unwrap()
            .unwrap();
        assert_eq!(next.copied_pixels, 0);
        let mut expected = vec![0; pixels.len()];
        paint(&frame, &mut expected, &Damage::Full.regions(SIZE)).unwrap();
        assert!(pixels == expected);
    }
}

#[test]
fn mismatched_surface_is_rejected_before_writes_and_reset_discards_history() {
    let mut host = Buffered::default();
    host.add_buffer();
    let mut pixels = vec![77; (SIZE.0 * SIZE.1 * 4) as usize];
    let wrong = Snapshot::new((SIZE.0 + 1, SIZE.1), BACKGROUND);
    assert!(
        host.render(0, SIZE, Some(wrong), &mut pixels, |_, _| panic!(
            "not reached"
        ))
        .is_err()
    );
    assert!(pixels.iter().all(|&p| p == 77));
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let snapshot = prepare(&mut frame, &mut store, 0);
    let submission = host
        .render(0, SIZE, Some(snapshot), &mut pixels, |pixels, repair| {
            paint(&frame, pixels, repair)
        })
        .unwrap()
        .unwrap();
    host.submitted(submission);
    host.reset();
    host.add_buffer();
    let snapshot = prepare(&mut frame, &mut store, 1);
    let submission = host
        .render(0, SIZE, Some(snapshot), &mut pixels, |pixels, repair| {
            paint(&frame, pixels, repair)
        })
        .unwrap()
        .unwrap();
    assert_eq!(submission.copied_pixels, 0);
    assert_eq!(submission.repaired_pixels, (SIZE.0 * SIZE.1) as u64);
}
