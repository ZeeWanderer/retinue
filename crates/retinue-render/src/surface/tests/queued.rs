use super::*;

#[test]
fn aged_native_copies_recover_from_abandoned_frames_and_policy_changes() {
    let mut host = Aged::default();
    let mut pixels = vec![vec![0; (SIZE.0 * SIZE.1 * 4) as usize]; 3];
    let mut serials = [None; 3];
    let mut serial = 0u8;
    let mut latest = None;
    let mut frame = PaintFrame::default();
    let mut store = ProgramStore::default();
    let mut copies = 0;
    for (step, index) in [0, 1, 2, 0, 0, 1, 2, 0, 2, 1, 1, 0].into_iter().enumerate() {
        host.set_relocation(if step == 5 {
            Relocation::Disabled
        } else {
            Relocation::Auto
        });
        let snapshot = prepare(&mut frame, &mut store, step as u32);
        let age = serials[index].map_or(0, |last| serial - last + 1);
        let (target, source) = if let Some(from) = latest.filter(|from| *from != index) {
            let (target, source) = if index < from {
                let (left, right) = pixels.split_at_mut(from);
                (&mut left[index], &right[0])
            } else {
                let (left, right) = pixels.split_at_mut(index);
                (&mut right[0], &left[from])
            };
            (target, Some(source.as_slice()))
        } else {
            (&mut pixels[index], None)
        };
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        let result = host.render_queued(
            age,
            SIZE,
            Some(snapshot),
            target,
            source,
            NonZeroU32::new(4).unwrap(),
            |p, r| {
                renderer.prepare_for(&frame, RasterMode::Replay, r);
                renderer.paint(&frame, p, r.bounds(), BACKGROUND);
                if step == 8 {
                    Err("paint failed".into())
                } else {
                    Ok(())
                }
            },
        );
        if step == 8 {
            assert!(result.is_err());
            continue;
        }
        let plan = result.unwrap().unwrap();
        if step == 3 {
            drop(plan);
            continue;
        }
        if step == 5 {
            assert_eq!(plan.copied_pixels, 0);
        }
        copies += plan.copied_pixels;
        host.submitted(plan);
        serial += 1;
        serials[index] = Some(serial);
        latest = Some(index);
        let mut oracle = vec![0; pixels[index].len()];
        paint(&frame, &mut oracle, &Damage::Full.regions(SIZE)).unwrap();
        assert_eq!(pixels[index], oracle, "step {step}");
    }
    assert!(copies > 0);
    host.reset();
    let mut resized = vec![0; 120 * 100 * 4];
    let plan = host
        .render_queued(
            0,
            (120, 100),
            None,
            &mut resized,
            None,
            NonZeroU32::MIN,
            |p, _| {
                p.clear();
                Ok(())
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(plan.copied_pixels, 0);
    assert_eq!(plan.repaired_pixels, 12_000);
    host.submitted(plan);
}
