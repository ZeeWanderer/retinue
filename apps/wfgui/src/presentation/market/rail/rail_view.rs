use super::*;

#[derive(PartialEq)]
pub(super) struct MarketRailView {
    pub area: State<(f32, f32)>,
    pub detail: State<Detail>,
    pub filters: Items<Filter>,
    pub orders: Items<Order>,
    pub suggestions: Items<Suggestion>,
    pub offset: State<f32>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

impl View for MarketRailView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (w, h) = self.area.get();
        let d = self.detail.get();
        let order = self.filters.order();
        let frame = viewport((w, h), order.len(), &d);
        let control = |id, rect| place(slot(id), rect).interactive();
        let text = |s: &str, size, color| label(s, self.regular.font.clone(), size, color, false);
        ui! {
            ZStack() {
                control(SEARCH, Bounds::new(10.0, 45.0, (w - 20.0).max(0.0), 32.0))
                if d.present {
                    place(slot(IMAGE), Bounds::new(13.0, 91.0, 64.0, 64.0))
                    place(label(&d.name, self.bold.font.clone(), 16.0, TEXT, false), Bounds::new(88.0, 90.0, (w - 100.0).max(0.0), 29.0))
                    control(Key::new("market.rail.side", 0), Bounds::new(88.0, 126.0, 68.0, 29.0))
                    control(Key::new("market.rail.side", 1), Bounds::new(164.0, 126.0, 68.0, 29.0))
                    place(slot(REFRESH_ICON), Bounds::new(w - 36.0, 130.0, 20.0, 20.0))
                    control(REFRESH, Bounds::new(w - 42.0, 124.0, 32.0, 32.0))
                    place(ui! {
                        VStack(7.0) {
                            for &id in order.keys() {
                                let f = self.filters.get(id).unwrap();
                                ZStack() {
                                    place(text(&f.label, 13.0, MUTED), Bounds::new(0.0, 0.0, 100.0, 30.0))
                                    control(Key::new("market.filter.control", id.value), f.bounds)
                                }.alignment(Alignment::Start).frame_aligned((w - 24.0).max(0.0), 30.0, AnchorPoint::TOP_LEADING).id(id)
                            }
                        }.alignment(Alignment::Start)
                    }, Bounds::new(12.0, 168.0, (w - 24.0).max(0.0), (order.len() as f32 * 37.0 - 7.0).max(0.0)))
                    if let Some(error) = &d.error {
                        place(text(error, 12.0, BAD), Bounds::new(12.0, frame.y - 75.0, (w - 24.0).max(0.0), 42.0))
                    }
                    place(text("Player / Status", 12.0, MUTED), Bounds::new(12.0, frame.y - 29.0, (w - 24.0).max(0.0), 25.0))
                    place(text("Qty", 12.0, MUTED), Bounds::new(w - 135.0, frame.y - 29.0, 46.0, 25.0))
                    place(text("Price", 12.0, MUTED), Bounds::new(w - 79.0, frame.y - 29.0, 65.0, 25.0))
                    orders_view::OrdersView { area: self.area.clone(), frame, orders: self.orders.clone(), offset: self.offset.clone(),
                        regular: self.regular.clone(), bold: self.bold.clone() }
                    control(SCROLL, frame)
                    if self.orders.order().is_empty() {
                        place(text(if d.loading { "Loading listings..." } else { "No online listings" }, 14.0, MUTED),
                            Bounds::new(16.0, frame.y + 12.0, (w - 32.0).max(0.0), 35.0))
                    }
                    if d.authenticated {
                        place(text("Platinum", 12.0, MUTED), Bounds::new(12.0, h - 110.0, 80.0, 24.0))
                        control(PRICE, Bounds::new(12.0, h - 83.0, 90.0, 30.0))
                        place(text("Quantity", 12.0, MUTED), Bounds::new(116.0, h - 110.0, 80.0, 24.0))
                        control(QUANTITY, Bounds::new(116.0, h - 83.0, 90.0, 30.0))
                        control(POST, Bounds::new(220.0, h - 83.0, (w - 232.0).max(0.0), 30.0))
                    }
                    control(WEBSITE, Bounds::new(12.0, h - 40.0, (w - 24.0).max(0.0), 30.0))
                } else {
                    place(text("Search an item to view current listings", 14.0, MUTED), Bounds::new(18.0, 102.0, (w - 36.0).max(0.0), 50.0))
                }
                for &id in self.suggestions.order().keys() {
                    let item = self.suggestions.get(id).unwrap();
                    control(id, Bounds::new(10.0, 81.0 + item.index as f32 * 30.0, (w - 20.0).max(0.0), 30.0))
                }
            }.alignment(Alignment::Start).frame_aligned(w, h, AnchorPoint::TOP_LEADING)
        }
    }
}
