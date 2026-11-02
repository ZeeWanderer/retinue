pub(super) mod groups_view;
use super::*;
fn node(id: impl std::hash::Hash) -> Key {
    key("build.groups", id)
}
const OPTIONS: [(&str, &str); 3] = [
    ("preserve_source_slots", "Keep exact slots"),
    ("allow_omni", "Allow Omni Forma"),
    ("allow_umbral_forma", "Allow Umbral Forma"),
];
fn empty_top(m: &Metrics) -> f32 {
    60.0 + (m.height - 60.0 - 132.0).max(0.0) / 2.0
}

pub(super) fn bind(
    ui: &mut ControlHost,
    b: &Builds,
    d: &build_planner_model::Groups,
    s: &PageState,
) {
    if b.tab != "groups" {
        return;
    }
    ui.view_control(node("equipment"), |ui, r| {
        ui.button("build:tab:equipment", r, "Choose equipment", true, true)
    });
    ui.view_control(node("discover"), |ui, r| {
        ui.button("build:tab:discover", r, "Discover builds", false, true)
    });
    ui.view_control(node("group"), |ui, r| {
        ui.choice("build:group", r, &d.id, &choices(&d.choices))
    });
    ui.view_control(node("rename"), |ui, r| {
        ui.button("build:rename", r, "Rename", false, !d.id.is_empty())
    });
    ui.view_control(node("delete"), |ui, r| {
        ui.destructive("build:delete", r, "Delete", !d.id.is_empty())
    });
    ui.view_control(node("calculate"), |ui, r| {
        ui.button(
            "build:calculate",
            r,
            if s.planning {
                "Working..."
            } else {
                "Calculate"
            },
            true,
            !d.id.is_empty() && !s.busy,
        )
    });
    ui.view_control(node("instance"), |ui, r| {
        ui.choice("build:group-instance", r, &d.instance, &choices(&d.copies))
    });
    for (i, (id, label)) in OPTIONS.into_iter().enumerate() {
        ui.view_control(node(id), |ui, r| {
            ui.toggle(format!("build:option:{id}"), r, label, d.options[i])
        });
    }
    ui.view_control(node("member"), |ui, r| {
        ui.choice("build:member", r, &d.member, &choices(&d.members))
    });
    ui.view_control(node("remove"), |ui, r| {
        ui.button("build:remove", r, "Remove", false, !d.members.is_empty())
    });
    ui.view_control(node("planned"), |ui, r| {
        ui.button(
            "build:planned",
            r,
            if b.planned { "Planned" } else { "Original" },
            b.planned,
            d.plan.ready,
        )
    });
    ui.view_control(node("add"), |ui, r| {
        ui.button("build:add-config", r, "Add configuration", false, true)
    });
    ui.view_control(node("find"), |ui, r| {
        ui.button("build:group-find", r, "Find builds", false, true)
    });
}
