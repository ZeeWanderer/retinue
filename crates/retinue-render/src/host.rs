use super::{
    AnalysisPolicy, AnalysisStats, PaintProgram,
    background::{Background, Pause},
    residency::RasterId,
};
use crate::{
    painter::{ImageStorage, RasterImage, render_thread_budget},
    resources::ResourceId,
};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet},
    num::NonZeroU32,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug)]
pub struct HostPolicy {
    pub peak_bytes: usize,
    pub preparation_tasks: usize,
    pub preparation_pixels: usize,
    pub preparation_time: Duration,
    /// Includes the shared calling thread, not just native workers.
    pub threads: u32,
    pub analysis: AnalysisPolicy,
}

impl HostPolicy {
    pub const fn desktop() -> Self {
        Self {
            peak_bytes: 128 * 1024 * 1024,
            preparation_tasks: 8,
            preparation_pixels: 4 * 1024 * 1024,
            preparation_time: Duration::from_millis(2),
            threads: 6,
            analysis: AnalysisPolicy::desktop(),
        }
    }

    pub const fn overlay() -> Self {
        Self {
            peak_bytes: 8 * 1024 * 1024,
            preparation_tasks: 2,
            preparation_pixels: 256 * 1024,
            preparation_time: Duration::from_micros(500),
            threads: 2,
            analysis: AnalysisPolicy::overlay(),
        }
    }

    pub(super) const fn standalone(peak_bytes: usize) -> Self {
        Self {
            peak_bytes,
            preparation_tasks: usize::MAX,
            preparation_pixels: usize::MAX,
            preparation_time: Duration::MAX,
            threads: 6,
            analysis: AnalysisPolicy::disabled(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HostStats {
    pub selected_bytes: usize,
    pub warm_bytes: usize,
    pub retired_bytes: usize,
    pub live_bytes: usize,
    pub pending_bytes: usize,
    pub peak_limit_bytes: usize,
    pub preparation_tasks: usize,
    pub preparation_pixels: usize,
    pub preparation_time: Duration,
    pub deferred_tasks: usize,
    pub warm_entries: usize,
    pub expired_entries: u64,
    pub analysis: AnalysisStats,
}

struct Allocation {
    storage: ImageStorage,
    bytes: usize,
    selected: usize,
    warm: usize,
}

struct Warm {
    image: Rc<RasterImage>,
    deadline: Instant,
}

struct State {
    policy: HostPolicy,
    allocations: HashMap<ResourceId, Allocation>,
    retired: HashSet<ResourceId>,
    live: usize,
    pending: usize,
    reclaimed: u64,
    depth: usize,
    foreground_workers: u32,
    tasks: usize,
    pixels: usize,
    time: Duration,
    deferred: usize,
    warm: HashMap<RasterId, Warm>,
    expirations: BTreeSet<(Instant, RasterId)>,
    expired: u64,
    analysis: Background,
    native_frames: usize,
}

/// One owner-thread host, shared by its independently retained renderers.
#[derive(Clone)]
pub struct RenderHost(Rc<RefCell<State>>);

#[must_use = "keep the pass alive around sibling preparation and drawing"]
pub struct RenderPass {
    host: RenderHost,
    workers: u32,
    threads: NonZeroU32,
    managed: bool,
    pause: Option<Pause>,
}

impl RenderPass {
    pub fn threads(&self) -> NonZeroU32 {
        self.threads
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        {
            let mut state = self.host.0.borrow_mut();
            state.depth -= 1;
            state.foreground_workers -= self.workers;
            state.native_frames -= usize::from(self.managed);
        }
        self.pause.take();
    }
}

pub(super) struct Reservation {
    host: RenderHost,
    bytes: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.host.0.borrow_mut();
        state.pending -= self.bytes;
        // Cancelled work can unblock siblings without freeing an existing image.
        state.reclaimed = state.reclaimed.wrapping_add(1);
    }
}

impl RenderHost {
    pub fn new(mut policy: HostPolicy) -> Self {
        policy.threads = render_thread_budget(policy.threads).get();
        policy.preparation_tasks = policy.preparation_tasks.max(1);
        Self(Rc::new(RefCell::new(State {
            policy,
            allocations: HashMap::new(),
            retired: HashSet::new(),
            live: 0,
            pending: 0,
            reclaimed: 0,
            depth: 0,
            foreground_workers: 0,
            tasks: 0,
            pixels: 0,
            time: Duration::ZERO,
            deferred: 0,
            warm: HashMap::new(),
            expirations: BTreeSet::new(),
            expired: 0,
            analysis: Background::new(policy.analysis),
            native_frames: 0,
        })))
    }

    /// Nest sibling preparation calls inside one pass. Zero means no foreground
    /// native context; otherwise pass its actual thread count, including caller.
    pub fn begin_pass(&self, foreground_threads: u32) -> RenderPass {
        self.pass(foreground_threads, false)
    }

    /// Hold through native completion. An in-flight analysis job keeps one worker
    /// slot; new jobs wait until this frame and all nested passes finish.
    pub fn begin_frame(&self, preferred_threads: NonZeroU32) -> RenderPass {
        self.pass(preferred_threads.get(), true)
    }

    fn pass(&self, requested: u32, managed: bool) -> RenderPass {
        let mut state = self.0.borrow_mut();
        let pause = state.analysis.pause();
        if state.depth == 0 {
            state.analysis.collect();
        }
        let threads = if managed {
            requested.max(1).min(available_threads(
                state.policy.threads,
                state.foreground_workers,
                pause.running,
            ))
        } else {
            requested
        };
        if state.depth == 0 {
            state.tasks = 0;
            state.pixels = 0;
            state.time = Duration::ZERO;
            state.deferred = 0;
        }
        state.depth += 1;
        let workers = if managed || state.native_frames == 0 {
            threads.saturating_sub(1)
        } else {
            0
        };
        state.foreground_workers += workers;
        state.native_frames += usize::from(managed);
        RenderPass {
            host: self.clone(),
            workers,
            threads: NonZeroU32::new(threads.max(1)).unwrap(),
            managed,
            pause: Some(pause),
        }
    }

    pub(super) fn analyze(&self, program: &PaintProgram) -> bool {
        let mut state = self.0.borrow_mut();
        if state.native_frames == 0 || state.policy.threads < 2 {
            return true;
        }
        state.analysis.submit(program)
    }

    /// Request another paced render pass when true, not an idle polling loop.
    pub fn has_deferred(&self) -> bool {
        self.0.borrow().deferred != 0
    }

    pub fn maintenance_deadline(&self) -> Option<Instant> {
        self.0
            .borrow()
            .expirations
            .first()
            .map(|(deadline, _)| *deadline)
    }

    /// Releases inactive pixels without revisiting a scene or requesting paint.
    pub fn maintain(&self, now: Instant) {
        let mut state = self.0.borrow_mut();
        state.analysis.collect();
        while let Some(&(deadline, id)) = state.expirations.first() {
            if deadline > now {
                break;
            }
            state.take_warm(id);
            state.expired = state.expired.saturating_add(1);
        }
        drop(state);
        self.reap();
    }

    pub(super) fn retain_warm(&self, id: RasterId, image: Rc<RasterImage>, deadline: Instant) {
        let mut state = self.0.borrow_mut();
        assert!(!state.warm.contains_key(&id));
        state.transition(image.id(), 0, 1);
        state.expirations.insert((deadline, id));
        state.warm.insert(id, Warm { image, deadline });
    }

    pub(super) fn has_warm(&self, id: RasterId) -> bool {
        self.0.borrow().warm.contains_key(&id)
    }

    pub(super) fn take_warm(&self, id: RasterId) -> Option<Rc<RasterImage>> {
        self.0.borrow_mut().take_warm(id)
    }

    pub fn stats(&self) -> HostStats {
        self.reap();
        let state = self.0.borrow();
        let mut stats = HostStats {
            live_bytes: state.live,
            pending_bytes: state.pending,
            peak_limit_bytes: state.policy.peak_bytes,
            preparation_tasks: state.tasks,
            preparation_pixels: state.pixels,
            preparation_time: state.time,
            deferred_tasks: state.deferred,
            warm_entries: state.warm.len(),
            expired_entries: state.expired,
            analysis: state.analysis.stats(),
            ..Default::default()
        };
        for allocation in state.allocations.values() {
            if allocation.selected != 0 {
                stats.selected_bytes += allocation.bytes;
            } else if allocation.warm != 0 {
                stats.warm_bytes += allocation.bytes;
            } else {
                stats.retired_bytes += allocation.bytes;
            }
        }
        stats
    }

    pub(super) fn admit(&self, pixels: usize, preferred_threads: u32) -> Option<u32> {
        let mut state = self.0.borrow_mut();
        let policy = state.policy;
        // A native task is indivisible. Permit one eligible large task rather
        // than perpetually deferring it; limits stop subsequent work.
        if state.tasks != 0
            && (state.tasks >= policy.preparation_tasks
                || state.pixels.saturating_add(pixels) > policy.preparation_pixels
                || state.time >= policy.preparation_time)
        {
            state.deferred += 1;
            return None;
        }
        state.tasks += 1;
        state.pixels = state.pixels.saturating_add(pixels);
        Some(preferred_threads.max(1).min(available_threads(
            policy.threads,
            state.foreground_workers,
            state.analysis.running(),
        )))
    }

    pub(super) fn charge(&self, elapsed: Duration) {
        let mut state = self.0.borrow_mut();
        state.time = state.time.saturating_add(elapsed);
    }

    pub(super) fn reserve(&self, bytes: usize) -> Option<Reservation> {
        self.maintain(Instant::now());
        let mut state = self.0.borrow_mut();
        if state
            .live
            .saturating_add(state.pending)
            .saturating_add(bytes)
            > state.policy.peak_bytes
        {
            return None;
        }
        state.pending += bytes;
        Some(Reservation {
            host: self.clone(),
            bytes,
        })
    }

    pub(super) fn track(&self, image: &RasterImage) {
        let mut state = self.0.borrow_mut();
        if !state.allocations.contains_key(&image.id()) {
            state.live += image.byte_len();
            state.allocations.insert(
                image.id(),
                Allocation {
                    storage: image.storage(),
                    bytes: image.byte_len(),
                    selected: 0,
                    warm: 0,
                },
            );
        }
        state.allocations.get_mut(&image.id()).unwrap().selected += 1;
        state.retired.remove(&image.id());
    }

    pub(super) fn transition(&self, id: ResourceId, selected: i32, warm: i32) {
        self.0.borrow_mut().transition(id, selected, warm);
    }

    pub(super) fn reap(&self) {
        let mut state = self.0.borrow_mut();
        let State {
            allocations,
            retired,
            live,
            reclaimed,
            ..
        } = &mut *state;
        retired.retain(|id| {
            if allocations[id].storage.is_alive() {
                return true;
            }
            *live -= allocations.remove(id).unwrap().bytes;
            *reclaimed = reclaimed.wrapping_add(1);
            false
        });
    }

    pub(super) fn reclaimed(&self) -> u64 {
        self.0.borrow().reclaimed
    }
}

fn available_threads(total: u32, native_workers: u32, analysis: bool) -> u32 {
    total
        .saturating_sub(native_workers)
        .saturating_sub(u32::from(analysis))
        .max(1)
}

impl State {
    fn transition(&mut self, id: ResourceId, selected: i32, warm: i32) {
        let allocation = self.allocations.get_mut(&id).expect("tracked allocation");
        allocation.selected = allocation
            .selected
            .checked_add_signed(selected as isize)
            .unwrap();
        allocation.warm = allocation.warm.checked_add_signed(warm as isize).unwrap();
        if allocation.selected == 0 && allocation.warm == 0 {
            self.retired.insert(id);
        } else {
            self.retired.remove(&id);
        }
    }

    fn take_warm(&mut self, id: RasterId) -> Option<Rc<RasterImage>> {
        let warm = self.warm.remove(&id)?;
        self.expirations.remove(&(warm.deadline, id));
        self.transition(warm.image.id(), 0, -1);
        Some(warm.image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_frames_count_native_workers_once_and_do_not_start_analysis_early() {
        use crate::{Bounds, Color, Command};
        let host = RenderHost::new(HostPolicy::desktop());
        let threads = host.0.borrow().policy.threads;
        let frame = host.begin_frame(NonZeroU32::new(threads).unwrap());
        let nested = host.begin_pass(threads);
        assert_eq!(host.0.borrow().foreground_workers, threads - 1);
        assert_eq!(host.admit(100, 6), Some(1));
        let source = PaintProgram::new(
            1,
            (160.0, 160.0),
            1.0,
            Rc::new(
                (0..256)
                    .map(|i| Command::Fill {
                        bounds: Bounds::new((i % 16 * 10) as f32, (i / 16 * 10) as f32, 8.0, 8.0),
                        radius: 0.0,
                        color: Color::rgb(0x228866),
                    })
                    .collect(),
            ),
        )
        .unwrap();
        assert!(host.analyze(&source));
        assert!(!host.stats().analysis.running);
        drop(nested);
        assert_eq!(host.0.borrow().foreground_workers, threads - 1);
        assert!(!source.has_replay_index());
        drop(frame);
        assert_eq!(host.0.borrow().foreground_workers, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.stats().analysis.jobs != 0 {
            host.maintain(Instant::now());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(source.has_replay_index(), threads > 1);
        assert_eq!(host.maintenance_deadline(), None);
        assert!(!host.has_deferred());
    }

    #[test]
    fn running_analysis_reserves_one_concurrency_slot_without_blocking_caller() {
        assert_eq!(available_threads(6, 0, false), 6);
        assert_eq!(available_threads(6, 0, true), 5);
        assert_eq!(available_threads(6, 4, true), 1);
        assert_eq!(available_threads(6, 4, false), 2);
        assert_eq!(available_threads(1, 0, false), 1);
    }

    #[test]
    fn nested_passes_share_work_and_foreground_workers_without_idle_polling() {
        let host = RenderHost::new(HostPolicy {
            preparation_tasks: 2,
            preparation_pixels: usize::MAX,
            preparation_time: Duration::MAX,
            ..HostPolicy::desktop()
        });
        let threads = host.0.borrow().policy.threads;
        {
            let _pass = host.begin_pass(threads);
            assert_eq!(host.admit(10, 6), Some(1));
            {
                let _sibling = host.begin_pass(0);
                assert_eq!(host.admit(20, 6), Some(1));
                assert_eq!(host.admit(30, 6), None);
            }
            assert_eq!(host.stats().preparation_tasks, 2);
            assert_eq!(host.stats().preparation_pixels, 30);
            assert!(host.has_deferred());
        }
        let _next = host.begin_pass(0);
        assert!(!host.has_deferred());
        assert_eq!(host.admit(10, 6), Some(threads));
    }

    #[test]
    fn time_and_pixel_limits_allow_one_indivisible_task_then_yield() {
        let host = RenderHost::new(HostPolicy {
            preparation_pixels: 10,
            preparation_time: Duration::from_micros(10),
            ..HostPolicy::desktop()
        });
        {
            let _pass = host.begin_pass(0);
            assert!(host.admit(11, 1).is_some());
            assert!(host.admit(1, 1).is_none());
        }
        let _next = host.begin_pass(0);
        assert!(host.admit(1, 1).is_some());
        host.charge(Duration::from_micros(11));
        assert!(host.admit(1, 1).is_none());
        assert_eq!(host.stats().preparation_time, Duration::from_micros(11));
    }
}
