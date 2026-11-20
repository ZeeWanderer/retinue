use super::super::Geometry;
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

pub(super) struct Placement {
    child: usize,
    slot: usize,
    key: Key,
    input: LayoutInput,
    layout: Layout,
}

impl Placement {
    pub(super) fn new(
        context: &LayoutContext<'_>,
        child: usize,
        bounds: Bounds,
    ) -> Result<Self, LayoutError> {
        let key = context
            .child_key(child)
            .ok_or(LayoutError::InvalidChild(child))?;
        let placed = context
            .placed
            .as_ref()
            .ok_or(LayoutError::PlacementDuringMeasurement)?;
        if placed[child] {
            return Err(LayoutError::DuplicatePlacement(key));
        }
        if !bounds.x.is_finite()
            || !bounds.y.is_finite()
            || !LayoutSize::new(bounds.width, bounds.height).valid()
        {
            return Err(LayoutError::InvalidGeometry);
        }
        let slot = context.tree.inputs[context.node].as_ref().unwrap().children[child];
        let style = &context.tree.inputs[slot].as_ref().unwrap().style;
        let size = Size {
            width: bounds.width,
            height: bounds.height,
        };
        Ok(Self {
            child,
            slot,
            key,
            input: LayoutInput {
                sizing_mode: taffy::SizingMode::InherentSize,
                axis: taffy::RequestedAxis::Both,
                known_dimensions: size.map(Some),
                parent_size: context.parent_size,
                available_space: size.map(AvailableSpace::Definite),
                ..context.input
            },
            layout: Layout {
                order: child as u32,
                location: Point {
                    x: bounds.x,
                    y: bounds.y,
                },
                padding: style
                    .padding
                    .resolve_or_zero(context.parent_size.width, &|_, _| 0.0),
                border: style
                    .border
                    .resolve_or_zero(context.parent_size.width, &|_, _| 0.0),
                ..Layout::default()
            },
        })
    }

    pub(super) fn run(&self, tree: &mut Computation<'_>) -> Result<(), LayoutError> {
        let output = tree.compute_child_layout(self.slot.into(), self.input);
        if tree.state.failure.is_some() {
            return Err(LayoutError::ChildFailed(self.key));
        }
        tree.set_unrounded_layout(
            self.slot.into(),
            &Layout {
                size: output.size,
                ..self.layout
            },
        );
        Ok(())
    }
}

pub(super) trait GroupGeometry {
    fn at(&self, slot: usize) -> &Geometry;
    fn at_mut(&mut self, slot: usize) -> &mut Geometry;
}

struct Partition<'a, 'nodes> {
    local: &'a [usize],
    nodes: &'a mut [&'nodes mut Geometry],
}

impl GroupGeometry for Partition<'_, '_> {
    fn at(&self, slot: usize) -> &Geometry {
        self.nodes[self.local[slot]]
    }

    fn at_mut(&mut self, slot: usize) -> &mut Geometry {
        self.nodes[self.local[slot]]
    }
}

struct Batch<'a> {
    #[cfg(feature = "layout-profiling")]
    cpu: u64,
    requests: Vec<Placement>,
    slots: Vec<usize>,
    nodes: Vec<&'a mut Geometry>,
    state: State,
    result: Result<(), LayoutError>,
}

// Extra ready batches let workers balance siblings with different text lengths.
const BATCHES_PER_PARTICIPANT: usize = 4;

impl LayoutContext<'_> {
    pub fn place_each(
        &mut self,
        mut bounds: impl FnMut(usize, Key) -> Result<Bounds, LayoutError>,
    ) -> Result<(), LayoutError> {
        let count = self.child_count();
        let width = self
            .tree
            .parallel
            .as_ref()
            .map_or(1, |p| p.participants().get());
        if width == 1 || count < 2 || self.tree.minimum_batch_nodes == usize::MAX {
            for child in 0..count {
                let key = self.child_key(child).unwrap();
                self.place_child(child, bounds(child, key)?)?;
            }
            return Ok(());
        }
        let mut requests = Vec::new();
        for child in 0..count {
            let request =
                Placement::new(self, child, bounds(child, self.child_key(child).unwrap())?)?;
            if self
                .tree
                .cache_get(request.slot.into(), &request.input)
                .is_some()
            {
                request.run(&mut self.tree)?;
                self.placed.as_mut().unwrap()[child] = true;
            } else {
                requests.push(request);
            }
        }
        if requests.len() < 2 {
            return self.place_serial(requests);
        }
        #[cfg(feature = "layout-profiling")]
        let split = crate::profiling::layout::thread_cpu_ns();
        let inputs = self.tree.inputs;
        let mut groups = Vec::new();
        let chunk = requests
            .len()
            .div_ceil(width.saturating_mul(BATCHES_PER_PARTICIPANT));
        let mut requests = requests.into_iter();
        let mut total = 0;
        let mut owner_layout = false;
        loop {
            let requests: Vec<_> = requests.by_ref().take(chunk).collect();
            if requests.is_empty() {
                break;
            }
            let mut slots: Vec<_> = requests.iter().map(|request| request.slot).collect();
            let mut cursor = 0;
            while cursor < slots.len() {
                let input = inputs[slots[cursor]].as_ref().unwrap();
                owner_layout |= input.kind == Kind::Owner;
                slots.extend(input.children.iter().copied());
                cursor += 1;
            }
            total += slots.len();
            groups.push(Batch {
                #[cfg(feature = "layout-profiling")]
                cpu: 0,
                requests,
                slots,
                nodes: Vec::new(),
                state: State::default(),
                result: Ok(()),
            });
        }
        if owner_layout || total < self.tree.minimum_batch_nodes {
            #[cfg(feature = "layout-profiling")]
            {
                self.tree.state.batch_cpu.split +=
                    crate::profiling::layout::thread_cpu_ns() - split;
            }
            return self.place_serial(groups.into_iter().flat_map(|batch| batch.requests));
        }
        #[cfg(feature = "layout-profiling")]
        let map = crate::profiling::layout::thread_cpu_ns();
        let mut mapping = vec![usize::MAX; inputs.len()];
        let (mut first, mut last) = (usize::MAX, 0);
        for (id, group) in groups.iter_mut().enumerate() {
            group.nodes.reserve_exact(group.slots.len());
            for &slot in &group.slots {
                assert_eq!(
                    mapping[slot],
                    usize::MAX,
                    "overlapping child layout batches"
                );
                mapping[slot] = id;
                first = first.min(slot);
                last = last.max(slot);
            }
        }
        #[cfg(feature = "layout-profiling")]
        let gather = crate::profiling::layout::thread_cpu_ns();
        // One mutable traversal lends each requested slot to exactly one group.
        for (owner, node) in mapping[first..=last]
            .iter_mut()
            .zip(&mut self.tree.state.nodes[first..=last])
        {
            if *owner != usize::MAX {
                let group = &mut groups[*owner];
                *owner = group.nodes.len();
                group.nodes.push(node.as_mut().unwrap());
            }
        }
        #[cfg(feature = "layout-profiling")]
        let scope = crate::profiling::layout::thread_cpu_ns();
        #[cfg(feature = "layout-profiling")]
        let wall = std::time::Instant::now();
        let completed = catch_unwind(AssertUnwindSafe(|| {
            self.tree
                .parallel
                .as_deref_mut()
                .unwrap()
                .for_each(&mut groups, |group| {
                    #[cfg(feature = "layout-profiling")]
                    let started = crate::profiling::layout::thread_cpu_ns();
                    let mut tree = Computation::new(inputs, &mut group.state, None);
                    let mut partition = Partition {
                        local: &mapping,
                        nodes: &mut group.nodes,
                    };
                    tree.partition = Some(&mut partition);
                    group.result = group
                        .requests
                        .iter()
                        .try_for_each(|request| request.run(&mut tree));
                    #[cfg(feature = "layout-profiling")]
                    {
                        group.cpu = crate::profiling::layout::thread_cpu_ns() - started;
                    }
                });
        }));
        #[cfg(feature = "layout-profiling")]
        let merge = {
            let end = crate::profiling::layout::thread_cpu_ns();
            self.tree.state.batch_cpu.wall += wall.elapsed().as_nanos() as u64;
            self.tree.state.batch_cpu.scope += end - scope;
            self.tree.state.batch_cpu.split += scope - split;
            self.tree.state.batch_cpu.mapping += gather - map;
            self.tree.state.batch_cpu.gather += scope - gather;
            self.tree.state.batch_cpu.slots += total;
            self.tree.state.batch_cpu.groups += groups.len() as u32;
            self.tree.state.batch_cpu.work += groups.iter().map(|group| group.cpu).sum::<u64>();
            end
        };
        let mut result = Ok(());
        for group in groups {
            self.tree.state.computed += group.state.computed;
            for (slot, work) in group.state.changed.iter() {
                self.tree.state.changed.mark(slot, work);
            }
            if self.tree.state.failure.is_none() {
                self.tree.state.failure = group.state.failure;
            }
            if result.is_ok() {
                result = group.result;
            }
            for request in group.requests {
                self.placed.as_mut().unwrap()[request.child] = true;
            }
        }
        #[cfg(feature = "layout-profiling")]
        {
            self.tree.state.batch_cpu.merge += crate::profiling::layout::thread_cpu_ns() - merge;
        }
        if let Err(panic) = completed {
            resume_unwind(panic);
        }
        result
    }

    fn place_serial(
        &mut self,
        requests: impl IntoIterator<Item = Placement>,
    ) -> Result<(), LayoutError> {
        for request in requests {
            request.run(&mut self.tree)?;
            self.placed.as_mut().unwrap()[request.child] = true;
        }
        Ok(())
    }
}
