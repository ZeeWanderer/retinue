use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use super::{
    ScriptInfo,
    analysis::*,
    corpus::{self, Snapshot},
    info,
    profile::{Opcode, Profile},
};

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u8,
    pub baseline: String,
    pub target: String,
    pub profile: Profile,
    pub matched_bodies: usize,
    pub ambiguous_bodies: usize,
    pub evidence: BTreeMap<u8, Evidence>,
    pub conflicts: Vec<String>,
    pub atom_candidates: BTreeMap<u32, AtomCandidate>,
    pub errors: BTreeMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Evidence {
    pub observations: usize,
    pub anchors: Vec<Anchor>,
    pub canonical_candidates: BTreeSet<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Anchor {
    pub method: &'static str,
    pub resource: String,
    pub old_prototype: usize,
    pub new_prototype: usize,
    pub pc: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct AtomCandidate {
    pub previous_ids: BTreeSet<u32>,
    pub suggested_name: Option<String>,
    pub observations: usize,
    pub anchors: Vec<AtomAnchor>,
    pub ambiguous: bool,
}

#[derive(Debug, Serialize)]
pub struct AtomAnchor {
    pub resource: String,
    pub old_prototype: usize,
    pub new_prototype: usize,
    pub constant_index: usize,
}

pub fn run(workspace: &Path, before: &Snapshot, after: &Snapshot) -> Report {
    let mut report = Report {
        schema: 1,
        baseline: before.name.clone(),
        target: after.name.clone(),
        profile: Profile::unmapped(after.profile.executable_sha256.clone()),
        matched_bodies: 0,
        ambiguous_bodies: 0,
        evidence: BTreeMap::new(),
        conflicts: Vec::new(),
        atom_candidates: BTreeMap::new(),
        errors: BTreeMap::new(),
    };
    report.profile.status = "candidate".into();
    for resource in before
        .scripts
        .keys()
        .filter(|path| after.scripts.contains_key(*path))
    {
        let pair = || {
            let old = info(
                &corpus::read_script(workspace, before, resource)?,
                &before.profile,
            )?;
            let new = info(
                &corpus::read_script(workspace, after, resource)?,
                &after.profile,
            )?;
            Ok::<_, String>((old, new))
        };
        match pair() {
            Ok((old, new)) => match_bodies(&mut report, resource, &old, &new, &before.profile),
            Err(error) => {
                report.errors.insert(resource.clone(), error);
            }
        }
    }
    finish(&mut report, &before.profile, &after.profile);
    report
}

fn match_bodies(
    report: &mut Report,
    resource: &str,
    old: &ScriptInfo,
    new: &ScriptInfo,
    profile: &Profile,
) {
    // The compiler emits PREPVARARGS A=params at every vararg entry.
    if let Some(previous) = old.prototypes.iter().find(|p| {
        p.vararg
            && p.words.first().is_some_and(|word| {
                profile
                    .mapping
                    .iter()
                    .any(|op| op.canonical == 65 && op.raw == *word as u8)
            })
            && p.words[0] >> 8 == u32::from(p.parameters)
    }) {
        for current in new.prototypes.iter().filter(|p| {
            p.vararg
                && p.words
                    .first()
                    .is_some_and(|word| word >> 8 == u32::from(p.parameters))
        }) {
            let evidence = report.evidence.entry(current.words[0] as u8).or_default();
            evidence.observations += 1;
            evidence.canonical_candidates.insert(65);
            if evidence.anchors.len() < 8 {
                evidence.anchors.push(Anchor {
                    method: "vararg_prologue",
                    resource: resource.into(),
                    old_prototype: previous.index,
                    new_prototype: current.index,
                    pc: 0,
                });
            }
        }
    }
    let mut pairs = Vec::new();
    for previous in &old.prototypes {
        if previous.words.len() < 8 || !previous.children.is_empty() {
            continue;
        }
        let Ok(ops) = instructions(previous, profile) else {
            continue;
        };
        let Ok(old_constants) = constants(previous, old, true) else {
            continue;
        };
        let candidates: Vec<_> = new
            .prototypes
            .iter()
            .filter(|current| {
                current.words.len() == previous.words.len()
                    && current.children.is_empty()
                    && header(current) == header(previous)
                    && constants(current, new, true).is_ok_and(|value| value == old_constants)
                    && ops.iter().all(|(pc, op)| {
                        (current.words[*pc] & operand_mask(op))
                            == (previous.words[*pc] & operand_mask(op))
                            && (!op.has_aux || current.words[pc + 1] == previous.words[pc + 1])
                    })
            })
            .collect();
        if candidates.len() == 1 {
            pairs.push((previous, candidates[0], ops));
        } else if candidates.len() > 1 {
            report.ambiguous_bodies += 1;
        }
    }
    let mut uses = BTreeMap::<usize, usize>::new();
    for (_, current, _) in &pairs {
        *uses.entry(current.index).or_default() += 1;
    }
    for (previous, current, ops) in pairs {
        if uses[&current.index] != 1 {
            report.ambiguous_bodies += 1;
            continue;
        }
        report.matched_bodies += 1;
        for (pc, op) in ops {
            let evidence = report.evidence.entry(current.words[pc] as u8).or_default();
            evidence.observations += 1;
            evidence.canonical_candidates.insert(op.canonical);
            if evidence.anchors.len() < 8 {
                evidence.anchors.push(Anchor {
                    method: "matching_body",
                    resource: resource.into(),
                    old_prototype: previous.index,
                    new_prototype: current.index,
                    pc,
                });
            }
        }
        for (index, (a, b)) in previous
            .constants
            .iter()
            .zip(&current.constants)
            .enumerate()
        {
            if let (Some(a), Some(b)) = (atom(a), atom(b)) {
                let candidate = report.atom_candidates.entry(b).or_default();
                candidate.previous_ids.insert(a);
                candidate.observations += 1;
                if candidate.anchors.len() < 4 {
                    candidate.anchors.push(AtomAnchor {
                        resource: resource.into(),
                        old_prototype: previous.index,
                        new_prototype: current.index,
                        constant_index: index,
                    });
                }
            }
        }
    }
}

fn finish(report: &mut Report, baseline: &Profile, target: &Profile) {
    let mut reverse = BTreeMap::<u8, BTreeSet<u8>>::new();
    for (raw, evidence) in &report.evidence {
        for canonical in &evidence.canonical_candidates {
            reverse.entry(*canonical).or_default().insert(*raw);
        }
    }
    for (raw, evidence) in &report.evidence {
        if evidence.canonical_candidates.len() != 1 {
            report
                .conflicts
                .push(format!("raw 0x{raw:02x} has multiple canonical candidates"));
            continue;
        }
        let canonical = *evidence.canonical_candidates.first().unwrap();
        if reverse[&canonical].len() != 1 {
            report
                .conflicts
                .push(format!("canonical {canonical} has multiple raw candidates"));
            continue;
        }
        if target.mapping.iter().any(|op| {
            (op.raw == *raw || op.canonical == canonical)
                && (op.raw != *raw || op.canonical != canonical)
        }) {
            report
                .conflicts
                .push(format!("raw 0x{raw:02x} conflicts with target profile"));
            continue;
        }
        report
            .profile
            .mapping
            .push(Opcode::new(*raw, canonical).expect("baseline canonical opcode"));
    }
    let mut atom_uses = BTreeMap::<u32, usize>::new();
    for candidate in report.atom_candidates.values() {
        for old in &candidate.previous_ids {
            *atom_uses.entry(*old).or_default() += 1;
        }
    }
    for candidate in report.atom_candidates.values_mut() {
        candidate.ambiguous = candidate.previous_ids.len() != 1
            || candidate.previous_ids.iter().any(|old| atom_uses[old] != 1);
        if !candidate.ambiguous {
            candidate.suggested_name = baseline
                .symbols
                .get(candidate.previous_ids.first().unwrap())
                .cloned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        let profile = Profile::unmapped("b".repeat(64));
        Report {
            schema: 1,
            baseline: "a".into(),
            target: "b".into(),
            profile,
            matched_bodies: 0,
            ambiguous_bodies: 0,
            evidence: BTreeMap::new(),
            conflicts: vec![],
            atom_candidates: BTreeMap::new(),
            errors: BTreeMap::new(),
        }
    }

    fn baseline() -> Profile {
        let mut p = Profile::unmapped("a".repeat(64));
        p.mapping = vec![Opcode::new(100, 7).unwrap(), Opcode::new(101, 22).unwrap()];
        p
    }

    #[test]
    fn remapping_keeps_aux_words_out_of_opcode_evidence() {
        let a = fixture(vec![100, 101, 100, 101, 100, 101, 100, 101, 101], vec![]);
        let b = fixture(vec![200, 101, 200, 101, 200, 101, 200, 101, 201], vec![]);
        let mut r = report();
        match_bodies(&mut r, "/test.lua", &a, &b, &baseline());
        finish(&mut r, &baseline(), &Profile::unmapped("b".repeat(64)));
        assert_eq!(r.matched_bodies, 1);
        assert_eq!(
            r.profile
                .mapping
                .iter()
                .map(|op| (op.raw, op.canonical))
                .collect::<Vec<_>>(),
            vec![(200, 7), (201, 22)]
        );
        assert!(!r.evidence.contains_key(&101));
    }

    #[test]
    fn ambiguous_and_literal_changed_bodies_do_not_supply_mappings() {
        let a = fixture(vec![101; 8], vec![vec![2, 0, 0, 0, 0, 0, 0, 0, 0]]);
        let mut b = a.clone();
        b.prototypes[0].words.fill(201);
        b.prototypes[0].constants[0][1] = 1;
        let mut r = report();
        match_bodies(&mut r, "test", &a, &b, &baseline());
        assert_eq!(r.matched_bodies, 0);
        b.prototypes[0].constants = a.prototypes[0].constants.clone();
        b.prototypes.push(b.prototypes[0].clone());
        b.prototypes[1].index = 1;
        match_bodies(&mut r, "test", &a, &b, &baseline());
        assert_eq!(r.ambiguous_bodies, 1);
        assert!(r.evidence.is_empty());
    }

    #[test]
    fn conflicting_bijection_is_withheld() {
        let mut r = report();
        for raw in [200, 201] {
            r.evidence.insert(
                raw,
                Evidence {
                    observations: 1,
                    anchors: vec![],
                    canonical_candidates: BTreeSet::from([22]),
                },
            );
        }
        finish(&mut r, &baseline(), &Profile::unmapped("b".repeat(64)));
        assert!(r.profile.mapping.is_empty());
        assert_eq!(r.conflicts.len(), 2);
    }

    #[test]
    fn vararg_prologue_has_an_independent_compiler_anchor() {
        let mut p = baseline();
        p.mapping.push(Opcode::new(99, 65).unwrap());
        let mut a = fixture(vec![99, 101], vec![]);
        a.prototypes[0].vararg = true;
        let mut b = fixture(vec![199, 201], vec![]);
        b.prototypes[0].vararg = true;
        let mut r = report();
        match_bodies(&mut r, "test", &a, &b, &p);
        finish(&mut r, &p, &Profile::unmapped("b".repeat(64)));
        assert_eq!(r.profile.mapping[0].canonical, 65);
        assert_eq!(r.evidence[&199].anchors[0].method, "vararg_prologue");
        assert_eq!(r.matched_bodies, 0);
        b.prototypes[0].words[0] |= 1 << 16;
        let mut rejected = report();
        match_bodies(&mut rejected, "test", &a, &b, &p);
        assert!(rejected.evidence.is_empty());
    }
}
