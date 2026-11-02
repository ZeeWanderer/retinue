use super::{countdown_view::CountdownView, *};

#[derive(PartialEq)]
pub(super) struct CycleView {
    pub id: Key,
    pub cycle: State<Cycle>,
    pub area: State<(f32, f32)>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

impl View for CycleView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let c = self.cycle.get();
        let (w, _) = self.area.get();
        let width = (w - 22.0) / 2.0;
        let name = format!("{} ", c.name.to_uppercase());
        let state = c.state.to_uppercase();
        let nw = text_width(&self.regular.font, &name, 13.0);
        let sw = text_width(&self.bold.font, &state, 14.0);
        let x = 51.0 + ((width - 57.0 - nw - sw) / 2.0).max(0.0);
        place(
            ui! {
                ZStack() {
                    fill(rgb(0x101623), 24.5)
                    place(fill(controls::CARD, 24.5), Bounds::new(0.0, 0.0, 49.0, 49.0))
                    place(icon(Key::new("rail.cycle.icon", self.id.value)), Bounds::new(7.0, 7.0, 35.0, 35.0))
                    place(label(name, self.regular.font.clone(), 13.0, MUTED, false), Bounds::new(x, 2.0, nw, 23.0))
                    place(label(state, self.bold.font.clone(), 14.0, TEXT, false), Bounds::new(x + nw, 2.0, sw + 1.0, 23.0))
                    place(CountdownView {
                        countdown: Countdown { activation: None, end: c.expiry }, font: self.regular.clone(),
                        size: 14.0, center: true, baro: false,
                    }, Bounds::new(51.0, 24.0, (width - 57.0).max(0.0), 22.0)).id(Key::new("rail.cycle.timer", self.id.value))
                }.alignment(Alignment::Start)
            },
            Bounds::new(
                7.0 + (c.index % 2) as f32 * (w - 14.0) / 2.0,
                77.0 + (c.index / 2) as f32 * 56.0,
                width,
                49.0,
            ),
        )
    }
}
