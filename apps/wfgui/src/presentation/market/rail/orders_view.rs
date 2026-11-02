use super::*;

#[derive(PartialEq)]
pub(super) struct OrdersView {
    pub area: State<(f32, f32)>,
    pub frame: Bounds,
    pub orders: Items<Order>,
    pub offset: State<f32>,
    pub regular: TextStyle,
    pub bold: TextStyle,
}

impl View for OrdersView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (w, _) = self.area.get();
        let offset = self.offset.get();
        let order = self.orders.order();
        let text = |s: &str, size, color| label(s, self.regular.font.clone(), size, color, false);
        place(ScrollViewport::new(ui! {
            ZStack() {
                for &id in order.keys() {
                    let row = self.orders.get(id).unwrap();
                    let y = row.index as f32 * 44.0 - offset;
                    if y < self.frame.height && y + 44.0 > 0.0 {
                        place(ui! {
                            ZStack() {
                                fill(controls::CARD, 5.0)
                                place(text(&row.name, 13.0, TEXT), Bounds::new(8.0, 1.0, (w - 150.0).max(0.0), 22.0))
                                place(text(&row.status, 11.0, GOOD), Bounds::new(8.0, 21.0, (w - 150.0).max(0.0), 18.0))
                                place(text(&row.quantity, 13.0, TEXT), Bounds::new(w - 142.0, 0.0, 44.0, 41.0))
                                place(label(&row.price, self.bold.font.clone(), 14.0, TEXT, false), Bounds::new(w - 86.0, 0.0, 48.0, 41.0))
                                place(slot(Key::new("market.order.currency", id.value)), Bounds::new(w - 42.0, 13.0, 16.0, 16.0))
                                place(slot(Key::new("market.order.hit", id.value)), Bounds::new(0.0, 0.0, (w - 14.0).max(0.0), 41.0)).interactive()
                            }.alignment(Alignment::Start)
                        }, Bounds::new(0.0, row.index as f32 * 44.0, (w - 14.0).max(0.0), 41.0)).id(id)
                    }
                }
            }.alignment(Alignment::Start).frame_aligned((w - 14.0).max(0.0), order.len() as f32 * 44.0, AnchorPoint::TOP_LEADING)
        }).position(0.0, offset), self.frame).id(VIEWPORT).reuse(CacheHint::Conditional)
    }
}
