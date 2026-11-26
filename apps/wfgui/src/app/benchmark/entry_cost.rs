use std::{
    cell::Cell,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Root,
    Child,
    Targets,
    Bind,
    Rail,
}

thread_local! {
    static TOTALS: Cell<[Duration; 5]> = const { Cell::new([Duration::ZERO; 5]) };
}

pub(crate) struct Span(Stage, Instant);

pub(crate) fn start(stage: Stage) -> Span {
    Span(stage, Instant::now())
}

impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.1.elapsed();
        TOTALS.update(|mut totals| {
            totals[self.0 as usize] += elapsed;
            totals
        });
    }
}

pub(crate) fn take() -> [Duration; 5] {
    TOTALS.take()
}

pub(crate) fn elapsed(stage: Stage) -> Duration {
    TOTALS.get()[stage as usize]
}

#[cfg(feature = "profiling")]
pub(crate) mod recurring {
    use super::*;
    use retinue_ui::{runtime::UpdateStats, scene::UpdateProfile as SceneProfile};

    #[derive(Default, Clone, Copy, serde::Serialize)]
    pub(crate) struct Sample {
        updates: usize,
        bodies: usize,
        mounted: usize,
        unmounted: usize,
        materialized: usize,
        solvers: usize,
        restored: usize,
        eval_ns: u64,
        lower_ns: u64,
        reconcile_ns: u64,
        solve_ns: u64,
        geometry_ns: u64,
        restore_ns: u64,
        record_ns: u64,
        frame_ns: u64,
        #[cfg(feature = "layout-profiling")]
        #[serde(serialize_with = "text_parts")]
        text_ns: [u64; retinue_ui::profiling::layout::TEXT_PARTS.len()],
        #[cfg(feature = "layout-profiling")]
        #[serde(serialize_with = "text_parts")]
        text_calls: [u32; retinue_ui::profiling::layout::TEXT_PARTS.len()],
    }
    thread_local! { static SAMPLE: Cell<Option<Sample>> = const { Cell::new(None) }; }
    pub(crate) fn start() {
        super::take();
        SAMPLE.set(Some(Sample::default()));
    }
    pub(crate) fn sampling() -> bool {
        SAMPLE.get().is_some()
    }
    #[cfg(feature = "layout-profiling")]
    fn text_parts<T: serde::Serialize, S: serde::Serializer>(
        values: &[T; retinue_ui::profiling::layout::TEXT_PARTS.len()],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            retinue_ui::profiling::layout::TEXT_PARTS
                .into_iter()
                .zip(values),
        )
    }
    #[cfg(feature = "layout-profiling")]
    pub(crate) fn record_text(layout: retinue_ui::profiling::layout::Sample) {
        let mut sample = SAMPLE.get().expect("active mount sample");
        for (total, ns) in sample.text_ns.iter_mut().zip(layout.text_ns) {
            *total += ns;
        }
        for (total, calls) in sample.text_calls.iter_mut().zip(layout.text_calls) {
            *total += calls;
        }
        SAMPLE.set(Some(sample));
    }
    pub(crate) fn record(
        work: UpdateStats,
        ir: retinue_ui::runtime::UpdateProfile,
        solvers: usize,
        before: SceneProfile,
        after: SceneProfile,
    ) {
        let mut sample = SAMPLE.get().expect("active mount sample");
        sample.updates += 1;
        sample.bodies += work.bodies;
        sample.mounted += work.mounted;
        sample.unmounted += work.unmounted;
        sample.materialized += work.projected;
        sample.solvers += solvers;
        sample.restored += after.restored_records - before.restored_records;
        sample.eval_ns += ir.evaluation.as_nanos() as u64;
        sample.lower_ns += ir.lowering.as_nanos() as u64;
        sample.reconcile_ns += (after.reconcile - before.reconcile).as_nanos() as u64;
        sample.solve_ns += (after.layout - before.layout).as_nanos() as u64;
        sample.geometry_ns += (after.geometry - before.geometry).as_nanos() as u64;
        sample.restore_ns += (after.restoration - before.restoration).as_nanos() as u64;
        sample.record_ns += (after.recording - before.recording).as_nanos() as u64;
        sample.frame_ns += (after.frame - before.frame).as_nanos() as u64;
        SAMPLE.set(Some(sample));
    }
    pub(crate) fn finish() -> (Sample, [u64; 5]) {
        (
            SAMPLE.take().expect("active mount sample"),
            super::take().map(|d| d.as_nanos() as u64),
        )
    }
}

#[cfg(feature = "profiling")]
thread_local! {
    static FIRST_CHILD: Cell<Option<[Duration; 7]>> = const { Cell::new(None) };
}

#[cfg(feature = "profiling")]
pub(crate) fn first_child(
    ir: retinue_ui::runtime::UpdateProfile,
    before: retinue_ui::scene::UpdateProfile,
    scene: retinue_ui::scene::UpdateProfile,
) {
    FIRST_CHILD.set(Some([
        ir.evaluation,
        ir.lowering,
        scene.layout - before.layout,
        scene.geometry - before.geometry,
        scene.frame - before.frame,
        scene.projection - before.projection,
        ir.input,
    ]));
}

#[cfg(feature = "profiling")]
pub(crate) fn take_first_child() -> Option<[Duration; 7]> {
    FIRST_CHILD.take()
}

#[cfg(feature = "layout-profiling")]
thread_local! {
    pub(crate) static CHILD_SOLVE: Cell<Option<(Instant, Instant)>> = const { Cell::new(None) };
}
