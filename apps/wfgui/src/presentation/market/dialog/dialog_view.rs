use super::*;

#[derive(PartialEq)]
pub(super) struct DialogView {
    pub content: State<Option<Content>>,
    pub fields: Items<NumberField>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

impl View for DialogView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let Some(c) = self.content.get() else {
            return empty();
        };
        let m = self.metrics.get();
        let frame = |r| m.local(c.bounds(&m), r);
        let label = |text: &str, size, color| {
            fitted_label(
                text,
                self.fonts.regular.clone(),
                size,
                color,
                false,
                m.scale,
            )
        };
        ui! {
            ZStack() {
                fill([0, 0, 0, 150], 0.0)
                place(ui! {
                    ZStack() {
                        fill(controls::POPOVER, 8.0)
                        place(fitted_label(&c.title, self.fonts.bold.clone(), 18.0, TEXT, false, m.scale), frame(Bounds::new(16.0, 12.0, 298.0, 30.0)))
                        if c.edit {
                            place(label(&c.subtitle, 14.0, TEXT), frame(Bounds::new(16.0, 44.0, 298.0, 26.0)))
                            for &id in self.fields.order().keys() {
                                let f = self.fields.get(id).unwrap();
                                let x = 16.0 + (f.index % 2) as f32 * 158.0;
                                let y = 76.0 + (f.index / 2) as f32 * 65.0;
                                place(label(f.label, 13.0, MUTED), frame(Bounds::new(x, y, 136.0, 22.0)))
                                place(slot(editor_key(id)), frame(Bounds::new(x, y + 23.0, 136.0, 34.0))).interactive()
                            }
                            if !c.subtypes.is_empty() {
                                place(slot(SUBTYPE), frame(Bounds::new(16.0, c.footer_y(), 298.0, 32.0))).interactive()
                            }
                            place(slot(VISIBILITY), frame(Bounds::new(16.0, c.footer_y() + if c.subtypes.is_empty() { 0.0 } else { 44.0 }, 298.0, 28.0))).interactive()
                        } else {
                            place(label(&c.message, 14.0, MUTED), frame(Bounds::new(16.0, 56.0, 298.0, 60.0)))
                        }
                        place(slot(CANCEL), frame(Bounds::new(16.0, c.height - 54.0, 138.0, 35.0))).interactive()
                        place(slot(CONFIRM), frame(Bounds::new(174.0, c.height - 54.0, 138.0, 35.0))).interactive()
                        if let Some(error) = &c.error {
                            place(label(error, 12.0, BAD), frame(Bounds::new(16.0, c.height - 85.0, 298.0, 28.0)))
                        }
                    }.alignment(Alignment::Start)
                }, c.bounds(&m)).id(PANEL)
            }.alignment(Alignment::Start).frame(m.width, m.height).role(Role::Overlay)
        }
    }
}
