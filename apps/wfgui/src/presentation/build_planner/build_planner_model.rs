use super::*;

#[derive(Clone, Default, PartialEq)]
pub(super) struct Topology {
    pub topology: Value,
    pub upgrades: Value,
    pub polarities: Value,
    pub shards: Value,
    pub overrides: Value,
}

#[derive(Clone, PartialEq)]
pub(super) struct EquipmentRow {
    pub id: String,
    pub name: String,
    pub class: String,
    pub asset: Option<Asset>,
}

#[derive(Clone, PartialEq)]
pub(super) struct SourceRow {
    pub id: String,
    pub action: String,
    pub title: String,
    pub subtitle: String,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Model {
    pub equipment: Equipment,
    pub discover: Discover,
    pub groups: Groups,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Equipment {
    pub loading: bool,
    pub selected: bool,
    pub owned: bool,
    pub name: String,
    pub definitions: Vec<EquipmentRow>,
    pub copies: Vec<(String, String)>,
    pub configs: Vec<(usize, String)>,
    pub topology: Topology,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Discover {
    pub detail: bool,
    pub sources: bool,
    pub title: String,
    pub summary: String,
    pub notes: String,
    pub topology: Topology,
    pub rows: Vec<SourceRow>,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Groups {
    pub loading: bool,
    pub selected: bool,
    pub id: String,
    pub choices: Vec<(String, String)>,
    pub copies: Vec<(String, String)>,
    pub instance: String,
    pub options: [bool; 3],
    pub members: Vec<(String, String)>,
    pub member: String,
    pub notes: String,
    pub topology: Topology,
    pub plan: Plan,
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Plan {
    pub ready: bool,
    pub blocked: Option<String>,
    pub topology: Value,
    pub polarities: Value,
    pub operations: Vec<String>,
    pub resources: String,
    pub capacity: Option<(String, bool)>,
}

impl Model {
    pub fn new(b: &Builds) -> Self {
        let definition = b.definition();
        let instance = b.instance();
        let config = b.config();
        let owned = b
            .instances()
            .iter()
            .map(|i| i.definition_id.as_str())
            .collect::<HashSet<_>>();
        let equipment = Equipment {
            loading: b.equipment.is_none(),
            selected: !definition.is_null(),
            owned: !instance.is_null(),
            name: text(&definition, "name").into(),
            definitions: b
                .definitions()
                .iter()
                .filter(|d| owned.contains(d.id.as_str()))
                .map(|d| EquipmentRow {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    class: d.class.clone(),
                    asset: d.asset.clone(),
                })
                .collect(),
            copies: b
                .instances()
                .iter()
                .filter(|i| i.definition_id == b.definition)
                .map(|i| {
                    let name = i
                        .custom_name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(text(&definition, "name"));
                    (
                        i.instance_id.clone(),
                        format!("{} ({})", name, suffix(&i.instance_id)),
                    )
                })
                .collect(),
            configs: entries(&instance["configs"])
                .iter()
                .map(|c| {
                    let index = number(c, "config_index") as usize;
                    let name = text(c, "name");
                    (
                        index,
                        if name.is_empty() {
                            format!(
                                "Config {}",
                                char::from_u32(65 + index as u32).unwrap_or('?')
                            )
                        } else {
                            name.into()
                        },
                    )
                })
                .collect(),
            topology: Topology {
                topology: instance["topology"].clone(),
                upgrades: config["upgrade_slots"].clone(),
                polarities: instance["effective_polarities"].clone(),
                shards: instance["shard_slots"].clone(),
                overrides: config["ability_override"].clone(),
            },
        };
        let d = &b.detail;
        let metadata = &d["metadata"];
        let discover = Discover {
            detail: !d.is_null(),
            sources: !b.sources.is_null(),
            title: text(metadata, "title").into(),
            summary: format!(
                "{}  -  {:.0} Forma",
                text(&metadata["author"], "username"),
                number(metadata, "formas")
            ),
            notes: text(metadata, "description").into(),
            topology: Topology {
                topology: d["presentation"]["topology"].clone(),
                upgrades: d["presentation"]["upgrades"].clone(),
                overrides: d["content"]["ability_override"].clone(),
                ..Default::default()
            },
            rows: entries(if b.sources.is_null() {
                &b.items["items"]
            } else {
                &b.sources["builds"]
            })
            .iter()
            .map(|row| {
                let build = row["identity"].is_object();
                let id = if build {
                    row["identity"]["external_id"].to_string()
                } else {
                    text(row, "canonical_id").into()
                };
                SourceRow {
                    action: format!(
                        "build:{}:{id}",
                        if build { "detail" } else { "source-item" }
                    ),
                    id,
                    title: text(row, if build { "title" } else { "name" }).into(),
                    subtitle: if build {
                        format!(
                            "{}  -  {:.0} Forma  -  Score {:.0}",
                            text(&row["author"], "username"),
                            number(row, "formas"),
                            number(row, "score")
                        )
                    } else {
                        text(row, "class").into()
                    },
                }
            })
            .collect(),
        };
        let group = &b.group;
        let members = entries(&group["members"]);
        let member = members
            .iter()
            .find(|m| text(m, "id") == b.member)
            .or_else(|| members.first())
            .unwrap_or(&Value::Null);
        let baseline = &group["baseline"];
        let result = &group["plan_result"];
        let planned = entries(&result["builds"])
            .iter()
            .find(|p| text(p, "member_id") == text(member, "id"));
        let snapshot = &member["snapshot"];
        let (topology, upgrades, polarities, overrides) = if b.planned && planned.is_some() {
            let p = planned.unwrap();
            (
                &baseline["topology"],
                &p["upgrade_slots"],
                &result["final_polarities"],
                &p["ability_override"],
            )
        } else if text(member, "kind") == "player_config" {
            (
                &baseline["topology"],
                &snapshot["config"]["upgrade_slots"],
                &baseline["effective_polarities"],
                &snapshot["config"]["ability_override"],
            )
        } else {
            (
                &snapshot["presentation"]["topology"],
                &snapshot["presentation"]["upgrades"],
                &baseline["effective_polarities"],
                &snapshot["content"]["ability_override"],
            )
        };
        let mut copies = b
            .instances()
            .iter()
            .filter(|i| i.definition_id == text(group, "definition_id"))
            .map(|i| {
                let name = i
                    .custom_name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Owned copy");
                (
                    i.instance_id.clone(),
                    format!("{} ({})", name, suffix(&i.instance_id)),
                )
            })
            .collect::<Vec<_>>();
        if text(group, "instance_id").is_empty() {
            copies.insert(0, (String::new(), "Choose an owned copy".into()));
        }
        let groups = Groups {
            loading: b.groups.is_null(),
            selected: !group.is_null(),
            id: text(group, "id").into(),
            choices: entries(&b.groups["groups"])
                .iter()
                .map(|g| (text(g, "id").into(), text(g, "name").into()))
                .collect(),
            copies,
            instance: text(group, "instance_id").into(),
            options: ["preserve_source_slots", "allow_omni", "allow_umbral_forma"]
                .map(|key| group["options"][key] == true),
            members: members
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    (
                        text(m, "id").into(),
                        format!("{}. {}", i + 1, text(m, "name")),
                    )
                })
                .collect(),
            member: text(member, "id").into(),
            notes: text(&snapshot["metadata"], "description").into(),
            topology: Topology {
                topology: topology.clone(),
                upgrades: upgrades.clone(),
                polarities: polarities.clone(),
                shards: baseline["shard_slots"].clone(),
                overrides: overrides.clone(),
            },
            plan: Plan {
                ready: text(result, "status") == "ready",
                blocked: (text(result, "status") == "blocked")
                    .then(|| format!("Plan blocked: {}", text(result, "reason").replace('_', " "))),
                topology: baseline["topology"].clone(),
                polarities: result["final_polarities"].clone(),
                operations: entries(&result["operations"])
                    .iter()
                    .enumerate()
                    .map(|(i, op)| {
                        if text(op, "action") == "swap" {
                            format!(
                                "{}. Swap {} with {}",
                                i + 1,
                                text(op, "label"),
                                text(op, "other_label")
                            )
                        } else {
                            format!(
                                "{}. Apply {} Forma to {}: {} -> {}",
                                i + 1,
                                text(op, "forma"),
                                text(op, "label"),
                                text(op, "before"),
                                text(op, "polarity")
                            )
                        }
                    })
                    .collect(),
                resources: result["forma_requirements"]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| {
                                v.as_u64()
                                    .filter(|n| *n > 0)
                                    .map(|n| format!("{n} {}", k.replace('_', " ")))
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
                capacity: planned.map(|p| {
                    (
                        format!(
                            "Planned capacity: {:.0} / {:.0} used, {:.0} remaining",
                            number(p, "drain"),
                            number(p, "capacity"),
                            number(p, "remaining_capacity")
                        ),
                        number(p, "remaining_capacity") >= 0.0,
                    )
                }),
            },
        };
        Self {
            equipment,
            discover,
            groups,
        }
    }
}

fn suffix(id: &str) -> &str {
    let offset = id.char_indices().rev().nth(5).map_or(0, |(i, _)| i);
    &id[offset..]
}
