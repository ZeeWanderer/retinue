use super::{
    countdown_view::CountdownView, cycle_view::CycleView, fissure_group_view::FissureGroupView, *,
};

#[derive(PartialEq)]
pub(super) struct EventsView {
    pub area: State<(f32, f32)>,
    pub ready: State<bool>,
    pub baro: State<Countdown>,
    pub cycles: Items<Cycle>,
    pub groups: Vec<Group>,
    pub offset: State<f32>,
    pub extent: State<f32>,
    pub disconnected: State<bool>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

impl View for EventsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (w, h) = self.area.get();
        if !self.ready.get() || w <= 0.0 || h <= 0.0 {
            return empty();
        }
        let extent = self.extent.get();
        ui! {
            ZStack() {
                place(ui! {
                    ZStack() {
                        fill(ACCENT, 12.0)
                        place(icon(Key::new("rail.resurgence", 0)), Bounds::new(5.0, 2.0, 20.0, 20.0))
                        place(label("Prime Resurgence", self.regular.font.clone(), 12.0, TEXT, false), Bounds::new(31.0, 0.0, 116.0, 24.0))
                    }.alignment(Alignment::Start)
                }, Bounds::new(45.0, 44.0, 152.0, 24.0))
                place(ui! {
                    ZStack() {
                        fill(controls::CARD, 12.0)
                        place(CountdownView { countdown: self.baro.get(), font: self.regular.clone(),
                            size: 12.0, center: false, baro: true }, Bounds::new(9.0, 0.0, 124.0, 24.0)).id(Key::new("rail.baro.timer", 0))
                    }.alignment(Alignment::Start)
                }, Bounds::new(w - 157.0, 44.0, 141.0, 24.0))
                for &id in self.cycles.order().keys() {
                    CycleView { id, cycle: self.cycles.item(id).unwrap(), area: self.area.clone(), regular: self.regular.clone(), bold: self.bold.clone() }.id(id)
                }
                place(ui! {
                    ZStack() {
                        fill(rgb(0x101623), 8.0)
                        place(fill(rgb(0x101623), 0.0), Bounds::new(0.0, 10.0, (w - 10.0).max(0.0), (h - 200.0).max(0.0)))
                        place(fill(controls::CARD, 8.0), Bounds::new(0.0, 0.0, (w - 10.0).max(0.0), 28.0))
                        place(fill(controls::CARD, 0.0), Bounds::new(0.0, 14.0, (w - 10.0).max(0.0), 14.0))
                        place(label("Void fissures", self.regular.font.clone(), 17.0, TEXT, false), Bounds::new(6.0, 0.0, 140.0, 28.0))
                        place(icon(Key::new("rail.notification", 0)), Bounds::new(w - 191.0, 5.0, 18.0, 18.0))
                    }.alignment(Alignment::Start)
                }, Bounds::new(5.0, 190.0, (w - 10.0).max(0.0), (h - 190.0).max(0.0)))
                place(ScrollViewport::new(ui! {
                    ZStack() {
                        for (group, era) in self.groups.iter().zip(ERAS) {
                            FissureGroupView { era, group: group.clone(), area: self.area.clone(),
                                offset: self.offset.clone(), regular: self.regular.clone(), bold: self.bold.clone() }.id(key("rail.group.view", era))
                        }
                    }.alignment(Alignment::Start).frame_aligned((w - 14.0).max(0.0), extent, AnchorPoint::TOP_LEADING)
                }).position(0.0, self.offset.get()), Bounds::new(7.0, 224.0, (w - 14.0).max(0.0), (h - 224.0).max(0.0))).id(VIEWPORT)
                if extent == 0.0 {
                    place(label("No active fissures", self.regular.font.clone(), 14.0, MUTED, false), Bounds::new(20.0, 245.0, (w - 40.0).max(0.0), 30.0))
                }
                if self.disconnected.get() {
                    place(label("Disconnected - showing cached events", self.regular.font.clone(), 12.0, BAD, false),
                        Bounds::new(12.0, (h - 28.0).max(0.0), (w - 24.0).max(0.0), 24.0))
                }
            }.alignment(Alignment::Start).frame_aligned(w, h, AnchorPoint::TOP_LEADING)
        }
    }
}
