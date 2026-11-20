use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct SummaryView {
    pub data: State<Data>,
    pub pending: State<usize>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

#[view]
impl View for SummaryView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let data = self.data.read();
        let m = self.metrics.read();
        let g = Geometry::new(&data, &m, &self.fonts.regular);
        let label = |text: String, size, color, alignment, font| {
            aligned_label(text, font, size, color, alignment)
        };
        ui! {
            ZStack() {
                place(empty(), m.bounds(12.0, 8.0, m.width - 24.0, m.summary_bottom)).named(ROOT)
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
                ForEach(TITLES.iter().zip(&data.panels).enumerate(), |(i, _)| *i as u64) { |(i, ((title, _, _), panel))|
                    let p = &g.panels[i];
                    place(ui! {
                        ZStack() {
                            fill(controls::CARD, 10.0)
                            place(fill(crate::presentation::style::SUMMARY_HEADER, 10.0), Bounds::new(0.0, 0.0, p.bounds.width, 40.0))
                            place(fill(crate::presentation::style::SUMMARY_HEADER, 0.0), p.cap)
                            place(slot(Key::new("mastery.summary.icon", i as u64)), p.icon)
                            place(label(title.to_string(), 17.0, TEXT, 0.0, self.fonts.regular.clone()), p.title)
                            place(label(panel.percent.clone(), 17.0, crate::presentation::style::SUMMARY_PERCENT, 0.0, self.fonts.regular.clone()), p.percent)
                            ForEach(panel.rows.iter().enumerate(), |(j, _)| *j as u64) { |(j, row)|
                                place(label(row.label.clone(), 14.67, TEXT, 1.0, self.fonts.regular.clone()), p.rows[j][0])
                                place(label(row.percent.clone(), 14.67, TEXT, 0.5, self.fonts.light.clone()), p.rows[j][1])
                                place(label(row.fraction.clone(), 14.67, TEXT, 0.5, self.fonts.light.clone()), p.rows[j][2])
                            }
                        }.alignment(AnchorPoint::TOP_LEADING)
                    }, p.bounds).named(Key::new("mastery.summary.panel", i as u64))
                }
                if self.pending.get() > 0 {
                    place(slot(LOADING), m.bounds(16.0, m.summary_bottom + 8.0, m.width - 32.0, 3.0))
                }
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, m.height))
        }
    }
}
