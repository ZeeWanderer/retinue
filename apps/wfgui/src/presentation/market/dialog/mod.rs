use super::*;
mod dialog_view;

const PANEL: Key = Key::new("market.dialog", 0);
const SUBTYPE: Key = Key::new("market.dialog.subtype", 0);
const VISIBILITY: Key = Key::new("market.dialog.visible", 0);
const CANCEL: Key = Key::new("market.dialog.cancel", 0);
const CONFIRM: Key = Key::new("market.dialog.confirm", 0);

fn editor_key(id: Key) -> Key {
    Key::new("market.dialog.editor", id.value)
}

#[derive(Clone, PartialEq)]
struct Content {
    edit: bool,
    title: String,
    subtitle: String,
    message: String,
    height: f32,
    rows: usize,
    subtypes: Vec<String>,
    subtype: String,
    visible: bool,
    error: Option<String>,
}

impl Content {
    fn bounds(&self, m: &Metrics) -> Bounds {
        Bounds::new(
            (m.width - 330.0) / 2.0,
            ((m.height - self.height) / 2.0).max(0.0),
            330.0,
            self.height,
        )
    }
    fn footer_y(&self) -> f32 {
        76.0 + self.rows as f32 * 65.0
    }
}

#[derive(Clone, PartialEq)]
struct NumberField {
    index: usize,
    name: &'static str,
    label: &'static str,
    value: String,
    min: u32,
    max: u32,
}

pub(super) struct Dialog {
    content: State<Option<Content>>,
    fields: Items<NumberField>,
}

impl Dialog {
    pub fn new() -> Self {
        Self {
            content: State::default(),
            fields: Items::default(),
        }
    }

    pub fn view(&self, metrics: State<Metrics>, fonts: Fonts) -> Element {
        dialog_view::DialogView {
            content: self.content.clone(),
            fields: self.fields.clone(),
            metrics,
            fonts,
        }
        .into_element()
    }

    pub fn sync(&self, market: &Market) {
        let Some(dialog) = market.dialog.as_ref() else {
            self.content.set(None);
            self.fields.replace([]).expect("empty market dialog");
            return;
        };
        let item = market
            .catalog
            .get(item_id(&dialog["order"]))
            .unwrap_or(&Value::Null);
        let edit = dialog["edit"] == true;
        let fields = if edit {
            editor::fields(item, &dialog["order"])
        } else {
            Vec::new()
        };
        self.content.set(Some(Content {
            edit,
            title: text(dialog, "title").into(),
            subtitle: order_name(&dialog["order"], market).into(),
            message: text(dialog, "message").into(),
            height: if edit {
                editor::height(dialog, item) + 28.0
            } else {
                210.0
            },
            rows: fields.len().div_ceil(2),
            subtypes: editor::subtypes(dialog, item),
            subtype: text(&dialog["draft"], "subtype").into(),
            visible: dialog["draft"]["visible"] == true,
            error: market.error.clone(),
        }));
        self.fields
            .replace(
                fields
                    .into_iter()
                    .enumerate()
                    .map(|(index, (name, label, min, max))| {
                        (
                            key("market.dialog.field", name),
                            NumberField {
                                index,
                                name,
                                label,
                                min,
                                max,
                                value: text(&dialog["draft"], name).into(),
                            },
                        )
                    }),
            )
            .expect("unique order fields");
    }

    pub fn bind(&self, ui: &mut ControlHost, market: &Market, busy: bool) {
        if market.dialog.is_none() {
            return;
        }
        let Some(r) = ui.view_bounds(PANEL) else {
            return;
        };
        ui.begin_modal();
        ui.region("market.dialog", r);
        for &id in self.fields.order().keys() {
            let f = self.fields.get(id).unwrap();
            ui.view_control(editor_key(id), |ui, r| {
                ui.number_field(
                    &format!("market:edit:{}", f.name),
                    r,
                    &f.value,
                    f.label,
                    f.min,
                    f.max,
                )
            });
        }
        let c = self.content.get().unwrap();
        ui.view_control(SUBTYPE, |ui, r| {
            ui.choice(
                "market:edit:subtype",
                r,
                &c.subtype,
                &c.subtypes
                    .iter()
                    .map(|s| (s.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
            )
        });
        ui.view_control(VISIBILITY, |ui, r| {
            ui.toggle("market:edit:visible", r, "Public", c.visible)
        });
        ui.view_control(CANCEL, |ui, r| {
            ui.button("market:cancel", r, "Cancel", false, true)
        });
        ui.view_control(CONFIRM, |ui, r| {
            if c.edit {
                ui.button("market:confirm", r, "Save", true, !busy);
            } else {
                ui.destructive("market:confirm", r, "Confirm", !busy);
            }
        });
    }
}
