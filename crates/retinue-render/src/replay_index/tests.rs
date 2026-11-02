use super::*;

#[test]
fn index_skips_ranges_in_order_and_preserves_unknown_operations() {
    let mut facts: Vec<_> = (0..1024)
        .map(|i| {
            Some(Pixels {
                x: i % 32 * 10,
                y: i / 32 * 10,
                width: 8,
                height: 8,
            })
        })
        .collect();
    facts[100] = None;
    let index = ReplayIndex::build(&facts, || false).unwrap();
    for origin in [(0, 0), (-10, -10), (13, 8)] {
        let clip = Rect {
            x: 5,
            y: 8,
            width: 19,
            height: 24,
        };
        let mut actual = Vec::new();
        let (visited, skipped) = index.visit(origin, clip, |range| actual.extend(range));
        let expected: Vec<_> = facts
            .iter()
            .enumerate()
            .filter_map(|(i, fact)| {
                fact.is_none_or(|f| !f.visible(origin, clip).is_empty())
                    .then_some(i)
            })
            .collect();
        assert!(expected.iter().all(|i| actual.contains(i)));
        assert!(actual.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            visited < 100 && skipped > 800,
            "{visited} nodes, {skipped} skipped"
        );
        assert!(
            index.nodes.capacity() * size_of::<Node>()
                <= ReplayIndex::reservation(facts.len()).unwrap()
        );
    }
}

#[test]
fn cancellation_and_extreme_coordinates_do_not_create_incorrect_bounds() {
    let calls = std::cell::Cell::new(0);
    let facts = vec![Some(Pixels::default()); 4096];
    assert!(
        ReplayIndex::build(&facts, || {
            calls.set(calls.get() + 1);
            calls.get() > 4
        })
        .is_none()
    );
    assert_eq!(calls.get(), 5);
    let facts = [
        Some(Pixels {
            x: i32::MIN,
            y: 0,
            width: u32::MAX,
            height: 4,
        }),
        Some(Pixels {
            x: i32::MAX,
            y: 0,
            width: u32::MAX,
            height: 4,
        }),
    ];
    let index = ReplayIndex::build(&facts, || false).unwrap();
    let mut seen = Vec::new();
    index.visit(
        (0, 0),
        Rect {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        |range| seen.extend(range),
    );
    assert_eq!(seen, vec![0, 1]);
}
