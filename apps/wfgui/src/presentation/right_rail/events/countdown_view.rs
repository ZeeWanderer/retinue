use super::*;

#[derive(PartialEq)]
pub(super) struct CountdownView {
    pub countdown: Countdown,
    pub font: TextStyle,
    pub size: f32,
    pub center: bool,
    pub baro: bool,
}

impl View for CountdownView {
    fn body(&self, cx: &mut ViewContext<'_>) -> impl IntoElement {
        let time = cx.time(self.countdown);
        label(
            self.countdown.label(milliseconds(time), self.baro),
            self.font.font.clone(),
            self.size,
            TEXT,
            self.center,
        )
    }
}
