use super::Result;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Local(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A layout node with statically declared children.
    Layout,
    Content,
    /// A layout node whose children are dynamic instances.
    DynamicLayout,
    /// Statically declared siblings, without a layout box.
    Group,
    /// Dynamic instance roots spliced into the arranging parent's children.
    Slot,
}

impl Kind {
    pub(super) fn materialized(self) -> bool {
        matches!(self, Self::Layout | Self::Content | Self::DynamicLayout)
    }

    pub(super) fn dynamic(self) -> bool {
        matches!(self, Self::DynamicLayout | Self::Slot)
    }
}

pub struct Declaration {
    pub parent: Option<Local>,
    pub kind: Kind,
}

pub(super) struct Node {
    pub parent: Option<Local>,
    pub kind: Kind,
    children: Range<usize>,
}

pub struct Structure {
    pub(super) nodes: Box<[Node]>,
    edges: Box<[Local]>,
    roots: Range<usize>,
    pub(super) materialized: usize,
}

impl Structure {
    pub fn compile(declarations: &[Declaration]) -> Result<Self> {
        if declarations.is_empty() || declarations.len() > u32::MAX as usize {
            return Err("invalid template size");
        }
        let mut counts = vec![0; declarations.len()];
        for (index, declaration) in declarations.iter().enumerate() {
            match declaration.parent {
                None if index == 0 => {}
                Some(Local(parent)) if (parent as usize) < index => {
                    if !matches!(
                        declarations[parent as usize].kind,
                        Kind::Layout | Kind::Group
                    ) {
                        return Err("only layouts and groups can have static children");
                    }
                    counts[parent as usize] += 1;
                }
                _ => return Err("template must have one root and parents before children"),
            }
        }
        let mut offset = 0;
        let mut nodes: Box<[_]> = declarations
            .iter()
            .zip(counts)
            .map(|(declaration, count)| {
                let children = offset..offset + count;
                offset += count;
                Node {
                    parent: declaration.parent,
                    kind: declaration.kind,
                    children,
                }
            })
            .collect();
        let mut cursors: Vec<_> = nodes.iter().map(|node| node.children.start).collect();
        let mut edges = vec![Local(0); offset];
        for (index, node) in nodes.iter().enumerate().skip(1) {
            let parent = node.parent.unwrap().0 as usize;
            edges[cursors[parent]] = Local(index as u32);
            cursors[parent] += 1;
        }
        let mut projected = Vec::new();
        let mut scratch = Vec::new();
        let mut ranges = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let start = projected.len();
            if matches!(node.kind, Kind::Layout | Kind::Group) {
                flatten(
                    &edges[node.children.clone()],
                    &nodes,
                    &edges,
                    &mut scratch,
                    &mut projected,
                );
            }
            ranges.push(start..projected.len());
        }
        let start = projected.len();
        flatten(&[Local(0)], &nodes, &edges, &mut scratch, &mut projected);
        let roots = start..projected.len();
        let materialized = nodes.iter().filter(|node| node.kind.materialized()).count();
        for (node, children) in nodes.iter_mut().zip(ranges) {
            node.children = children;
        }
        Ok(Self {
            nodes,
            edges: projected.into_boxed_slice(),
            roots,
            materialized,
        })
    }

    /// Layout children after transparent groups are flattened; slots remain dynamic.
    pub fn layout_children(&self, node: Local) -> &[Local] {
        &self.edges[self.nodes[node.0 as usize].children.clone()]
    }

    pub fn roots(&self) -> &[Local] {
        &self.edges[self.roots.clone()]
    }

    pub fn bytes(&self) -> usize {
        std::mem::size_of_val(&*self.nodes) + std::mem::size_of_val(&*self.edges)
    }
}

fn flatten(
    children: &[Local],
    nodes: &[Node],
    edges: &[Local],
    stack: &mut Vec<Local>,
    out: &mut Vec<Local>,
) {
    stack.extend(children.iter().rev().copied());
    while let Some(local) = stack.pop() {
        let node = &nodes[local.0 as usize];
        if node.kind == Kind::Group {
            stack.extend(edges[node.children.clone()].iter().rev().copied());
        } else {
            out.push(local);
        }
    }
}
