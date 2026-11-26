use super::*;
#[allow(dead_code)]
#[path = "../../../../../../tools/benchmarks/support.rs"]
mod bench;

#[test]
#[ignore = "accessibility producer measurement; use make bench BENCH_SUITE=gui-semantics"]
fn accessibility_producer_benchmark() {
    let mut cases = Vec::new();
    for (name, count, active) in [
        ("one-inactive", 1, false),
        ("one-active", 1, true),
        ("eight-inactive", 8, false),
        ("eight-active", 8, true),
    ] {
        if !bench::selected(name) {
            continue;
        }
        let mut case = bench::measure(
            name,
            (1920, 1080),
            1.0,
            || {
                let mut shell = Shell::new(Some(Snapshot::fixture()), 1.0).unwrap();
                shell.pages.fixture();
                shell.resize((1920, 1080), 1.0).unwrap();
                shell.set_page(Page::Relics);
                let card = shell.capture_bounds("page.card").unwrap();
                let positions = [
                    ((card.x + 5) as f64, (card.y + 5) as f64),
                    ((card.x + 5) as f64, (card.y - 1) as f64),
                ];
                (shell, positions, 0usize, 0usize, 0usize)
            },
            |(shell, positions, step, nodes, calls), _| {
                *nodes = 0;
                *calls = 0;
                for _ in 0..count {
                    let damage = shell.pointer_move(positions[*step % 2]);
                    *step += 1;
                    if damage != Damage::None && active {
                        let tree = std::hint::black_box(shell.accessibility());
                        *nodes += tree.nodes.len();
                        *calls += 1;
                    }
                }
            },
            |(_, _, _, nodes, calls)| json!({"motion_events":count, "constructed_trees":calls, "published_nodes":nodes}),
        );
        case["actual_threads"] = json!(1);
        cases.push(case);
    }
    bench::report_with_scope(
        "gui-semantics",
        cases,
        "Pointer transitions on the Relic grid. Active includes the desktop producer's complete tree build; excludes platform adapter transport, assistive client and painting.",
    );
}
