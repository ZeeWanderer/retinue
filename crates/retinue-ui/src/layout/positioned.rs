use super::{Layout, LayoutError, LayoutSize, Proposal, Subviews, values::LayoutValueKey};
use crate::{Bounds, Key};
use rustc_hash::FxHashMap;

/// Child-local frames for a layout whose geometry is already known.
pub struct Placement;

impl LayoutValueKey for Placement {
    type Value = Option<Bounds>;
    fn default_value() -> Self::Value {
        None
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Positioned {
    size: LayoutSize,
}

impl Positioned {
    pub fn new(size: LayoutSize) -> Self {
        Self { size }
    }
}

impl Layout for Positioned {
    type Cache = ();

    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(self.size)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        place(bounds, children, |_, _| None)
    }
}

#[derive(PartialEq)]
pub(crate) struct ChildFrames {
    pub size: LayoutSize,
    pub frames: FxHashMap<Key, Bounds>,
}

impl Layout for ChildFrames {
    type Cache = ();

    fn measure(
        &self,
        _: Proposal,
        _: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        Ok(self.size)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        place(bounds, children, |children, index| {
            self.frames.get(&children.key(index)?).copied()
        })
    }
}

fn place(
    bounds: Bounds,
    children: &mut Subviews<'_, '_>,
    frame: impl Fn(&Subviews<'_, '_>, usize) -> Option<Bounds>,
) -> Result<(), LayoutError> {
    for index in 0..children.len() {
        let local = match frame(children, index) {
            Some(frame) => frame,
            None => children.value::<Placement>(index)?.unwrap_or(Bounds::new(
                0.0,
                0.0,
                bounds.width,
                bounds.height,
            )),
        };
        children.place(
            index,
            Bounds::new(
                bounds.x + local.x,
                bounds.y + local.y,
                local.width,
                local.height,
            ),
        )?;
    }
    Ok(())
}
