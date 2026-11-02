use super::*;

#[derive(PartialEq)]
pub(super) struct DiscoverView {
    pub metrics: State<Metrics>,
    pub model: State<Rc<build_planner_model::Model>>,
    pub state: State<PageState>,
    pub fonts: Fonts,
    pub rows: Items<(usize, build_planner_model::SourceRow)>,
    pub offset: State<f32>,
}

impl View for DiscoverView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        let model = self.model.get();
        let d = &model.discover;
        let state = self.state.get();
        let c = |id, rect| control(node(id), rect);
        ui! {
            ZStack() {
                c("query", m.bounds(14.0, 78.0, m.width - 305.0, 33.0))
                c("scope", m.bounds(m.width - 282.0, 78.0, 165.0, 33.0))
                c("search", m.bounds(m.width - 107.0, 78.0, 93.0, 33.0))
                c("ordering", m.bounds(14.0, 121.0, 150.0, 30.0))
                if d.detail {
                    c("back", m.bounds(m.width - 102.0, 121.0, 88.0, 30.0))
                    if state.added { c("added", m.bounds(m.width - 222.0, 121.0, 110.0, 30.0)) }
                    place(label(&self.fonts, &d.title, 20.0, TEXT, true, m.scale), m.bounds(14.0, 164.0, m.width - 28.0, 35.0))
                    place(label(&self.fonts, &d.summary, 13.0, MUTED, false, m.scale), m.bounds(14.0, 202.0, m.width - 290.0, 27.0))
                    c("add", m.bounds(m.width - 264.0, 201.0, 145.0, 31.0))
                    c("website", m.bounds(m.width - 109.0, 201.0, 95.0, 31.0))
                } else {
                    place(ScrollViewport::new(ui! {
                        ZStack() {
                            for &id in self.rows.order().keys() {
                                let (i, _) = self.rows.get(id).unwrap();
                                control(id, m.bounds(0.0, i as f32 * 74.0, m.width - 24.0, 67.0))
                            }
                        }.alignment(Alignment::Start).frame_aligned(m.source_list().width, d.rows.len() as f32 * 74.0 + 20.0, AnchorPoint::TOP_LEADING)
                    }).position(0.0, self.offset.get()), m.source_list()).reuse(CacheHint::Conditional)
                    control(BAR, m.source_list())
                    if d.rows.is_empty() {
                        place(label(&self.fonts, if state.searching { "Loading builds..." } else { "Search equipment to browse builds" },
                            16.0, MUTED, false, m.scale), m.bounds(24.0, 196.0, m.width - 48.0, 40.0))
                    }
                    if d.sources {
                        c("previous", m.bounds(14.0, m.height - 39.0, 94.0, 30.0))
                        c("next", m.bounds(m.width - 108.0, m.height - 39.0, 94.0, 30.0))
                    }
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, AnchorPoint::TOP_LEADING)
        }
    }
}
