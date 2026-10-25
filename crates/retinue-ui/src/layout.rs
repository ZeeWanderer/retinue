use taffy::TaffyError;
use taffy::prelude::*;

use super::Rect;

struct Node<K> {
    key: K,
    clips_children: bool,
}

pub struct UiTree<K> {
    tree: TaffyTree<Node<K>>,
}

pub struct UiLayout<K> {
    nodes: Vec<ResolvedNode<K>>,
}

struct ResolvedNode<K> {
    key: K,
    bounds: Rect,
    content_bounds: Rect,
    clip: Option<Rect>,
}

impl<K: Clone> UiTree<K> {
    pub fn new() -> Self {
        let mut tree = TaffyTree::new();
        tree.disable_rounding();
        Self { tree }
    }

    pub fn leaf(&mut self, key: K, style: Style) -> Result<NodeId, String> {
        self.tree
            .new_leaf_with_context(
                style,
                Node {
                    key,
                    clips_children: false,
                },
            )
            .map_err(layout_error)
    }

    pub fn row(&mut self, key: K, mut style: Style, children: &[NodeId]) -> Result<NodeId, String> {
        style.display = Display::Flex;
        style.flex_direction = FlexDirection::Row;
        self.branch(key, style, children, false)
    }

    pub fn column(
        &mut self,
        key: K,
        mut style: Style,
        children: &[NodeId],
    ) -> Result<NodeId, String> {
        style.display = Display::Flex;
        style.flex_direction = FlexDirection::Column;
        self.branch(key, style, children, false)
    }

    pub fn grid(
        &mut self,
        key: K,
        mut style: Style,
        children: &[NodeId],
    ) -> Result<NodeId, String> {
        style.display = Display::Grid;
        self.branch(key, style, children, true)
    }

    pub fn stack(
        &mut self,
        key: K,
        mut style: Style,
        children: &[NodeId],
    ) -> Result<NodeId, String> {
        style.display = Display::Grid;
        style.grid_template_columns = vec![fr(1.0_f32)];
        style.grid_template_rows = vec![fr(1.0_f32)];
        for child in children {
            let mut child_style = self.tree.style(*child).map_err(layout_error)?.clone();
            child_style.grid_column = line(1);
            child_style.grid_row = line(1);
            self.tree
                .set_style(*child, child_style)
                .map_err(layout_error)?;
        }
        self.branch(key, style, children, false)
    }

    pub fn compute(
        mut self,
        root: NodeId,
        origin: (u32, u32),
        size: (u32, u32),
    ) -> Result<UiLayout<K>, String> {
        self.tree
            .compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(size.0 as f32),
                    height: AvailableSpace::Definite(size.1 as f32),
                },
            )
            .map_err(layout_error)?;
        let mut nodes = Vec::new();
        self.resolve(root, (origin.0 as f32, origin.1 as f32), None, &mut nodes)?;
        Ok(UiLayout { nodes })
    }

    fn branch(
        &mut self,
        key: K,
        style: Style,
        children: &[NodeId],
        clips_children: bool,
    ) -> Result<NodeId, String> {
        let node = self
            .tree
            .new_with_children(style, children)
            .map_err(layout_error)?;
        self.tree
            .set_node_context(
                node,
                Some(Node {
                    key,
                    clips_children,
                }),
            )
            .map_err(layout_error)?;
        Ok(node)
    }

    fn resolve(
        &self,
        node: NodeId,
        parent_origin: (f32, f32),
        parent_clip: Option<Rect>,
        output: &mut Vec<ResolvedNode<K>>,
    ) -> Result<(), String> {
        let layout = self.tree.layout(node).map_err(layout_error)?;
        let origin = (
            parent_origin.0 + layout.location.x,
            parent_origin.1 + layout.location.y,
        );
        let bounds = Rect {
            x: origin.0.round().max(0.0) as u32,
            y: origin.1.round().max(0.0) as u32,
            width: layout.size.width.round().max(0.0) as u32,
            height: layout.size.height.round().max(0.0) as u32,
        };
        let context = self
            .tree
            .get_node_context(node)
            .ok_or_else(|| "UI node has no context".to_owned())?;
        let left = (layout.border.left + layout.padding.left).round().max(0.0) as u32;
        let right = (layout.border.right + layout.padding.right)
            .round()
            .max(0.0) as u32;
        let top = (layout.border.top + layout.padding.top).round().max(0.0) as u32;
        let bottom = (layout.border.bottom + layout.padding.bottom)
            .round()
            .max(0.0) as u32;
        output.push(ResolvedNode {
            key: context.key.clone(),
            bounds,
            content_bounds: Rect {
                x: bounds.x + left.min(bounds.width),
                y: bounds.y + top.min(bounds.height),
                width: bounds.width.saturating_sub(left.saturating_add(right)),
                height: bounds.height.saturating_sub(top.saturating_add(bottom)),
            },
            clip: parent_clip,
        });
        let child_clip = if context.clips_children {
            Some(parent_clip.map_or(bounds, |clip| clip.intersection(bounds)))
        } else {
            parent_clip
        };
        for child in self.tree.children(node).map_err(layout_error)? {
            self.resolve(child, origin, child_clip, output)?;
        }
        Ok(())
    }
}

impl<K: PartialEq> UiLayout<K> {
    pub fn find_bounds(&self, key: K) -> Option<Rect> {
        self.nodes
            .iter()
            .find(|node| node.key == key)
            .map(|node| node.bounds)
    }

    pub fn bounds(&self, key: K) -> Rect {
        self.node(key).bounds
    }

    pub fn content_bounds(&self, key: K) -> Rect {
        self.node(key).content_bounds
    }

    pub fn clip(&self, key: K) -> Option<Rect> {
        self.node(key).clip
    }

    fn node(&self, key: K) -> &ResolvedNode<K> {
        self.nodes
            .iter()
            .find(|node| node.key == key)
            .expect("resolved UI node")
    }
}

fn layout_error(error: TaffyError) -> String {
    format!("UI layout failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq)]
    enum Element {
        Root,
        Back,
        Front,
    }

    #[test]
    fn stack_children_share_bounds() {
        let mut tree = UiTree::new();
        let back = tree
            .leaf(
                Element::Back,
                Style {
                    size: Size {
                        width: percent(1.0_f32),
                        height: percent(1.0_f32),
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let front = tree
            .leaf(
                Element::Front,
                Style {
                    size: Size {
                        width: percent(1.0_f32),
                        height: percent(1.0_f32),
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let root = tree
            .stack(
                Element::Root,
                Style {
                    size: Size {
                        width: length(80.0_f32),
                        height: length(40.0_f32),
                    },
                    ..Default::default()
                },
                &[back, front],
            )
            .unwrap();
        let layout = tree.compute(root, (10, 20), (80, 40)).unwrap();

        assert_eq!(layout.bounds(Element::Back), layout.bounds(Element::Front));
        assert_eq!(
            layout.bounds(Element::Root),
            Rect {
                x: 10,
                y: 20,
                width: 80,
                height: 40,
            }
        );
    }
}
