use crate::Key;
use retinue_render::CollectionMotion;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildPartition {
    pub key: Key,
    pub children: Range<usize>,
}

/// Advisory partitions of declared children, resolved to paint ranges after layout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collection {
    pub partitions: Vec<ChildPartition>,
    pub motion: CollectionMotion,
}
