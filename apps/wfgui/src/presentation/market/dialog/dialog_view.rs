use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct DialogView {
    pub content: State<Option<Content>>,
    pub fields: Items<NumberField>,
    pub metrics: State<Metrics>,
    pub fonts: Fonts,
}

#[view]
impl View for DialogView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
        if let Some(c) = self.content.get() {
        let m = self.metrics.get();
        let frame = |r| m.local(c.bounds(&m), r);
        let label = |text: &str, size, color| {
            super::label(
                text,
                self.fonts.regular.clone(),
                size,
                color,
                false,
            )
        };
        ui! {
            ZStack() {
                fill([0, 0, 0, 150], 0.0)
                place(ui! {
                    ZStack() {
                        fill(controls::POPOVER, 8.0)
                        place(super::label(&c.title, self.fonts.bold.clone(), 18.0, TEXT, false), frame(Bounds::new(16.0, 12.0, 298.0, 30.0)))
                        if c.edit {
                            place(label(&c.subtitle, 14.0, TEXT), frame(Bounds::new(16.0, 44.0, 298.0, 26.0)))
                            ForEach(self.fields.order().keys().iter().copied(), |id| *id) { |id|
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
                    }.alignment(AnchorPoint::TOP_LEADING)
                }, c.bounds(&m)).named(PANEL)
            }.alignment(AnchorPoint::TOP_LEADING).frame(super::frame(m.width, m.height)).role(Role::Overlay)
        }
        }
        }
    }
}
