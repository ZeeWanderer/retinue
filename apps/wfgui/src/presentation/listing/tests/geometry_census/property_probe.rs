use super::*;
use retinue_render::{
    Clip, ClipId, PaintInstance, PlacementUpdate, PropertyBinding, PropertyChange,
    PropertyDefinitions, TransformId, Translation,
};

// Isolate the paint-frame consumer. Give it a homogeneous cohort in painter order,
// excluding changing programs and edge clips that cannot follow one translation.
pub(super) fn compare(old: &FrameSnapshot, next: &FrameSnapshot, scale: f32) -> Value {
    let next_by_key: HashMap<_, _> = next
        .frame()
        .instances()
        .iter()
        .map(|i| (i.key, i))
        .collect();
    let delta = |a: &PaintInstance, b: &PaintInstance| {
        (b.origin().0 - a.origin().0, b.origin().1 - a.origin().1)
    };
    let mut counts = std::collections::BTreeMap::new();
    for a in old.frame().instances() {
        if let Some(b) = next_by_key.get(&a.key)
            && a.origin() != b.origin()
        {
            *counts.entry(delta(a, b)).or_insert(0usize) += 1;
        }
    }
    let (&shift, _) = counts.iter().max_by_key(|(_, n)| *n).unwrap();
    let root = TransformId("bench.viewport".into());
    let mut defs = PropertyDefinitions::default();
    defs.translations.push(Translation {
        id: root,
        parent: None,
        offset: (0.0, 0.0),
    });
    let mut origins = HashMap::new();
    let mut clips = HashMap::new();
    let mut bound = Vec::new();
    let mut resolved = Vec::new();
    let mut ends = Vec::new();
    for a in old.frame().instances() {
        let Some(&b) = next_by_key.get(&a.key) else {
            continue;
        };
        if delta(a, b) != shift || a.program.id() != b.program.id() {
            continue;
        }
        let (clip, end) = (a.clip(), b.clip());
        let follows = clip != end;
        if follows
            && (clip.width != end.width
                || clip.height != end.height
                || i64::from(end.x) - i64::from(clip.x) != i64::from(shift.0)
                || i64::from(end.y) - i64::from(clip.y) != i64::from(shift.1))
        {
            continue;
        }
        let p = a.origin();
        let transform = *origins.entry(p).or_insert_with(|| {
            let id = TransformId(Key::new("bench.local", defs.translations.len() as u64));
            defs.translations.push(Translation {
                id,
                parent: Some(root),
                offset: (p.0 as f32 / scale, p.1 as f32 / scale),
            });
            id
        });
        let id = *clips
            .entry((clip.x, clip.y, clip.width, clip.height, follows))
            .or_insert_with(|| {
                let id = ClipId(Key::new("bench.clip", defs.clips.len() as u64));
                defs.clips.push(Clip {
                    id,
                    parent: None,
                    transform: follows.then_some(root),
                    bounds: Bounds::new(
                        clip.x as f32 / scale,
                        clip.y as f32 / scale,
                        clip.width as f32 / scale,
                        clip.height as f32 / scale,
                    ),
                });
                id
            });
        let mut i = PaintInstance::with_properties(
            a.key,
            a.program.clone(),
            PropertyBinding {
                transform: Some(transform),
                clip: Some(id),
            },
        );
        i.cache = a.cache;
        bound.push(i);
        resolved.push(a.clone());
        ends.push([(p, clip), (b.origin(), end)]);
    }
    assert!(!bound.is_empty());
    let mut inherited = PaintFrame::default();
    inherited
        .replace_with_properties(old.frame().size(), scale, bound, vec![], defs)
        .unwrap();
    let mut dense = PaintFrame::default();
    dense
        .replace(old.frame().size(), scale, resolved, vec![])
        .unwrap();
    let patch = |f: &mut PaintFrame, pos: usize, relative: bool| {
        if relative {
            let d = if pos == 1 { shift } else { (0, 0) };
            f.patch_properties([PropertyChange::Translation(
                root,
                (d.0 as f32 / scale, d.1 as f32 / scale),
            )])
            .unwrap()
        } else {
            f.patch_resolved(
                [],
                ends.iter().enumerate().map(|(index, e)| PlacementUpdate {
                    index,
                    origin: e[pos].0,
                    clip: e[pos].1,
                }),
                [],
            )
            .unwrap()
        }
    };
    for pos in [1, 0] {
        let a = patch(&mut dense, pos, false);
        let b = patch(&mut inherited, pos, true);
        assert_eq!(a.regions(old.frame().size()), b.regions(old.frame().size()));
        for (i, expected) in inherited.instances().iter().zip(&ends) {
            assert_eq!((i.origin(), i.clip()), expected[pos]);
        }
    }
    let times = |frame: &mut PaintFrame, relative| {
        let start = Instant::now();
        for n in 0..128 {
            std::hint::black_box(patch(frame, 1 - n % 2, relative));
        }
        start.elapsed().as_nanos() as u64 / 128
    };
    let mut pairs = Vec::new();
    for n in 0..8 {
        let (a, b) = if n % 2 == 0 {
            (times(&mut dense, false), times(&mut inherited, true))
        } else {
            let b = times(&mut inherited, true);
            (times(&mut dense, false), b)
        };
        pairs.push([a, b]);
    }
    let stats = inherited.property_stats();
    json!({"eligible_paints":ends.len(),"affine_delta_px":shift,"translations_visited":stats.translations,
        "clips_visited":stats.clips,"instances_resolved":stats.instances,
        "paired_ns_per_patch_dense_then_property":pairs})
}
