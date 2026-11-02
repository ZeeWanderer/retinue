use super::{Layout, LayoutError, LayoutSize, Proposal, Subviews};
use crate::{Bounds, Key};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorPoint {
    pub x: f32,
    pub y: f32,
}

impl AnchorPoint {
    pub const TOP_LEADING: Self = Self { x: 0.0, y: 0.0 };
    pub const TOP_TRAILING: Self = Self { x: 1.0, y: 0.0 };
    pub const BOTTOM_LEADING: Self = Self { x: 0.0, y: 1.0 };
    pub const BOTTOM_TRAILING: Self = Self { x: 1.0, y: 1.0 };
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };
    fn valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorTarget {
    Container,
    Child(Key),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attachment {
    pub child: Key,
    pub target: AnchorTarget,
    pub target_point: AnchorPoint,
    pub child_point: AnchorPoint,
    pub offset: (f32, f32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnchorError {
    DuplicateChild(Key),
    Cycle(Vec<Key>),
    InvalidGeometry,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anchored {
    alignment: AnchorPoint,
    attachments: Vec<Attachment>,
    order: Vec<usize>,
}

#[derive(Default)]
pub struct AnchorCache {
    children: HashMap<Key, usize>,
    frames: Vec<Bounds>,
}

impl Anchored {
    pub fn new(alignment: AnchorPoint, attachments: Vec<Attachment>) -> Result<Self, AnchorError> {
        if !alignment.valid()
            || attachments.iter().any(|a| {
                !a.target_point.valid()
                    || !a.child_point.valid()
                    || !a.offset.0.is_finite()
                    || !a.offset.1.is_finite()
            })
        {
            return Err(AnchorError::InvalidGeometry);
        }
        let mut index = HashMap::new();
        for (i, attachment) in attachments.iter().enumerate() {
            if index.insert(attachment.child, i).is_some() {
                return Err(AnchorError::DuplicateChild(attachment.child));
            }
        }
        fn visit(
            i: usize,
            attachments: &[Attachment],
            index: &HashMap<Key, usize>,
            marks: &mut [u8],
            stack: &mut Vec<Key>,
            order: &mut Vec<usize>,
        ) -> Result<(), AnchorError> {
            if marks[i] == 2 {
                return Ok(());
            }
            if marks[i] == 1 {
                let start = stack
                    .iter()
                    .position(|key| *key == attachments[i].child)
                    .unwrap();
                let mut cycle = stack[start..].to_vec();
                cycle.push(attachments[i].child);
                return Err(AnchorError::Cycle(cycle));
            }
            marks[i] = 1;
            stack.push(attachments[i].child);
            if let AnchorTarget::Child(target) = attachments[i].target {
                if let Some(&parent) = index.get(&target) {
                    visit(parent, attachments, index, marks, stack, order)?;
                }
            }
            stack.pop();
            marks[i] = 2;
            order.push(i);
            Ok(())
        }
        let mut order = Vec::with_capacity(attachments.len());
        let mut marks = vec![0; attachments.len()];
        let mut stack = Vec::new();
        for i in 0..attachments.len() {
            visit(i, &attachments, &index, &mut marks, &mut stack, &mut order)?;
        }
        Ok(Self {
            alignment,
            attachments,
            order,
        })
    }

    pub fn attachments(&self) -> &[Attachment] {
        &self.attachments
    }

    fn validate(&self, cache: &AnchorCache) -> Result<(), LayoutError> {
        for attachment in &self.attachments {
            if cache.children.contains_key(&attachment.child) {
                if let AnchorTarget::Child(target) = attachment.target {
                    if !cache.children.contains_key(&target) {
                        return Err(LayoutError::MissingChild(target));
                    }
                }
            }
        }
        Ok(())
    }
}

impl Layout for Anchored {
    type Cache = AnchorCache;

    fn update_cache(
        &self,
        cache: &mut AnchorCache,
        children: &mut Subviews<'_, '_>,
    ) -> Result<(), LayoutError> {
        cache.children.clear();
        cache
            .children
            .extend((0..children.len()).map(|index| (children.key(index).unwrap(), index)));
        cache.frames.resize(children.len(), Bounds::default());
        self.validate(cache)
    }

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut AnchorCache,
    ) -> Result<LayoutSize, LayoutError> {
        let mut size = LayoutSize::ZERO;
        for index in 0..children.len() {
            if self
                .attachments
                .iter()
                .any(|attachment| Some(attachment.child) == children.key(index))
            {
                continue;
            }
            let child = children.measure(index, proposal)?;
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        Ok(size)
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        cache: &mut AnchorCache,
    ) -> Result<(), LayoutError> {
        for index in 0..children.len() {
            let size = children.measure(index, Proposal::bounded(bounds.width, bounds.height))?;
            cache.frames[index] = Bounds::new(
                bounds.x + (bounds.width - size.width) * self.alignment.x,
                bounds.y + (bounds.height - size.height) * self.alignment.y,
                size.width,
                size.height,
            );
        }
        for &index in &self.order {
            let attachment = self.attachments[index];
            let Some(&child) = cache.children.get(&attachment.child) else {
                continue;
            };
            let target = match attachment.target {
                AnchorTarget::Container => bounds,
                AnchorTarget::Child(key) => cache.frames[cache.children[&key]],
            };
            let size = cache.frames[child];
            cache.frames[child] = Bounds::new(
                target.x + target.width * attachment.target_point.x
                    - size.width * attachment.child_point.x
                    + attachment.offset.0,
                target.y + target.height * attachment.target_point.y
                    - size.height * attachment.child_point.y
                    + attachment.offset.1,
                size.width,
                size.height,
            );
        }
        for (index, frame) in cache.frames.iter().enumerate() {
            children.place(index, *frame)?;
        }
        Ok(())
    }
}
