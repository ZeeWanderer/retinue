use super::{countdown_view::CountdownView, *};

#[derive(PartialEq)]
pub(super) struct CycleView {
    pub id: Key,
    pub cycle: State<Cycle>,
    pub area: State<(f32, f32)>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

#[view]
impl View for CycleView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let c = self.cycle.get();
        let (w, _) = self.area.get();
        let width = (w - 22.0) / 2.0;
        let name = format!("{} ", c.name.to_uppercase());
        let state = c.state.to_uppercase();
        place(
            ui! {
                ZStack() {
                    fill(rgb(0x101623), 24.5)
                    place(fill(controls::CARD, 24.5), Bounds::new(0.0, 0.0, 49.0, 49.0))
                    place(icon(Key::new("rail.cycle.icon", self.id.value)), Bounds::new(7.0, 7.0, 35.0, 35.0))
                    HStack(0.0) {
                        Text(name).font(self.regular.font.clone()).font_size(13.0).color(Color::from_bgra(MUTED)).line_limit(1)
                        Text(state).font(self.bold.font.clone()).font_size(14.0).color(Color::from_bgra(TEXT)).line_limit(1)
                    }.alignment(VerticalAlignment::Center)
                        .frame(Frame::new().width((width - 57.0).max(0.0)).height(23.0)).offset(51.0, 2.0)
                    place(compose(CountdownView {
                        countdown: Countdown { activation: None, end: c.expiry }, font: self.regular.clone(),
                        size: 14.0, center: true, baro: false,
                    }), Bounds::new(51.0, 24.0, (width - 57.0).max(0.0), 22.0)).named(Key::new("rail.cycle.timer", self.id.value))
                }.alignment(AnchorPoint::TOP_LEADING)
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
