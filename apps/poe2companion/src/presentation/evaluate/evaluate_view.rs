use super::{
    item_view::ItemView,
    panel::{CONTENT, MAIN, Node, RAIL},
    slots::slot,
};
use retinue_render::Key;
use retinue_ui::prelude::*;

#[derive(PartialEq)]
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

impl View for EvaluateView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let s = self.scale;
        ui! {
            HStack(0.0) {
                VStack(0.0) {
                    slot(Node::Bar, MAIN * s, 28.0 * s)
                    VStack(0.0) {
                        VStack(0.0) {
                            slot(Node::Header, CONTENT * s, self.header)
                            ItemView { scale: s, rows: self.rows.clone(), height: self.viewport }
                                .id(Key::from(Node::Item))
                            slot(Node::Profiles, CONTENT * s, 41.0 * s)
                        }.id(Key::from(Node::Card))
                        if self.estimate > 0.0 {
                            slot(Node::Estimate, CONTENT * s, self.estimate)
                                .padding(Insets { top: 8.0 * s, ..Insets::default() })
                        }
                        if self.error > 0.0 {
                            slot(Node::Error, CONTENT * s, self.error)
                                .padding(Insets { top: 8.0 * s, ..Insets::default() })
                        }
                        for (node, height) in &self.sections {
                            slot(*node, CONTENT * s, height * s).padding(Insets {
                                top: if matches!(node, Node::Search | Node::Status | Node::ResultFilters | Node::Table | Node::RateBudget) {
                                    8.0 * s
                                } else { 0.0 },
                                ..Insets::default()
                            })
                        }
                    }.alignment(Alignment::Start).padding(12.0 * s).id(Key::from(Node::Content))
                }.alignment(Alignment::Start).id(Key::from(Node::Main))
                empty().frame(RAIL * s, 0.0)
            }.alignment(Alignment::Start).offset(self.origin.0 as f32, self.origin.1 as f32)
                .id(Key::from(Node::Root))
        }
    }
}
