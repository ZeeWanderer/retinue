use super::*;

const SIZE: (u32, u32) = (100, 60);
const A: Rect = Rect {
    x: 10,
    y: 20,
    width: 5,
    height: 7,
};
const B: Rect = Rect { x: 30, ..A };
const C: Rect = Rect { y: 40, ..A };

fn commit_age(tracker: &mut AgeRepaintTracker) {
    let plan = tracker.plan(1, SIZE).unwrap();
    assert!(tracker.submitted(plan.ticket));
}

fn commit_slot(tracker: &mut RepaintTracker, index: usize) {
    let plan = tracker.plan(index, SIZE).unwrap();
    assert!(tracker.submitted(index, plan.ticket));
}

fn check(plan: &Repaint, repair: Region, damage: Region) {
    assert_eq!(plan.repair, repair);
    assert_eq!(plan.damage, damage);
}

fn prepared(count: usize) -> RepaintTracker {
    let mut tracker = RepaintTracker::default();
    tracker.pending.queued = Damage::None;
    tracker.buffers.resize(count, Damage::None);
    tracker
}

#[test]
fn unions_preserve_gaps_until_repair_planning() {
    let damage = Damage::rect(A)
        .union(Damage::rect(B))
        .union(Damage::rect(C));
    let region = damage.regions(SIZE);
    assert_eq!(region.area(), 3 * region::area(A));
    assert!(!region.intersects(Rect { x: 20, ..A }));
    let mut tracker = AgeRepaintTracker::default();
    commit_age(&mut tracker);
    tracker.invalidate(damage);
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(Rect { x: 50, ..A }));
    let repaint = tracker.plan(2, SIZE).unwrap();
    assert_eq!(repaint.damage.area(), region::area(A));
    assert_eq!(repaint.repair.area(), 4 * region::area(A));
    assert!(!repaint.repair.intersects(Rect { x: 20, ..A }));
}

#[test]
fn aged_buffers_consume_offscreen_damage_without_advancing_history() {
    let mut tracker = AgeRepaintTracker::default();
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(A));
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(Rect { x: 200, ..A }));
    assert_eq!(tracker.plan(1, SIZE), None);
    assert!(!tracker.is_dirty());
    tracker.invalidate(Damage::rect(B));
    assert_eq!(tracker.plan(2, SIZE).unwrap().repair, Region::new([A, B]));
}

#[test]
fn aged_buffers_repair_only_changes_since_their_last_frame() {
    let mut tracker = AgeRepaintTracker::default();
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(A));
    check(&tracker.plan(1, SIZE).unwrap(), A.into(), A.into());
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(B));
    check(
        &tracker.plan(2, SIZE).unwrap(),
        Region::new([A, B]),
        B.into(),
    );
    for age in [0, 10, 255] {
        assert_eq!(
            tracker.plan(age, SIZE).unwrap().repair,
            Damage::Full.regions(SIZE)
        );
    }
    tracker.reset();
    assert_eq!(
        tracker.plan(1, SIZE).unwrap().damage,
        Damage::Full.regions(SIZE)
    );
    commit_age(&mut tracker);
    assert!(!tracker.is_dirty());
    assert_eq!(tracker.plan(0, SIZE), None);
}

#[test]
fn repair_includes_changes_while_buffer_was_unavailable() {
    let mut tracker = prepared(3);
    tracker.invalidate(Damage::rect(A));
    check(&tracker.plan(0, SIZE).unwrap(), A.into(), A.into());
    commit_slot(&mut tracker, 0);
    tracker.invalidate(Damage::rect(B));
    check(
        &tracker.plan(1, SIZE).unwrap(),
        Region::new([A, B]),
        B.into(),
    );
    commit_slot(&mut tracker, 1);
    tracker.invalidate(Damage::rect(C));
    check(
        &tracker.plan(2, SIZE).unwrap(),
        Region::new([A, B, C]),
        C.into(),
    );
    check(
        &tracker.plan(0, SIZE).unwrap(),
        Region::new([B, C]),
        C.into(),
    );
    commit_slot(&mut tracker, 2);
    assert!(!tracker.is_dirty());
}

#[test]
fn new_buffer_needs_full_repair_but_not_full_compositor_damage() {
    let mut tracker = prepared(1);
    tracker.invalidate(Damage::rect(A));
    tracker.add_buffer();
    check(
        &tracker.plan(1, SIZE).unwrap(),
        Damage::Full.regions(SIZE),
        A.into(),
    );
}

#[test]
fn coalesces_changes_without_a_free_buffer() {
    let mut tracker = prepared(3);
    tracker.invalidate(Damage::rect(A));
    tracker.invalidate(Damage::rect(B));
    for _ in 0..2 {
        check(
            &tracker.plan(0, SIZE).unwrap(),
            Region::new([A, B]),
            Region::new([A, B]),
        );
    }
    assert!(tracker.is_dirty());
}

#[test]
fn failed_paint_preserves_damage_and_requires_full_repair() {
    let mut tracker = prepared(1);
    tracker.invalidate(Damage::rect(A));
    tracker.plan(0, SIZE).unwrap();
    tracker.failed(0);
    assert!(tracker.is_dirty());
    check(
        &tracker.plan(0, SIZE).unwrap(),
        Damage::Full.regions(SIZE),
        A.into(),
    );
}

#[test]
fn resize_or_remap_resets_buffer_validity() {
    let mut tracker = prepared(3);
    tracker.reset();
    tracker.add_buffer();
    let full = Damage::Full.regions((200, 120));
    check(&tracker.plan(0, (200, 120)).unwrap(), full.clone(), full);
}

#[test]
fn clips_damage_and_consumes_offscreen_changes() {
    let mut tracker = prepared(1);
    tracker.invalidate(Damage::rect(Rect {
        x: 95,
        y: 55,
        width: 10,
        height: 10,
    }));
    let rect = Rect {
        x: 95,
        y: 55,
        width: 5,
        height: 5,
    };
    check(&tracker.plan(0, SIZE).unwrap(), rect.into(), rect.into());
    commit_slot(&mut tracker, 0);
    tracker.invalidate(Damage::rect(Rect { x: u32::MAX, ..A }));
    assert_eq!(tracker.plan(0, SIZE), None);
    assert!(!tracker.is_dirty());
    assert_eq!(Damage::rect(Rect { width: 0, ..A }), Damage::None);
}

#[test]
fn late_damage_survives_completion_even_at_the_same_coordinates() {
    for late in [A, B] {
        let mut tracker = prepared(2);
        tracker.invalidate(Damage::rect(A));
        let first = tracker.plan(0, SIZE).unwrap();
        tracker.failed(0);
        tracker.invalidate(Damage::rect(late));
        assert!(tracker.submitted(0, first.ticket));
        assert!(tracker.is_dirty());
        check(&tracker.plan(0, SIZE).unwrap(), late.into(), late.into());
        check(
            &tracker.plan(1, SIZE).unwrap(),
            Region::new([A, late]),
            late.into(),
        );

        let mut tracker = AgeRepaintTracker::default();
        commit_age(&mut tracker);
        tracker.invalidate(Damage::rect(A));
        let first = tracker.plan(1, SIZE).unwrap();
        tracker.invalidate(Damage::rect(late));
        assert!(tracker.submitted(first.ticket));
        check(&tracker.plan(1, SIZE).unwrap(), late.into(), late.into());
        check(
            &tracker.plan(2, SIZE).unwrap(),
            Region::new([A, late]),
            late.into(),
        );
    }
}

#[test]
fn stale_cross_surface_and_wrong_target_tickets_cannot_acknowledge_damage() {
    let mut tracker = prepared(2);
    tracker.invalidate(Damage::rect(A));
    let first = tracker.plan(0, SIZE).unwrap();
    assert!(!tracker.submitted(1, first.ticket));
    tracker.invalidate(Damage::rect(B));
    let second = tracker.plan(0, SIZE).unwrap();
    check(&second, Region::new([A, B]), Region::new([A, B]));
    assert!(!tracker.submitted(0, first.ticket));
    let mut other = RepaintTracker::default();
    other.add_buffer();
    other.plan(0, SIZE).unwrap();
    assert!(!other.submitted(0, second.ticket));
    assert!(tracker.submitted(0, second.ticket));
    assert!(!tracker.submitted(0, second.ticket));
    tracker.invalidate(Damage::rect(C));
    let third = tracker.plan(0, SIZE).unwrap();
    tracker.reset();
    tracker.add_buffer();
    assert!(!tracker.submitted(0, third.ticket));
    check(
        &tracker.plan(0, SIZE).unwrap(),
        Damage::Full.regions(SIZE),
        Damage::Full.regions(SIZE),
    );
}

#[test]
fn stale_aged_ticket_does_not_advance_history_or_clear_new_damage() {
    let mut tracker = AgeRepaintTracker::default();
    commit_age(&mut tracker);
    tracker.invalidate(Damage::rect(A));
    let old = tracker.plan(1, SIZE).unwrap();
    tracker.invalidate(Damage::rect(B));
    let next = tracker.plan(1, SIZE).unwrap();
    assert!(!tracker.submitted(old.ticket));
    assert!(tracker.current(next.ticket));
    assert!(tracker.submitted(next.ticket));
    assert!(!tracker.submitted(next.ticket));
    tracker.invalidate(Damage::rect(C));
    check(
        &tracker.plan(2, SIZE).unwrap(),
        Region::new([A, B, C]),
        C.into(),
    );
}

#[test]
fn margin_saturates_at_output_origin_and_integer_limit() {
    let rect = Rect {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    };
    assert_eq!(
        rect.expanded(3),
        Rect {
            x: 0,
            y: 0,
            width: 6,
            height: 6
        }
    );
    let far = Rect {
        x: u32::MAX - 1,
        ..rect
    }
    .expanded(3);
    assert_eq!(far.x + far.width, u32::MAX);
}
