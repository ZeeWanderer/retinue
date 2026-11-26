//! Owner-thread census of cold paragraph inputs. IDs are local to one recording.
use rustc_hash::FxHashMap;
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::Arc};

type Shape = ((u64, u32), u32, Arc<str>);
type Placement = (usize, Option<u32>, Option<u8>, Option<usize>);

#[derive(Default)]
struct Census {
    shapes: FxHashMap<Shape, usize>,
    placements: FxHashMap<Placement, usize>,
    builds: Vec<[usize; 2]>,
}

thread_local! { static ACTIVE: RefCell<Option<Census>> = const { RefCell::new(None) }; }

pub struct ParagraphTrace(PhantomData<Rc<()>>);

impl ParagraphTrace {
    pub fn start() -> Self {
        ACTIVE.with_borrow_mut(|state| {
            assert!(state.is_none(), "paragraph census already active");
            *state = Some(Census::default());
        });
        Self(PhantomData)
    }

    /// Pairs of [shape ID, placement ID] in build order; never exports label text.
    pub fn take_frame(&self) -> Vec<[usize; 2]> {
        ACTIVE.with_borrow_mut(|state| std::mem::take(&mut state.as_mut().unwrap().builds))
    }
}

impl Drop for ParagraphTrace {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|state| *state = None);
    }
}

pub(crate) fn built(
    text: &Arc<str>,
    face: (u64, u32),
    size: f32,
    width: Option<f32>,
    align: Option<u8>,
    limit: Option<usize>,
) {
    ACTIVE.with_borrow_mut(|state| {
        let Some(c) = state else { return };
        let next = c.shapes.len();
        let shape = *c
            .shapes
            .entry((face, size.to_bits(), text.clone()))
            .or_insert(next);
        let next = c.placements.len();
        let placed = *c
            .placements
            .entry((shape, width.map(f32::to_bits), align, limit))
            .or_insert(next);
        c.builds.push([shape, placed]);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_distinguish_shaping_from_placement_and_reset_between_traces() {
        let trace = ParagraphTrace::start();
        let emit = |text: &str, face, size, width, align, limit| {
            built(&Arc::from(text), face, size, width, align, limit);
        };
        emit("x", (1, 0), 12., None, None, None);
        emit("x", (1, 0), 12., Some(20.), Some(0), None);
        emit("x", (1, 0), 12., Some(20.), Some(1), None);
        emit("x", (1, 0), 12., Some(20.), Some(1), Some(1));
        assert_eq!(trace.take_frame(), [[0, 0], [0, 1], [0, 2], [0, 3]]);
        emit("x", (1, 0), 12., Some(20.), Some(0), None);
        emit("x", (1, 1), 12., None, None, None);
        emit("x", (1, 0), 16., None, None, None);
        emit("y", (1, 0), 12., None, None, None);
        assert_eq!(trace.take_frame(), [[0, 1], [1, 4], [2, 5], [3, 6]]);
        drop(trace);
        let fresh = ParagraphTrace::start();
        emit("y", (1, 0), 12., None, None, None);
        assert_eq!(fresh.take_frame(), [[0, 0]]);
    }
}
