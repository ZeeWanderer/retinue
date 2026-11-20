use retinue_ui::{
    Bounds, layout::Available, layout::Layout, layout::LayoutError, layout::LayoutSize,
    layout::Proposal, layout::Subviews,
};

#[derive(PartialEq)]
pub(super) struct RowLayout {
    pub scale: f32,
}

impl RowLayout {
    fn labels(&self, width: f32) -> Proposal {
        Proposal {
            width: Available::Limit((width - 136.0 * self.scale).max(0.0)),
            height: Available::Ideal,
        }
    }
}

impl Layout for RowLayout {
    type Cache = ();

    fn measure(
        &self,
        proposal: Proposal,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<LayoutSize, LayoutError> {
        let width = match proposal.width {
            Available::Limit(width) => width,
            width => {
                children
                    .measure(
                        0,
                        Proposal {
                            width,
                            height: Available::Ideal,
                        },
                    )?
                    .width
                    + 136.0 * self.scale
            }
        };
        let labels = children.measure(0, self.labels(width))?;
        let button = if children.len() > 1 {
            children.measure(1, Proposal::IDEAL)?
        } else {
            LayoutSize::default()
        };
        Ok(LayoutSize::new(
            width,
            labels.height.max(button.height) + 32.0 * self.scale,
        ))
    }

    fn place(
        &self,
        bounds: Bounds,
        children: &mut Subviews<'_, '_>,
        _: &mut (),
    ) -> Result<(), LayoutError> {
        let labels = children.measure(0, self.labels(bounds.width))?;
        let padding = 16.0 * self.scale;
        children.place(
            0,
            Bounds::new(
                bounds.x + padding,
                bounds.y + (bounds.height - labels.height) / 2.0,
                (bounds.width - 136.0 * self.scale).max(0.0),
                labels.height,
            ),
        )?;
        if children.len() == 1 {
            return Ok(());
        }
        let button = children.measure(1, Proposal::IDEAL)?;
        children.place(
            1,
            Bounds::new(
                bounds.x + bounds.width - padding - button.width,
                bounds.y + (bounds.height - button.height) / 2.0,
                button.width,
                button.height,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use retinue_render::{Color, Key};
    use retinue_ui::{authoring::*, environment::Environment, ir::InstanceGraph, state::State};

    #[test]
    fn row_can_remove_and_restore_its_optional_action() {
        let button = State::new(true);
        let read = button.clone();
        let key = Key::new("row", 0);
        let mut graph = InstanceGraph::default();
        let root = graph.mount_body(move |_| {
            ui! {
                LayoutView(RowLayout { scale: 1.0 }) {
                    Rectangle(Color::rgb(0xffffff)).frame(Frame::new().width(100.0).height(40.0))
                    if read.get() {
                        Rectangle(Color::rgb(0x888888)).frame(Frame::new().width(80.0).height(34.0))
                    }
                }.named(key).frame(Frame::new().width(300.0))
            }
        }, Environment::default()).unwrap();
        graph.set_root(Some(root)).unwrap();
        for present in [true, false, true] {
            button.set(present);
            assert!(
                graph
                    .advance((400, 100), 1.0, None)
                    .unwrap()
                    .completion
                    .unwrap()
                    .is_ready()
            );
            let row = graph.geometry(graph.unique(key).unwrap().unwrap()).unwrap();
            assert_eq!(row.layout.height, 72.0);
        }
    }
}
