use super::{countdown_view::CountdownView, *};

#[derive(PartialEq)]
pub(super) struct FissureGroupView {
    pub era: &'static str,
    pub group: Group,
    pub area: State<(f32, f32)>,
    pub offset: State<f32>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

impl View for FissureGroupView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let order = self.group.rows.order();
        if order.is_empty() {
            return empty();
        }
        let f = self.group.frame.get();
        let (w, h) = self.area.get();
        let offset = self.offset.get();
        place(ui! {
            ZStack() {
                fill(controls::PANEL, 7.0)
                place(icon(key("rail.era.icon", self.era)), Bounds::new(12.0, (f.height - 39.0) / 2.0, 39.0, 39.0))
                place(icon(key("rail.era.hit", self.era)), Bounds::new(3.0, 0.0, 56.0, f.height)).interactive()
                for &id in order.keys() {
                    let row = self.group.rows.get(id).unwrap();
                    let y = f.y + row.top - offset;
                    if y < (h - 224.0).max(0.0) && y + 25.0 > 0.0 {
                        let title = text_width(&self.bold.font, &row.mission, 13.0) + 6.0;
                        place(ui! {
                            ZStack() {
                                place(icon(Key::new("rail.fissure.icon", id.value)), Bounds::new(62.0, 5.0, 15.0, 15.0))
                                place(label(row.mission, self.bold.font.clone(), 13.0, TEXT, false), Bounds::new(82.0, 0.0, title.min((w - 172.0).max(20.0)), 25.0))
                                place(label(format!("({})", row.node), self.regular.font.clone(), 12.5, TEXT, false),
                                    Bounds::new(82.0 + title, 0.0, (w - 172.0 - title).max(0.0), 25.0))
                                place(CountdownView {
                                    countdown: Countdown { activation: None, end: Some(row.expiry) }, font: self.regular.clone(),
                                    size: 12.5, center: false, baro: false,
                                }, Bounds::new(w - 80.0, 0.0, 62.0, 25.0)).id(Key::new("rail.fissure.timer", id.value))
                                place(icon(Key::new("rail.fissure.hit", id.value)), Bounds::new(60.0, 0.0, (w - 78.0).max(0.0), 25.0)).interactive()
                            }.alignment(Alignment::Start)
                        }, Bounds::new(0.0, row.top, (w - 14.0).max(0.0), 25.0)).id(id)
                    }
                }
            }.alignment(Alignment::Start)
        }, f).id(key("rail.group", self.era))
    }
}
