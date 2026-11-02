mod dialog_view;
use super::*;
fn node(id: &str) -> Key {
    key("build.dialog", id)
}

#[derive(Clone, Default, PartialEq)]
struct DialogState {
    kind: String,
    title: String,
    options: Vec<(String, String)>,
}
pub(super) struct Dialog {
    state: State<DialogState>,
}
impl Dialog {
    pub fn new() -> Self {
        Self {
            state: State::default(),
        }
    }
    pub fn sync(&self, b: &Builds) {
        let kind = b.dialog.clone().unwrap_or_default();
        let definition = if kind == "source" {
            text(&b.detail["metadata"]["item"], "canonical_id")
        } else {
            &b.definition
        };
        self.state.set(DialogState {
            title: match kind.as_str() {
                "create" | "create-source" => "Create build group",
                "rename" => "Rename group",
                "delete" => "Delete this group?",
                "source" => "Add build to group",
                _ => "Add current configuration",
            }
            .into(),
            options: if matches!(kind.as_str(), "source" | "config") {
                entries(&b.groups["groups"])
                    .iter()
                    .filter(|g| text(g, "definition_id") == definition)
                    .map(|g| (text(g, "id").into(), text(g, "name").into()))
                    .collect()
            } else {
                vec![]
            },
            kind,
        });
    }
    pub fn view(&self, metrics: State<Metrics>, fonts: Fonts) -> Element {
        dialog_view::DialogView {
            metrics,
            state: self.state.clone(),
            fonts,
        }
        .into_element()
    }
    pub fn bind(&self, ui: &mut ControlHost, b: &Builds, busy: bool) {
        let s = self.state.get();
        if s.kind.is_empty() {
            return;
        }
        ui.begin_modal();
        ui.view_control(node("name"), |ui, r| {
            ui.field("build:name", r, &b.name, "Group name")
        });
        ui.view_control(node("target"), |ui, r| {
            ui.choice("build:target", r, &b.target, &choices(&s.options))
        });
        ui.view_control(node("new"), |ui, r| {
            ui.button("build:new-source", r, "New group...", false, true)
        });
        ui.view_control(node("cancel"), |ui, r| {
            ui.button("build:cancel", r, "Cancel", false, true)
        });
        let valid = match s.kind.as_str() {
            "create" | "create-source" | "rename" => !b.name.trim().is_empty(),
            "source" | "config" => !b.target.is_empty(),
            _ => true,
        };
        ui.view_control(node("confirm"), |ui, r| {
            ui.button("build:confirm", r, "Confirm", true, valid && !busy)
        });
    }
}
