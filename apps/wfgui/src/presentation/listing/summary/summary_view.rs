use super::*;

#[derive(PartialEq)]
pub(super) struct SummaryView {
    pub data: State<Data>,
    pub pending: State<usize>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

impl View for SummaryView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let data = self.data.read();
        let m = self.metrics.read();
        let g = Geometry::new(&data, &m, &self.fonts.regular);
        let label = |text: String, size, color, alignment, font| {
            aligned_label(text, font, size, color, alignment, m.scale)
        };
        ui! {
            ZStack() {
                place(empty(), m.bounds(12.0, 8.0, m.width - 24.0, m.summary_bottom)).id(ROOT)
                place(label(data.level.clone(), 51.0, TEXT, 0.0, self.fonts.regular.clone()), m.bounds(12.0, 8.0, 66.0, 65.0))
                place(slot(RANK), m.bounds(81.0, 10.0, 60.0, 60.0))
                place(fill(controls::CARD, 9.5), m.bounds(154.0, 46.0, (m.width - 170.0).max(1.0), 19.0))
                if data.extension > 0.0 {
                    place(fill(crate::presentation::settings::rgb(0xcfb14d), 9.5), m.bounds(154.0, 46.0,
                        (m.width - 170.0).max(1.0) * (data.progress + data.extension).min(1.0), 19.0))
                }
                place(fill(ACCENT, 9.5), m.bounds(154.0, 46.0, (m.width - 170.0).max(1.0) * data.progress, 19.0))
                place(label(data.percent.clone(), 17.0, TEXT, 0.0, self.fonts.regular.clone()), m.bounds(154.0, 17.0, 85.0, 25.0))
                place(label(data.xp.clone(), 17.0, TEXT, 0.0, self.fonts.regular.clone()), g.xp)
                for (i, ((title, _, _), panel)) in TITLES.iter().zip(&data.panels).enumerate() {
                    let p = &g.panels[i];
                    place(ui! {
                        ZStack() {
                            fill(controls::CARD, 10.0)
                            place(fill(crate::presentation::style::SUMMARY_HEADER, 10.0), Bounds::new(0.0, 0.0, p.bounds.width, 40.0))
                            place(fill(crate::presentation::style::SUMMARY_HEADER, 0.0), p.cap)
                            place(slot(Key::new("mastery.summary.icon", i as u64)), p.icon)
                            place(label(title.to_string(), 17.0, TEXT, 0.0, self.fonts.regular.clone()), p.title)
                            place(label(panel.percent.clone(), 17.0, crate::presentation::style::SUMMARY_PERCENT, 0.0, self.fonts.regular.clone()), p.percent)
                            for (j, row) in panel.rows.iter().enumerate() {
                                for (column, text) in [&row.label, &row.percent, &row.fraction].into_iter().enumerate() {
                                    place(label(text.clone(), 14.67, TEXT, if column == 0 { 1.0 } else { 0.5 },
                                        if column == 0 { self.fonts.regular.clone() } else { self.fonts.light.clone() }), p.rows[j][column])
                                }
                            }
                        }.alignment(Alignment::Start)
                    }, p.bounds).id(Key::new("mastery.summary.panel", i as u64))
                }
                if self.pending.get() > 0 {
                    place(slot(LOADING), m.bounds(16.0, m.summary_bottom + 8.0, m.width - 32.0, 3.0))
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING)
        }
    }
}
