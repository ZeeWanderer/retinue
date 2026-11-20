use super::{
    item_view::item_view,
    panel::{CONTENT, MAIN, Node, RAIL},
    slots::slot,
};
use retinue_render::Key;
use retinue_ui::{authoring::*, layout::Insets};

#[derive(Default, PartialEq)]
pub(super) struct EvaluateView {
    pub scale: f32,
    pub origin: (u32, u32),
    pub rows: Vec<(usize, f32)>,
    pub viewport: f32,
    pub header: f32,
    pub estimate: f32,
    pub error: f32,
    pub sections: Vec<(Node, f32)>,
}

#[view]
impl View for EvaluateView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let s = self.scale;
        ui! {
            HStack(0.0) {
                VStack(0.0) {
                    slot(Node::Bar, MAIN * s, 28.0 * s)
                    VStack(0.0) {
                        VStack(0.0) {
                            slot(Node::Header, CONTENT * s, self.header)
                            item_view(s, &self.rows, self.viewport).named(Key::from(Node::Item))
                            slot(Node::Profiles, CONTENT * s, 41.0 * s)
                        }.named(Key::from(Node::Card))
                        if self.estimate > 0.0 {
                            slot(Node::Estimate, CONTENT * s, self.estimate)
                                .padding(Insets { top: 8.0 * s, ..Insets::default() })
                        }
                        if self.error > 0.0 {
                            slot(Node::Error, CONTENT * s, self.error)
                                .padding(Insets { top: 8.0 * s, ..Insets::default() })
                        }
                        ForEach(self.sections.iter().copied(), |(node, _)| Key::from(*node)) { |(node, height)|
                            slot(node, CONTENT * s, height * s).padding(Insets {
                                top: if matches!(node, Node::Search | Node::Status | Node::ResultFilters | Node::Table | Node::RateBudget) {
                                    8.0 * s
                                } else { 0.0 },
                                ..Insets::default()
                            })
                        }
                    }.alignment(HorizontalAlignment::Leading).padding(12.0 * s).named(Key::from(Node::Content))
                }.alignment(HorizontalAlignment::Leading).named(Key::from(Node::Main))
                empty().frame(Frame::new().width(RAIL * s).height(0.0))
            }.alignment(VerticalAlignment::Top).offset(self.origin.0 as f32, self.origin.1 as f32)
                .named(Key::from(Node::Root))
        }
    }
}
