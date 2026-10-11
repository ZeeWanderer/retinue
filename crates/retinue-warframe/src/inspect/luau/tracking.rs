use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    ScriptInfo,
    analysis::*,
    corpus::{self, Snapshot},
    evidence::{self, Evidence, PropertyState},
    info,
    profile::Profile,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u8,
    pub features: BTreeMap<String, Feature>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub properties: Vec<String>,
    #[serde(default)]
    pub native: Vec<String>,
    #[serde(default)]
    pub data: Vec<String>,
}

impl Manifest {
    pub fn builtin() -> Self {
        let value: Self = serde_json::from_str(include_str!("features.json"))
            .expect("checked-in feature manifest");
        value.order().expect("valid feature dependencies");
        value
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let value: Self = corpus::read_json(path)?;
        value.order()?;
        Ok(value)
    }

    fn order(&self) -> Result<Vec<String>, String> {
        if self.schema != 1 {
            return Err("unsupported feature manifest schema".into());
        }
        let mut pending: BTreeSet<_> = self.features.keys().cloned().collect();
        let mut done = BTreeSet::new();
        let mut order = Vec::new();
        for (id, feature) in &self.features {
            for dependency in &feature.depends_on {
                if !self.features.contains_key(dependency) {
                    return Err(format!("{id}: unknown feature dependency {dependency}"));
                }
            }
        }
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .find(|id| {
                    self.features[*id]
                        .depends_on
                        .iter()
                        .all(|d| done.contains(d))
                })
                .cloned();
            let Some(id) = ready else {
                return Err(format!("feature dependency cycle: {pending:?}"));
            };
            pending.remove(&id);
            done.insert(id.clone());
            order.push(id);
        }
        Ok(order)
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u8,
    pub baseline: String,
    pub target: String,
    pub baseline_profile_sha256: String,
    pub target_profile_sha256: String,
    pub candidate_profiles: bool,
    pub evidence_errors: BTreeMap<String, String>,
    pub capture_errors: BTreeMap<String, BTreeMap<String, String>>,
    pub summary: BTreeMap<String, usize>,
    pub scripts: BTreeMap<String, Change>,
    pub features: BTreeMap<String, Impact>,
}

#[derive(Debug, Serialize)]
pub struct Change {
    pub status: String,
    pub error: Option<String>,
    pub old_prototypes: Option<usize>,
    pub new_prototypes: Option<usize>,
    pub matched_prototypes: usize,
    pub bodies_matching_except_atoms: usize,
    pub prototype_matches: Vec<PrototypeMatch>,
    pub possible_dependencies: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
pub struct PrototypeMatch {
    pub before: usize,
    pub after: usize,
    pub atoms_changed: bool,
}

impl Change {
    fn new(status: &str) -> Self {
        Self {
            status: status.into(),
            error: None,
            old_prototypes: None,
            new_prototypes: None,
            matched_prototypes: 0,
            bodies_matching_except_atoms: 0,
            prototype_matches: Vec::new(),
            possible_dependencies: BTreeSet::new(),
        }
    }

    fn review(&self) -> bool {
        !matches!(self.status.as_str(), "unchanged" | "opcode_encoding_only")
    }
}

#[derive(Debug, Serialize)]
pub struct Impact {
    pub changed: bool,
    pub review_required: bool,
    pub scripts: BTreeMap<String, String>,
    pub native: BTreeMap<String, String>,
    pub properties: BTreeMap<String, PropertyChange>,
    pub dependency_review: Vec<String>,
    pub data_not_tracked: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PropertyChange {
    pub changed: bool,
    pub status: String,
    pub before: PropertyState,
    pub after: PropertyState,
}

pub fn compare(
    workspace: &Path,
    before: &Snapshot,
    after: &Snapshot,
    manifest: &Manifest,
) -> Result<Report, String> {
    let order = manifest.order()?;
    let mut report = Report {
        schema: 1,
        baseline: before.name.clone(),
        target: after.name.clone(),
        baseline_profile_sha256: fingerprint(&json!(before.profile)),
        target_profile_sha256: fingerprint(&json!(after.profile)),
        candidate_profiles: before.profile.status != "reviewed"
            || after.profile.status != "reviewed",
        evidence_errors: BTreeMap::new(),
        capture_errors: BTreeMap::new(),
        summary: BTreeMap::new(),
        scripts: BTreeMap::new(),
        features: BTreeMap::new(),
    };
    for snapshot in [before, after] {
        if !snapshot.errors.is_empty() {
            report
                .capture_errors
                .insert(snapshot.name.clone(), snapshot.errors.clone());
        }
    }
    let paths: BTreeSet<_> = before.scripts.keys().chain(after.scripts.keys()).collect();
    for path in paths {
        let result = compare_script(workspace, before, after, path);
        let change = result.unwrap_or_else(|error| {
            let mut c = Change::new("unsupported_or_corrupt");
            c.error = Some(error);
            c
        });
        *report.summary.entry(change.status.clone()).or_default() += 1;
        report.scripts.insert(path.clone(), change);
    }
    let mut load_evidence = |snapshot: &Snapshot| match evidence::load(workspace, snapshot) {
        Ok(value) => value,
        Err(error) => {
            report.evidence_errors.insert(snapshot.name.clone(), error);
            None
        }
    };
    let old_evidence = load_evidence(before);
    let new_evidence = load_evidence(after);
    let executable_changed = before.profile.executable_sha256 != after.profile.executable_sha256;
    for name in order {
        let feature = &manifest.features[&name];
        let mut visited = BTreeSet::new();
        let mut pending = feature.scripts.clone();
        let mut properties = BTreeMap::new();
        let roots: BTreeSet<String> = feature
            .properties
            .iter()
            .flat_map(|root| {
                if root.ends_with('/') {
                    let matches: BTreeSet<_> = old_evidence
                        .iter()
                        .chain(new_evidence.iter())
                        .flat_map(|e| e.properties.keys())
                        .filter(|p| p.starts_with(root))
                        .cloned()
                        .collect();
                    if matches.is_empty() {
                        BTreeSet::from([root.clone()])
                    } else {
                        matches
                    }
                } else {
                    BTreeSet::from([root.clone()])
                }
            })
            .collect();
        for root in &roots {
            let old = evidence::property(old_evidence.as_ref(), root);
            let new = evidence::property(new_evidence.as_ref(), root);
            pending.extend(
                old.script_candidates
                    .iter()
                    .chain(&new.script_candidates)
                    .cloned(),
            );
            let decoder_changed = match (&old_evidence, &new_evidence) {
                (Some(a), Some(b)) => a.decoders != b.decoders,
                _ => false,
            };
            let status = if old.sha256 != new.sha256 {
                "record_or_parent_changed_review"
            } else if !old.gaps.is_empty() || !new.gaps.is_empty() {
                "incomplete"
            } else if decoder_changed {
                "decoder_changed_review"
            } else {
                "unchanged"
            };
            properties.insert(
                root.clone(),
                PropertyChange {
                    changed: old.sha256 != new.sha256,
                    status: status.into(),
                    before: old,
                    after: new,
                },
            );
        }
        let mut scripts = BTreeMap::new();
        while let Some(path) = pending.pop() {
            if !visited.insert(path.clone()) {
                continue;
            }
            if let Some(change) = report.scripts.get(&path) {
                pending.extend(change.possible_dependencies.iter().cloned());
                if change.review() {
                    scripts.insert(path, change.status.clone());
                }
            } else {
                scripts.insert(path, "not_captured".into());
            }
        }
        let native: BTreeMap<_, _> = feature
            .native
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    native_status(
                        old_evidence.as_ref(),
                        new_evidence.as_ref(),
                        id,
                        executable_changed,
                    )
                    .into(),
                )
            })
            .collect();
        let dependency_review: Vec<_> = feature
            .depends_on
            .iter()
            .filter(|id| report.features[*id].review_required)
            .cloned()
            .collect();
        let changed = properties.values().any(|p| p.changed)
            || (!feature.native.is_empty() && executable_changed)
            || native
                .values()
                .any(|status| status == "instructions_or_binding_changed_review")
            || feature
                .depends_on
                .iter()
                .any(|id| report.features[id].changed)
            || visited.iter().any(|path| {
                let raw_changed = before.scripts.get(path).map(|s| &s.sha256)
                    != after.scripts.get(path).map(|s| &s.sha256);
                raw_changed
                    && report
                        .scripts
                        .get(path)
                        .is_none_or(|c| c.status != "opcode_encoding_only")
            });
        let review_required = !scripts.is_empty()
            || native.values().any(|status: &String| status != "unchanged")
            || properties.values().any(|p| p.status != "unchanged")
            || !dependency_review.is_empty()
            || !feature.data.is_empty()
            || report.candidate_profiles
            || !report.capture_errors.is_empty()
            || !report.evidence_errors.is_empty();
        report.features.insert(
            name.clone(),
            Impact {
                changed,
                review_required,
                scripts,
                native,
                properties,
                dependency_review,
                data_not_tracked: feature.data.clone(),
            },
        );
    }
    Ok(report)
}

fn native_status(
    before: Option<&Evidence>,
    after: Option<&Evidence>,
    id: &str,
    executable_changed: bool,
) -> &'static str {
    match (
        before.and_then(|e| e.native.get(id)),
        after.and_then(|e| e.native.get(id)),
    ) {
        (Some(a), Some(b)) if a.sha256 != b.sha256 => "instructions_or_binding_changed_review",
        (Some(_), Some(_)) if executable_changed => "body_unchanged_external_dependencies_review",
        (Some(_), Some(_)) => "unchanged",
        _ => "not_captured",
    }
}

fn compare_script(
    workspace: &Path,
    before: &Snapshot,
    after: &Snapshot,
    path: &str,
) -> Result<Change, String> {
    let old = before
        .scripts
        .get(path)
        .map(|_| {
            corpus::read_script(workspace, before, path)
                .and_then(|bytes| Ok((info(&bytes, &before.profile)?, bytes)))
        })
        .transpose()?;
    let new = after
        .scripts
        .get(path)
        .map(|_| {
            corpus::read_script(workspace, after, path)
                .and_then(|bytes| Ok((info(&bytes, &after.profile)?, bytes)))
        })
        .transpose()?;
    let mut change = match (&old, &new) {
        (Some((old_info, a)), Some((new_info, b))) => {
            compare_parsed(a, old_info, &before.profile, b, new_info, &after.profile)
                .unwrap_or_else(|error| {
                    let mut c = Change::new("unsupported_or_corrupt");
                    c.error = Some(error);
                    c
                })
        }
        (Some(_), None) => Change::new("not_in_target_snapshot"),
        (None, Some(_)) => Change::new("not_in_baseline_snapshot"),
        _ => unreachable!(),
    };
    for (script, _) in [&old, &new].into_iter().flatten() {
        change.possible_dependencies.extend(dependencies(script));
    }
    for (input, profile) in [(&old, &before.profile), (&new, &after.profile)] {
        if let Some((script, _)) = input {
            for proto in &script.prototypes {
                if let Err(error) = instructions(proto, profile) {
                    change.status = "unsupported_or_corrupt".into();
                    change.error = Some(error);
                    break;
                }
            }
        }
    }
    Ok(change)
}

fn compare_parsed(
    a: &[u8],
    old: &ScriptInfo,
    old_profile: &Profile,
    b: &[u8],
    new: &ScriptInfo,
    new_profile: &Profile,
) -> Result<Change, String> {
    let old_bodies = bodies(old, old_profile, false)?;
    let new_bodies = bodies(new, new_profile, false)?;
    let old_anonymous = bodies(old, old_profile, true)?;
    let new_anonymous = bodies(new, new_profile, true)?;
    let status = if old.main_prototype == new.main_prototype && old_bodies == new_bodies {
        if normalized_bytes(a, old, old_profile)? == normalized_bytes(b, new, new_profile)? {
            if a == b {
                "unchanged"
            } else {
                "opcode_encoding_only"
            }
        } else {
            "metadata_or_layout_changed_review"
        }
    } else if old.main_prototype == new.main_prototype && old_anonymous == new_anonymous {
        "atom_rebinding_review"
    } else {
        "code_or_constants_changed_review"
    };
    let mut change = Change::new(status);
    change.old_prototypes = Some(old.prototypes.len());
    change.new_prototypes = Some(new.prototypes.len());
    change.matched_prototypes = common_bodies(&old_bodies, &new_bodies);
    change.bodies_matching_except_atoms = common_bodies(&old_anonymous, &new_anonymous);
    for (old_index, body) in old_anonymous.iter().enumerate() {
        if old_anonymous.iter().filter(|other| *other == body).count() != 1 {
            continue;
        }
        let mut matches = new_anonymous
            .iter()
            .enumerate()
            .filter(|(_, other)| *other == body);
        if let Some((new_index, _)) = matches.next()
            && matches.next().is_none()
        {
            change.prototype_matches.push(PrototypeMatch {
                before: old_index,
                after: new_index,
                atoms_changed: old_bodies[old_index] != new_bodies[new_index],
            });
        }
    }
    Ok(change)
}

fn bodies(script: &ScriptInfo, profile: &Profile, hide_atoms: bool) -> Result<Vec<String>, String> {
    script
        .prototypes
        .iter()
        .map(|proto| projection(proto, script, profile, hide_atoms).map(|v| fingerprint(&v)))
        .collect()
}

fn common_bodies(old: &[String], new: &[String]) -> usize {
    let mut counts = BTreeMap::<&String, usize>::new();
    for body in old {
        *counts.entry(body).or_default() += 1;
    }
    new.iter()
        .filter(|body| match counts.get_mut(body) {
            Some(count) if *count > 0 => {
                *count -= 1;
                true
            }
            _ => false,
        })
        .count()
}

fn normalized_bytes(
    bytes: &[u8],
    script: &ScriptInfo,
    profile: &Profile,
) -> Result<Vec<u8>, String> {
    let mut bytes = bytes.to_vec();
    for proto in &script.prototypes {
        for (pc, op) in instructions(proto, profile)? {
            let word = (proto.words[pc] & operand_mask(op)) | u32::from(op.canonical);
            let start = proto.code_offset + pc * 4;
            bytes
                .get_mut(start..start + 4)
                .ok_or("invalid code offset")?
                .copy_from_slice(&word.to_le_bytes());
        }
    }
    Ok(bytes)
}

pub fn feature_manifest() -> Value {
    json!(Manifest::builtin())
}

#[derive(Default, Serialize)]
struct Occurrences {
    count: usize,
    examples: Vec<String>,
    name: Option<String>,
}

impl Occurrences {
    fn add(&mut self, count: usize, path: &str) {
        self.count += count;
        if self.examples.len() < 8 && !self.examples.iter().any(|p| p == path) {
            self.examples.push(path.into());
        }
    }
}

pub fn coverage(workspace: &Path, snapshot: &Snapshot) -> Value {
    let mut opcodes = BTreeMap::<String, Occurrences>::new();
    let mut atoms = BTreeMap::<String, Occurrences>::new();
    let mut incomplete = BTreeMap::new();
    for path in snapshot.scripts.keys() {
        let result = corpus::read_script(workspace, snapshot, path)
            .and_then(|bytes| info(&bytes, &snapshot.profile));
        match result {
            Ok(script) => {
                for op in &script.opcode_coverage {
                    let entry = opcodes.entry(format!("0x{:02x}", op.raw)).or_default();
                    entry.add(op.instructions, path);
                    entry.name = Some(op.name.clone());
                }
                for value in &script.atom_constants {
                    let entry = atoms.entry(value.hex.clone()).or_default();
                    entry.add(value.occurrences, path);
                    entry.name = snapshot.profile.symbols.get(&value.value).cloned();
                }
                let errors: Vec<_> = script
                    .prototypes
                    .iter()
                    .filter_map(|p| instructions(p, &snapshot.profile).err())
                    .collect();
                if !errors.is_empty() {
                    incomplete.insert(path.clone(), errors);
                }
            }
            Err(error) => {
                incomplete.insert(path.clone(), vec![error]);
            }
        }
    }
    json!({"schema": 1, "snapshot": snapshot.name, "executable_sha256": snapshot.profile.executable_sha256,
        "profile_sha256": fingerprint(&json!(snapshot.profile)), "scripts": snapshot.scripts.len(),
        "fully_decoded": snapshot.scripts.len() - incomplete.len(), "incomplete": incomplete,
        "capture_errors": snapshot.errors, "opcodes": opcodes, "atoms": atoms,
        "counting": "Known instruction boundaries only; stops at the first unknown opcode in each prototype. Atoms count constant-table entries."})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::luau::profile::Opcode;

    #[test]
    fn remapped_code_and_changed_literals_are_separate() {
        let mut p = Profile::unmapped("a".repeat(64));
        p.mapping.push(Opcode::new(100, 22).unwrap());
        let mut q = p.clone();
        q.mapping[0].raw = 200;
        let a = fixture(vec![100], vec![]);
        let b = fixture(vec![200], vec![]);
        let raw_a = 100u32.to_le_bytes();
        let raw_b = 200u32.to_le_bytes();
        assert_eq!(
            compare_parsed(&raw_a, &a, &p, &raw_b, &b, &q)
                .unwrap()
                .status,
            "opcode_encoding_only"
        );
        let mut c = b.clone();
        c.prototypes[0].words[0] |= 1 << 16;
        assert_eq!(
            compare_parsed(&raw_a, &a, &p, &raw_b, &c, &q)
                .unwrap()
                .status,
            "code_or_constants_changed_review"
        );
        q.mapping.clear();
        assert!(compare_parsed(&raw_a, &a, &p, &raw_b, &b, &q).is_err());
    }

    #[test]
    fn names_never_hide_changed_native_binding() {
        let mut p = Profile::unmapped("a".repeat(64));
        p.mapping.push(Opcode::new(100, 22).unwrap());
        let a = fixture(vec![100], vec![vec![1, 2, 0, 0, 0]]);
        let b = fixture(vec![100], vec![vec![1, 3, 0, 0, 0]]);
        p.symbols.insert(2, "sameLabel".into());
        p.symbols.insert(3, "sameLabel".into());
        assert_eq!(
            compare_parsed(&[], &a, &p, &[], &b, &p).unwrap().status,
            "atom_rebinding_review"
        );
    }

    #[test]
    fn feature_manifest_keeps_native_and_data_dependencies_explicit() {
        let manifest = Manifest::builtin();
        assert_eq!(manifest.schema, 1);
        let archimedea = &manifest.features["archimedea.weekly-loadout"];
        assert!(archimedea.native.iter().any(|name| name.contains("PCG")));
        assert!(!archimedea.data.is_empty());
        let calculator = &manifest.features["calculator.direct-fire"];
        assert!(calculator.depends_on.contains(&"calculator.mesa".into()));
        assert!(
            manifest.features["calculator.mesa"]
                .scripts
                .iter()
                .any(|name| name.ends_with("CowgirlPassive.lua"))
        );
        assert!(!manifest.features["calculator.upgrades"].native.is_empty());
        assert!(!manifest.features["calculator.upgrades"].data.is_empty());
        assert!(
            manifest.features["calculator.primary-crux"]
                .scripts
                .iter()
                .any(|name| name.ends_with("StatusAndAmmoEfficiencyOnWeakpointHit.lua"))
        );
    }

    #[test]
    fn dependency_graph_rejects_unknown_ids_and_cycles() {
        assert!(
            serde_json::from_value::<Manifest>(json!({"schema":1,
            "features":{"a":{"depend_on":["b"]}}}))
            .is_err()
        );
        let parse = |value| serde_json::from_value::<Manifest>(value).unwrap();
        assert!(
            parse(json!({"schema":1,"features":{"a":{"depends_on":["b"]}}}))
                .order()
                .is_err()
        );
        assert!(
            parse(json!({"schema":1,"features":{"a":{"depends_on":["b"]},
            "b":{"depends_on":["a"]}}}))
            .order()
            .is_err()
        );
        let m = parse(json!({"schema":1,"features":{"a":{"depends_on":["b"]},"b":{}}}));
        assert_eq!(m.order().unwrap(), vec!["b", "a"]);
    }

    #[test]
    fn contact_stage_tracks_eligibility_without_claiming_public_integration() {
        let manifest = Manifest::builtin();
        let contact = &manifest.features["calculator.contact-packet"];
        for id in [
            "damage.contact-controller",
            "damage.hit-proxy-selection",
            "damage.hit-proxy-modifiers",
            "damage.ignore-headshot",
            "damage.deferred-scalar",
            "damage.critical-factor",
            "damage.weakpoint-predicate",
            "damage.weapon-upgrades",
            "upgrade.manager",
            "upgrade.bucket-filter",
            "upgrade.damage-type-filter",
            "upgrade.deferred-mode",
            "upgrade.type-identities",
            "damage.critical-context",
            "damage.critical-source",
            "upgrade.weapon-scalar",
            "upgrade.weapon-presence",
            "upgrade.critical-mode",
            "damage.forced-proc-counter",
            "damage.packet-origin",
            "upgrade.weapon-integer",
            "damage.critical-source-handle",
            "damage.critical-source-policy",
            "damage.player-reference-valid",
            "reference.nonnull",
        ] {
            assert!(contact.native.iter().any(|name| name == id));
        }
        assert!(!contact.data.is_empty());
        for resource in [
            "/Lotus/Upgrades/Mods/Sets/Vigilante/VigilanteSetMod",
            "/Lotus/Upgrades/Mods/Sets/Hunter/PrimaryHunterMunitionsMod",
            "/Lotus/Upgrades/Mods/Sets/Hunter/CompanionHunterSynergyMod",
            "/Lotus/Upgrades/Mods/Sentinel/Kubrow/BeastWeapon/BeastCullTheWeakMod",
        ] {
            assert!(contact.properties.iter().any(|name| name == resource));
        }
        assert!(contact.depends_on.contains(&"calculator.upgrades".into()));
        assert!(
            !manifest.features["calculator.direct-fire.core"]
                .depends_on
                .contains(&"calculator.contact-packet".into())
        );
    }

    #[test]
    fn companion_critical_research_tracks_scripts_without_enabling_effects() {
        let manifest = Manifest::builtin();
        for (feature, script) in [
            ("calculator.cats-eye", "CatbrowCatsEyeAbility.lua"),
            ("calculator.tenacious-bond", "TenaciousPartner.lua"),
        ] {
            let effect = &manifest.features[feature];
            assert!(effect.scripts.iter().any(|path| path.ends_with(script)));
            assert!(!effect.properties.is_empty());
            assert!(!effect.data.is_empty());
            assert!(effect.depends_on.contains(&"calculator.upgrades".into()));
            for public in ["calculator.direct-fire", "calculator.direct-fire.core"] {
                assert!(
                    !manifest.features[public]
                        .depends_on
                        .iter()
                        .any(|id| id == feature)
                );
            }
        }
    }

    #[test]
    fn native_body_equality_is_not_external_dependency_equality() {
        let mut a = Evidence::default();
        a.native.insert(
            "scalar".into(),
            evidence::Native {
                entry: "140001000".into(),
                sha256: "a".repeat(64),
                instructions: 2,
            },
        );
        let mut b = Evidence::default();
        b.native.insert(
            "scalar".into(),
            evidence::Native {
                entry: "140001000".into(),
                sha256: "a".repeat(64),
                instructions: 2,
            },
        );
        assert_eq!(
            native_status(Some(&a), Some(&b), "scalar", false),
            "unchanged"
        );
        assert_eq!(
            native_status(Some(&a), Some(&b), "scalar", true),
            "body_unchanged_external_dependencies_review"
        );
        b.native.get_mut("scalar").unwrap().sha256 = "b".repeat(64);
        assert_eq!(
            native_status(Some(&a), Some(&b), "scalar", false),
            "instructions_or_binding_changed_review"
        );
        assert_eq!(native_status(None, None, "scalar", false), "not_captured");
    }
}
