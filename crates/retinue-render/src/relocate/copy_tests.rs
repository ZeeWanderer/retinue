use super::*;

fn plan(size: (u32, u32), copies: Vec<Copy>) -> Plan {
    Plan {
        size,
        copies,
        repair: Region::default(),
    }
}

#[test]
fn independent_copy_matches_reference_across_row_partitions() {
    let size = (19, 13);
    let length = (size.0 * size.1 * 4) as usize;
    let source: Vec<_> = (0..length).map(|n| (n * 31 % 251) as u8).collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .unwrap();
    for copies in [
        Vec::new(),
        vec![Copy {
            source: Rect {
                x: 0,
                y: 0,
                width: 19,
                height: 11,
            },
            destination: Rect {
                x: 0,
                y: 2,
                width: 19,
                height: 11,
            },
        }],
        vec![
            Copy {
                source: Rect {
                    x: 2,
                    y: 0,
                    width: 5,
                    height: 9,
                },
                destination: Rect {
                    x: 3,
                    y: 3,
                    width: 5,
                    height: 9,
                },
            },
            Copy {
                source: Rect {
                    x: 0,
                    y: 3,
                    width: 8,
                    height: 10,
                },
                destination: Rect {
                    x: 10,
                    y: 0,
                    width: 8,
                    height: 10,
                },
            },
        ],
    ] {
        let plan = plan(size, copies);
        let mut expected = vec![0xee; length];
        for copy in &plan.copies {
            for y in 0..copy.source.height {
                for x in 0..copy.source.width {
                    let from = ((copy.source.y + y) * size.0 + copy.source.x + x) as usize * 4;
                    let to =
                        ((copy.destination.y + y) * size.0 + copy.destination.x + x) as usize * 4;
                    expected[to..to + 4].copy_from_slice(&source[from..from + 4]);
                }
            }
        }
        let mut actual = vec![0xee; length];
        plan.apply_from(&source, &mut actual).unwrap();
        assert_eq!(actual, expected);
        for threads in [1, 2, 3, 4, 16] {
            actual.fill(0xee);
            plan.apply_parallel(&pool, &source, &mut actual, threads)
                .unwrap();
            assert_eq!(actual, expected, "{threads} row partitions");
        }
    }
}

#[test]
fn independent_copy_rejects_invalid_buffers_and_budget() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    for size in [(0, 0), (0, 1), (1, 0)] {
        let plan = plan(size, Vec::new());
        assert!(plan.apply_from(&[], &mut []).is_err());
        assert!(plan.apply_parallel(&pool, &[], &mut [], 2).is_err());
    }
    let plan = plan((1, 1), Vec::new());
    for (from, to) in [(0, 4), (4, 0), (5, 4), (4, 5)] {
        let source = vec![0; from];
        let mut target = vec![0; to];
        assert!(plan.apply_from(&source, &mut target).is_err());
        assert!(plan.apply_parallel(&pool, &source, &mut target, 2).is_err());
    }
    assert!(plan.apply_parallel(&pool, &[0; 4], &mut [0; 4], 0).is_err());
}
