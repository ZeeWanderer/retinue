//! Opt-in, exclusive owner-thread accounting for diagnostic benchmark builds.
//! Worker time is excluded from owner phases; layout-profiling measures it separately.

use std::{cell::RefCell, marker::PhantomData, rc::Rc, time::Instant};

#[cfg(feature = "layout-profiling")]
pub mod layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Phase {
    Host,
    Body,
    Ir,
    Bindings,
    Reconcile,
    Layout,
    Projection,
    Record,
    Programs,
    Input,
    Prepare,
    Submit,
    Native,
    LayoutWait,
    PrepareWait,
    NativeWait,
}

pub const NAMES: [&str; 16] = [
    "host",
    "body",
    "ir",
    "bindings",
    "reconcile",
    "layout",
    "projection",
    "record",
    "programs",
    "input",
    "prepare",
    "submit",
    "native",
    "layout_wait",
    "prepare_wait",
    "native_wait",
];

#[cfg(feature = "ir-profiling")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum IrPart {
    Other,
    Observe,
    Resolve,
    Stage,
    Topology,
    Publish,
    Retire,
    Keys,
    Composite,
    Renew,
    Names,
}

#[cfg(feature = "ir-profiling")]
pub const IR_NAMES: [&str; 11] = [
    "other",
    "observe",
    "resolve",
    "stage",
    "topology",
    "publish",
    "retire",
    "keys",
    "composite",
    "renew",
    "names",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Location {
    Frame(Phase),
    #[cfg(feature = "layout-profiling")]
    Text(layout::TextPart),
    #[cfg(feature = "ir-profiling")]
    Ir(IrPart),
}

impl From<Phase> for Location {
    fn from(phase: Phase) -> Self {
        match phase {
            #[cfg(feature = "ir-profiling")]
            Phase::Ir => Self::Ir(IrPart::Other),
            _ => Self::Frame(phase),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    #[cfg(feature = "layout-profiling")]
    pub layout: layout::Sample,
    pub phases_ns: [u64; NAMES.len()],
    #[cfg(feature = "ir-profiling")]
    pub ir: IrSample,
    pub total_ns: u64,
}

#[cfg(feature = "ir-profiling")]
#[derive(Clone, Copy, Debug, Default)]
pub struct IrSample {
    pub ns: [u64; IR_NAMES.len()],
    pub calls: [u32; IR_NAMES.len()],
}

struct Accounting {
    #[cfg(feature = "layout-profiling")]
    text_detail: bool,
    #[cfg(feature = "layout-profiling")]
    layout: layout::Trace,
    phase: Location,
    depth: usize,
    last: u64,
    elapsed: [u64; NAMES.len()],
    #[cfg(feature = "ir-profiling")]
    ir: IrSample,
}

impl Accounting {
    fn new() -> Self {
        Self {
            #[cfg(feature = "layout-profiling")]
            text_detail: false,
            #[cfg(feature = "layout-profiling")]
            layout: layout::Trace::default(),
            phase: Phase::Host.into(),
            depth: 0,
            last: 0,
            elapsed: [0; NAMES.len()],
            #[cfg(feature = "ir-profiling")]
            ir: IrSample::default(),
        }
    }

    fn switch(&mut self, phase: impl Into<Location>, now: u64) -> Location {
        let dt = now - self.last;
        #[cfg(feature = "layout-profiling")]
        if self.phase == Location::Frame(Phase::LayoutWait) {
            self.layout.wait(self.last, now);
        }
        match self.phase {
            Location::Frame(p) => self.elapsed[p as usize] += dt,
            #[cfg(feature = "layout-profiling")]
            Location::Text(part) => {
                self.layout.text_time(part, dt);
                self.elapsed[Phase::Layout as usize] += dt;
            }
            #[cfg(feature = "ir-profiling")]
            Location::Ir(part) => {
                self.ir.ns[part as usize] += dt;
                self.elapsed[Phase::Ir as usize] += dt;
            }
        }
        self.last = now;
        std::mem::replace(&mut self.phase, phase.into())
    }

    fn finish(mut self, now: u64) -> Sample {
        assert_eq!(self.depth, 0, "profiling scope outlived its capture");
        self.switch(Phase::Host, now);
        Sample {
            #[cfg(feature = "layout-profiling")]
            layout: self.layout.finish(),
            phases_ns: self.elapsed,
            #[cfg(feature = "ir-profiling")]
            ir: self.ir,
            total_ns: now,
        }
    }
}

struct Active {
    started: Instant,
    accounting: Accounting,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
}

pub struct Capture(PhantomData<Rc<()>>);

impl Capture {
    /// Include nested text costs in the layout phase for this owner-thread capture only.
    #[cfg(feature = "layout-profiling")]
    pub fn start_with_text_layout() -> Self {
        let capture = Self::start();
        ACTIVE.with_borrow_mut(|active| active.as_mut().unwrap().accounting.text_detail = true);
        capture
    }

    pub fn start() -> Self {
        ACTIVE.with_borrow_mut(|active| {
            assert!(active.is_none(), "nested profiling capture");
            *active = Some(Active {
                started: Instant::now(),
                accounting: Accounting::new(),
            });
        });
        Self(PhantomData)
    }

    pub fn finish(self) -> Sample {
        ACTIVE.with_borrow_mut(|active| {
            let Active {
                started,
                accounting,
            } = active.take().expect("active capture");
            accounting.finish(started.elapsed().as_nanos() as u64)
        })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| *active = None);
    }
}

#[must_use]
pub struct Scope {
    previous: Option<(Location, usize)>,
    thread: PhantomData<Rc<()>>,
}

impl Scope {
    #[cfg(feature = "layout-profiling")]
    pub(crate) fn recorded(&self) -> bool {
        self.previous.is_some()
    }

    #[cfg(feature = "layout-profiling")]
    pub(crate) fn text(part: layout::TextPart) -> Self {
        use layout::TextPart::{Guide, Measure, Place};
        Self::open(|accounting| match accounting.phase {
            _ if !accounting.text_detail => None,
            Location::Frame(Phase::Layout) if matches!(part, Measure | Place | Guide) => {
                Some(Location::Text(part))
            }
            Location::Text(_) => Some(Location::Text(part)),
            _ => None,
        })
    }

    pub fn enter(phase: Phase) -> Self {
        Self::open(|_| Some(phase.into()))
    }

    #[cfg(feature = "ir-profiling")]
    pub(crate) fn ir(part: IrPart) -> Self {
        Self::open(|accounting| {
            matches!(accounting.phase, Location::Ir(_)).then_some(Location::Ir(part))
        })
    }

    fn open(location: impl FnOnce(&Accounting) -> Option<Location>) -> Self {
        let previous = ACTIVE.with_borrow_mut(|active| {
            let active = active.as_mut()?;
            let phase = location(&active.accounting)?;
            #[cfg(feature = "layout-profiling")]
            if let Location::Text(part) = phase {
                active.accounting.layout.text_call(part);
            }
            #[cfg(feature = "ir-profiling")]
            if let Location::Ir(part) = phase {
                active.accounting.ir.calls[part as usize] += 1;
            }
            let previous = active
                .accounting
                .switch(phase, active.started.elapsed().as_nanos() as u64);
            active.accounting.depth += 1;
            Some((previous, active.accounting.depth))
        });
        Self {
            previous,
            thread: PhantomData,
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some((phase, depth)) = self.previous {
            ACTIVE.with_borrow_mut(|active| {
                let active = active.as_mut().expect("scope belongs to an active capture");
                assert_eq!(
                    active.accounting.depth, depth,
                    "profiling scopes must be nested"
                );
                active
                    .accounting
                    .switch(phase, active.started.elapsed().as_nanos() as u64);
                active.accounting.depth -= 1;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "layout-profiling")]
    fn text_cost_partitions_owner_layout_and_ignores_paint() {
        use layout::TextPart::*;
        assert!(Scope::text(Measure).previous.is_none());
        {
            let capture = Capture::start();
            {
                let _layout = Scope::enter(Phase::Layout);
                assert!(Scope::text(Measure).previous.is_none());
            }
            assert_eq!(capture.finish().layout.text_calls, [0; 8]);
        }
        let capture = Capture::start_with_text_layout();
        assert!(Scope::text(Shape).previous.is_none());
        {
            let _layout = Scope::enter(Phase::Layout);
            assert!(Scope::text(Reflow).previous.is_none());
            let _measure = Scope::text(Measure);
            {
                let _shape = Scope::text(Shape);
            }
            {
                let _reflow = Scope::text(Reflow);
            }
        }
        {
            let _paint = Scope::enter(Phase::Record);
            assert!(Scope::text(Place).previous.is_none());
        }
        let sample = capture.finish();
        assert_eq!(sample.layout.text_calls, [1, 0, 0, 1, 1, 0, 0, 0]);
        assert!(
            sample.layout.text_ns.iter().sum::<u64>() <= sample.phases_ns[Phase::Layout as usize]
        );
        assert_eq!(sample.phases_ns.iter().sum::<u64>(), sample.total_ns);
    }

    #[test]
    fn nested_phases_partition_time_including_same_phase_recursion() {
        let mut accounting = Accounting::new();
        let host = accounting.switch(Phase::Ir, 10);
        let ir = accounting.switch(Phase::Body, 20);
        let body = accounting.switch(Phase::Body, 25);
        accounting.switch(body, 28);
        accounting.switch(ir, 30);
        accounting.switch(host, 40);
        let result = accounting.finish(50);
        assert_eq!(result.phases_ns[Phase::Host as usize], 20);
        assert_eq!(result.phases_ns[Phase::Ir as usize], 20);
        assert_eq!(result.phases_ns[Phase::Body as usize], 10);
        assert_eq!(result.phases_ns.iter().sum::<u64>(), result.total_ns);
    }

    #[test]
    #[cfg(feature = "ir-profiling")]
    fn ir_detail_partitions_only_the_ir_owner_phase() {
        let mut accounting = Accounting::new();
        let host = accounting.switch(Phase::Ir, 1);
        let ir = accounting.switch(Location::Ir(IrPart::Observe), 3);
        let observe = accounting.switch(Location::Ir(IrPart::Resolve), 8);
        let resolve = accounting.switch(Phase::Body, 10);
        accounting.switch(resolve, 17);
        accounting.switch(observe, 20);
        accounting.switch(ir, 23);
        accounting.switch(host, 24);
        let sample = accounting.finish(25);
        assert_eq!(sample.ir.ns[IrPart::Other as usize], 3);
        assert_eq!(sample.ir.ns[IrPart::Observe as usize], 8);
        assert_eq!(sample.ir.ns[IrPart::Resolve as usize], 5);
        assert_eq!(
            sample.ir.ns.iter().sum::<u64>(),
            sample.phases_ns[Phase::Ir as usize]
        );
        assert_eq!(sample.phases_ns[Phase::Body as usize], 7);

        let capture = Capture::start();
        assert!(Scope::ir(IrPart::Publish).previous.is_none());
        {
            let _ir = Scope::enter(Phase::Ir);
            let _detail = Scope::ir(IrPart::Resolve);
            let _bindings = Scope::enter(Phase::Bindings);
            assert!(Scope::ir(IrPart::Stage).previous.is_none());
        }
        let sample = capture.finish();
        assert_eq!(sample.ir.calls[IrPart::Resolve as usize], 1);
        assert_eq!(sample.ir.calls[IrPart::Stage as usize], 0);
        assert_eq!(sample.ir.calls[IrPart::Publish as usize], 0);
        assert_eq!(
            sample.ir.ns.iter().sum::<u64>(),
            sample.phases_ns[Phase::Ir as usize]
        );
    }

    #[test]
    fn capture_is_thread_local_and_resets_between_frames() {
        let capture = Capture::start();
        {
            let _scope = Scope::enter(Phase::Ir);
            std::thread::spawn(|| {
                let scope = Scope::enter(Phase::Body);
                assert!(scope.previous.is_none());
            })
            .join()
            .unwrap();
        }
        let result = capture.finish();
        assert_eq!(result.phases_ns[Phase::Body as usize], 0);
        assert_eq!(result.phases_ns.iter().sum::<u64>(), result.total_ns);
        let result = Capture::start().finish();
        assert_eq!(result.phases_ns[Phase::Ir as usize], 0);
    }

    #[test]
    fn unwinding_restores_capture_state() {
        let result = std::panic::catch_unwind(|| {
            let _capture = Capture::start();
            let _scope = Scope::enter(Phase::Body);
            panic!("test");
        });
        assert!(result.is_err());
        let result = Capture::start().finish();
        assert_eq!(result.phases_ns.iter().sum::<u64>(), result.total_ns);
        assert!(Scope::enter(Phase::Ir).previous.is_none());
    }

    #[test]
    fn compiled_callbacks_have_nested_phases_across_roots() {
        use crate::{authoring::*, environment::Environment, ir::InstanceGraph};
        use std::cell::Cell;

        struct Ink(Rc<Cell<usize>>);
        impl crate::drawing::CanvasContent for Ink {
            fn draw(&self, _: &mut crate::drawing::DrawingContext) {
                ACTIVE.with_borrow(|active| {
                    assert_eq!(
                        active.as_ref().unwrap().accounting.phase,
                        Phase::Record.into()
                    );
                });
                self.0.set(self.0.get() + 1);
            }
        }
        impl PartialEq for Ink {
            fn eq(&self, other: &Self) -> bool {
                Rc::ptr_eq(&self.0, &other.0)
            }
        }
        let paints = Rc::new(Cell::new(0));
        let roots = (0..2)
            .map(|_| {
                let mut graph = InstanceGraph::default();
                let paints = paints.clone();
                let root = graph
                    .mount_body(
                        move |_| {
                            ACTIVE.with_borrow(|active| {
                                assert_eq!(
                                    active.as_ref().unwrap().accounting.phase,
                                    Phase::Body.into()
                                );
                            });
                            Canvas(Ink(paints.clone())).frame(Frame::new().width(20.0).height(20.0))
                        },
                        Environment::default(),
                    )
                    .unwrap();
                graph.set_root(Some(root)).unwrap();
                graph
            })
            .collect::<Vec<_>>();
        let capture = Capture::start();
        for mut graph in roots {
            assert!(
                graph
                    .advance((40, 40), 1.0, None)
                    .unwrap()
                    .completion
                    .unwrap()
                    .is_ready()
            );
            ACTIVE.with_borrow(|active| {
                assert_eq!(
                    active.as_ref().unwrap().accounting.phase,
                    Phase::Host.into()
                );
            });
        }
        let result = capture.finish();
        assert_eq!(paints.get(), 2);
        assert_eq!(result.phases_ns.iter().sum::<u64>(), result.total_ns);
    }
}
