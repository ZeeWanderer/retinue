use super::*;
use retinue_ui::{drawing::DrawList, drawing::PixelCanvas, drawing::TextStyle, semantics::Role};

#[derive(PartialEq)]
pub(super) struct OverlaysView {
    pub filters: State<Option<FilterGeometry>>,
    pub menu: State<Option<Menu>>,
    pub text: TextStyle,
}

impl View for OverlaysView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        ui! {
            ZStack() {
                if let Some(f) = self.filters.get() {
                    let content = ui! {
                        ZStack() {
                            for i in 0..f.groups {
                                place(slot(Key::new("listing.filter.title", i as u64)),
                                    f.rect(0.0, i as f32 * 64.0, f.bounds.width - 32.0, 19.0))
                                for j in 0..2 {
                                    let w = (f.bounds.width - 42.0).max(0.0) / 2.0;
                                    place(slot(Key::new("listing.filter.choice", (i * 2 + j) as u64)),
                                        f.rect(j as f32 * (w + 10.0), i as f32 * 64.0 + 23.0, w, 32.0)).interactive()
                                }
                            }
                        }.alignment(Alignment::Start).frame_aligned(f.viewport().width, f.groups as f32 * 64.0,
                            retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING)
                    };
                    place(ui! {
                        ZStack() {
                            slot(PANEL)
                            place(slot(HEADER), Bounds::new(0.0, 0.0, f.bounds.width, 40.0))
                            place(slot(CLOSE), f.rect(f.bounds.width - 36.0, 4.0, 32.0, 32.0)).interactive()
                            place(ScrollViewport::new(content).position(0.0, f.offset), f.viewport()).id(CONTENT)
                            place(slot(SCROLLBAR), f.viewport()).interactive()
                            place(slot(CLEAR), f.rect(12.0, f.bounds.height - 42.0, f.bounds.width - 24.0, 30.0)).interactive()
                        }.alignment(Alignment::Start)
                    }, f.bounds).role(Role::Overlay)
                }
                if let Some(m) = self.menu.get() {
                    let drawing = {
                        let mut canvas = PixelCanvas::at((m.layout.bounds.x, m.layout.bounds.y), m.scale);
                        let entries = m.entries.iter().map(|(_, text, enabled)| (text.as_str(), *enabled)).collect::<Vec<_>>();
                        m.layout.paint(&mut canvas, &self.text.font, &entries, m.hover, None);
                        DrawList::from(canvas.commands)
                    };
                    place(ui! {
                        ZStack() {
                            Canvas(drawing)
                            for (i, r) in m.layout.rows.iter().enumerate() {
                                let b = m.layout.bounds;
                                place(empty(), Bounds::new((r.x - b.x) as f32 / m.scale, (r.y - b.y) as f32 / m.scale,
                                    r.width as f32 / m.scale, r.height as f32 / m.scale))
                                    .id(Key::new("listing.menu.row", i as u64)).interactive()
                            }
                        }.alignment(Alignment::Start)
                    }, logical(m.layout.bounds, m.scale)).id(MENU).role(Role::Overlay)
                }
            }.alignment(Alignment::Start)
        }
    }
}
