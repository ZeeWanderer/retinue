use super::row_view::RowView;
use crate::presentation::widgets::compiled::slot;
use retinue_render::Key;
use retinue_ui::{authoring::*, layout::Insets};

#[derive(Clone, PartialEq)]
pub(in crate::presentation::settings) struct SectionView {
    pub id: Key,
    pub rows: Vec<(Option<Key>, RowView)>,
    pub scale: f32,
}

#[view]
impl View for SectionView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            VStack(0.0) {
                ForEach(self.rows.iter(), |(_, row)| row.id) { |(separator, row)|
                    if let Some(id) = separator {
                        slot(*id).frame(Frame::new().height(self.scale).fill_width())
                            .padding(Insets { left: 16.0 * self.scale, right: 16.0 * self.scale, ..Default::default() })
                    }
                    compose(row.clone())
                }
            }.stretch_children().frame(Frame::new().fill_width()).background { slot(self.id) }
        }
    }
}
