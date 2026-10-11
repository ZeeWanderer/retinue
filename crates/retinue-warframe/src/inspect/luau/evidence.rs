use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::corpus::{self, Snapshot};
use crate::inspect::report;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bindings {
    schema: u8,
    executable_sha256: String,
    packages: Option<PathBuf>,
    #[serde(default)]
    properties: Vec<PathBuf>,
    #[serde(default)]
    native: BTreeMap<String, NativeBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeBinding {
    report: PathBuf,
    query: usize,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Evidence {
    pub schema: u8,
    pub snapshot: String,
    pub executable_sha256: String,
    pub packages_sha256: Option<String>,
    pub sources: BTreeMap<PathBuf, String>,
    pub decoders: BTreeSet<String>,
    pub properties: BTreeMap<String, Value>,
    pub native: BTreeMap<String, Native>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Native {
    pub entry: String,
    pub sha256: String,
    pub instructions: usize,
}

#[derive(Deserialize)]
struct Properties {
    version: u32,
    stats: Value,
    packages_sha256: String,
    decoder_sha256: String,
    duplicate_paths: BTreeSet<String>,
    requested: Vec<String>,
    types: BTreeMap<String, Value>,
}

pub fn import(workspace: &Path, name: &str, bindings: &Path) -> Result<Evidence, String> {
    let snapshot = corpus::load(workspace, name)?;
    let input: Bindings = corpus::read_json(bindings)?;
    if input.schema != 1 || input.executable_sha256 != snapshot.profile.executable_sha256 {
        return Err(
            "evidence bindings must use schema 1 and the snapshot's executable SHA-256".into(),
        );
    }
    let base = bindings.parent().unwrap_or(Path::new("."));
    let packages_sha256 = input
        .packages
        .map(|p| corpus::hash_file(&base.join(p)))
        .transpose()?;
    let mut evidence = Evidence {
        schema: 1,
        snapshot: name.into(),
        executable_sha256: input.executable_sha256,
        packages_sha256,
        ..Evidence::default()
    };
    for file in input.properties {
        let path = base.join(file);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let props: Properties =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        evidence.add_properties(props)?;
        evidence.sources.insert(path, corpus::hash(&bytes));
    }
    for (id, binding) in input.native {
        let path = base.join(binding.report);
        let hash = corpus::hash_file(&path)?;
        let result = report::get(&path, binding.query, Some(&evidence.executable_sha256))?;
        let native = native(&result.query).map_err(|e| format!("{id}: {e}"))?;
        if hash != corpus::hash_file(&path)? {
            return Err(format!("{} changed during import", path.display()));
        }
        evidence.sources.insert(path, hash);
        evidence.native.insert(id, native);
    }
    if evidence.sources.is_empty() {
        return Err("evidence bindings contain no property or native reports".into());
    }
    corpus::write_new_json(&path(workspace, &snapshot), &evidence)?;
    Ok(evidence)
}

impl Evidence {
    fn add_properties(&mut self, report: Properties) -> Result<(), String> {
        if report.version != 46 || report.stats.get("aligned") != Some(&json!(true)) {
            return Err("unsupported or unaligned Packages property report".into());
        }
        if self.packages_sha256.as_ref() != Some(&report.packages_sha256) {
            return Err("property report does not match the supplied Packages file".into());
        }
        if report.decoder_sha256.len() != 64
            || !report.decoder_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("property report lacks a decoder SHA-256".into());
        }
        for root in &report.requested {
            if !report.types.contains_key(root) {
                return Err(format!("property report omitted requested root {root}"));
            }
        }
        for (name, value) in report.types {
            if report.duplicate_paths.contains(&name) {
                return Err(format!("ambiguous property record {name}"));
            }
            if !name.starts_with('/') || !value.is_object() {
                return Err(format!("invalid property record {name}"));
            }
            if value.get("missing") != Some(&json!(true)) {
                if value.get("parent").and_then(Value::as_str).is_none()
                    || !value
                        .get("ownText")
                        .is_some_and(|v| v.is_null() || v.is_string())
                {
                    return Err(format!("incomplete property record {name}"));
                }
            }
            if self.properties.get(&name).is_some_and(|old| old != &value) {
                return Err(format!("conflicting property records for {name}"));
            }
            self.properties.insert(name, value);
        }
        self.decoders.insert(report.decoder_sha256);
        Ok(())
    }
}

fn native(query: &Value) -> Result<Native, String> {
    if query.get("kind").and_then(Value::as_str) != Some("function")
        || query.get("found") != Some(&json!(true))
        || query.get("instructions_truncated") != Some(&json!(false))
    {
        return Err("native evidence needs a found, untruncated function query".into());
    }
    let entry = query
        .get("entry")
        .and_then(Value::as_str)
        .ok_or("native function has no entry")?;
    let instructions = query
        .get("instructions")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
        .ok_or("native function has no instructions")?;
    if instructions.iter().any(|i| {
        i.get("text").and_then(Value::as_str).is_none()
            || i.get("address").and_then(Value::as_str).is_none()
    }) {
        return Err("native function has malformed instructions".into());
    }
    // Keep addresses: relocation is a review, not proof that global/call bindings survived.
    Ok(Native {
        entry: entry.into(),
        sha256: corpus::hash(&serde_json::to_vec(instructions).unwrap()),
        instructions: instructions.len(),
    })
}

fn path(workspace: &Path, snapshot: &Snapshot) -> PathBuf {
    workspace
        .join("evidence")
        .join(format!("{}.json", snapshot.name))
}

pub fn load(workspace: &Path, snapshot: &Snapshot) -> Result<Option<Evidence>, String> {
    let path = path(workspace, snapshot);
    match std::fs::read(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
        Ok(bytes) => {
            let evidence: Evidence = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if evidence.schema != 1
                || evidence.snapshot != snapshot.name
                || evidence.executable_sha256 != snapshot.profile.executable_sha256
            {
                return Err(format!("{}: mismatched evidence identity", path.display()));
            }
            Ok(Some(evidence))
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct PropertyState {
    pub sha256: Option<String>,
    pub gaps: BTreeSet<String>,
    pub script_candidates: BTreeSet<String>,
}

pub fn property(evidence: Option<&Evidence>, root: &str) -> PropertyState {
    let mut result = PropertyState::default();
    let mut records = BTreeMap::new();
    let mut current = root;
    while !current.is_empty() {
        if records.contains_key(current) {
            result
                .gaps
                .insert(format!("inheritance cycle at {current}"));
            break;
        }
        let Some(value) = evidence.and_then(|e| e.properties.get(current)) else {
            result.gaps.insert(format!("not captured: {current}"));
            break;
        };
        records.insert(current, value);
        if value.get("missing") == Some(&json!(true)) {
            result
                .gaps
                .insert(format!("unresolved property: {current}"));
            break;
        }
        if let Some(text) = value.get("ownText").and_then(Value::as_str) {
            for line in text.lines() {
                if let Some(script) = line.trim().strip_prefix("Script=") {
                    let script = script.trim().trim_matches('"');
                    if script.is_empty() || script == "{" {
                        continue;
                    }
                    if script.starts_with('/') && script.ends_with(".lua") {
                        result.script_candidates.insert(script.into());
                    } else if !script.contains('/') && script.ends_with(".lua") {
                        if let Some((directory, _)) = current.rsplit_once('/') {
                            result
                                .script_candidates
                                .insert(format!("{directory}/{script}"));
                        }
                    } else {
                        result
                            .gaps
                            .insert(format!("unresolved script reference: {script}"));
                    }
                }
            }
        }
        let Some(parent) = value.get("parent").and_then(Value::as_str) else {
            result
                .gaps
                .insert(format!("missing parent field: {current}"));
            break;
        };
        current = parent;
    }
    if !records.is_empty() {
        result.sha256 = Some(corpus::hash(&serde_json::to_vec(&records).unwrap()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::{profile::Profile, tracking};
    use super::*;

    fn properties() -> Properties {
        serde_json::from_value(json!({"version":46, "stats":{"aligned":true},
        "packages_sha256":"a".repeat(64), "decoder_sha256":"b".repeat(64),
        "duplicate_paths":[], "requested":["/child"], "types":{
            "/child":{"parent":"/parent", "ownText":null},
            "/parent":{"parent":"", "ownText":"Value=1\n"}
        }}))
        .unwrap()
    }

    #[test]
    fn inherited_records_and_unknown_fields_are_not_discarded() {
        let mut evidence = Evidence {
            packages_sha256: Some("a".repeat(64)),
            ..Evidence::default()
        };
        evidence.add_properties(properties()).unwrap();
        let original = property(Some(&evidence), "/child");
        assert!(original.gaps.is_empty());
        evidence.properties.get_mut("/parent").unwrap()["ownText"] =
            json!("Value=1\nNewField=2\nScript={\nScript=new.lua\n}\n");
        let changed = property(Some(&evidence), "/child");
        assert_ne!(original.sha256, changed.sha256);
        assert!(changed.script_candidates.contains("/new.lua"));
        assert!(changed.gaps.is_empty());
        evidence.properties.remove("/parent");
        assert!(!property(Some(&evidence), "/child").gaps.is_empty());
        evidence.properties.get_mut("/child").unwrap()["parent"] = json!("/child");
        assert!(
            property(Some(&evidence), "/child")
                .gaps
                .iter()
                .any(|g| g.contains("cycle"))
        );
        assert!(!property(None, "/child").gaps.is_empty());
    }

    #[test]
    fn stale_ambiguous_or_conflicting_properties_fail_import() {
        let mut evidence = Evidence::default();
        assert!(evidence.add_properties(properties()).is_err());
        evidence.packages_sha256 = Some("a".repeat(64));
        let mut report = properties();
        report.stats["aligned"] = json!(false);
        assert!(evidence.add_properties(report).is_err());
        let mut report = properties();
        report.duplicate_paths.insert("/child".into());
        assert!(evidence.add_properties(report).is_err());
        evidence.add_properties(properties()).unwrap();
        let mut report = properties();
        report.types.get_mut("/child").unwrap()["ownText"] = json!("Value=2");
        assert!(evidence.add_properties(report).is_err());
        let mut report = properties();
        report.types.remove("/child");
        assert!(evidence.add_properties(report).is_err());
    }

    #[test]
    fn incomplete_native_query_is_not_a_fingerprint() {
        let mut q = json!({"kind":"function", "found":true, "entry":"140001000",
            "instructions_truncated":false, "instructions":[{"address":"140001000","text":"RET"}]});
        assert!(native(&q).is_ok());
        q["instructions_truncated"] = json!(true);
        assert!(native(&q).is_err());
        q["instructions_truncated"] = json!(false);
        q["instructions"] = json!([]);
        assert!(native(&q).is_err());
    }

    struct Workspace(PathBuf);
    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn retained_evidence_is_immutable_and_changes_propagate() {
        let w = Workspace(std::env::temp_dir().join(format!(
            "wfinspect-evidence-{}-{}",
            std::process::id(),
            crate::inspect::unix_time_ms()
        )));
        std::fs::create_dir(&w.0).unwrap();
        let mut profile = Profile::unmapped("c".repeat(64));
        profile.status = "reviewed".into();
        let a = Snapshot {
            schema: 1,
            extractor_version: "test".into(),
            name: "before".into(),
            captured_unix_ms: 0,
            source: "test".into(),
            profile,
            packages: vec![],
            path_filter: None,
            scripts: BTreeMap::new(),
            errors: BTreeMap::new(),
        };
        let mut b = a.clone();
        b.name = "after".into();
        for snapshot in [&a, &b] {
            corpus::write_new_json(
                &w.0.join("snapshots")
                    .join(format!("{}.json", snapshot.name)),
                snapshot,
            )
            .unwrap();
        }
        let package = w.0.join("Packages.bin");
        std::fs::write(&package, b"fixture").unwrap();
        let mut props = serde_json::to_value(json!({"version":46,"stats":{"aligned":true},
            "packages_sha256":corpus::hash(b"fixture"), "decoder_sha256":"b".repeat(64),
            "duplicate_paths":[], "requested":["/child"], "types":{
            "/child":{"parent":"/parent", "ownText":null},
            "/parent":{"parent":"", "ownText":"Value=1"}}}))
        .unwrap();
        corpus::write_new_json(&w.0.join("props.json"), &props).unwrap();
        let bindings = w.0.join("bindings.json");
        corpus::write_new_json(
            &bindings,
            &json!({"schema":1,"executable_sha256":"c".repeat(64),
            "packages":"Packages.bin", "properties":["props.json"]}),
        )
        .unwrap();
        import(&w.0, "before", &bindings).unwrap();
        assert!(import(&w.0, "before", &bindings).is_err());
        props["types"]["/parent"]["ownText"] = json!("Value=2\nScript=/new.lua\n");
        std::fs::write(w.0.join("props.json"), serde_json::to_vec(&props).unwrap()).unwrap();
        import(&w.0, "after", &bindings).unwrap();
        let manifest = serde_json::from_value(json!({"schema":1,"features":{
            "shared":{"properties":["/child"]}, "effect":{"depends_on":["shared"]}, "independent":{}}})).unwrap();
        let result = tracking::compare(&w.0, &a, &b, &manifest).unwrap();
        assert_eq!(
            result.features["shared"].properties["/child"].status,
            "record_or_parent_changed_review"
        );
        assert_eq!(
            result.features["shared"].scripts["/new.lua"],
            "not_captured"
        );
        assert_eq!(result.features["effect"].dependency_review, vec!["shared"]);
        assert!(result.features["effect"].changed);
        assert!(!result.features["independent"].changed);
        assert!(!result.features["independent"].review_required);
        assert!(load(&w.0, &a).unwrap().unwrap().properties["/parent"]["ownText"] == "Value=1");
        std::fs::write(w.0.join("evidence/after.json"), b"invalid").unwrap();
        assert!(
            !tracking::compare(&w.0, &a, &b, &manifest)
                .unwrap()
                .evidence_errors
                .is_empty()
        );
        let mut wrong = a.clone();
        wrong.profile.executable_sha256 = "d".repeat(64);
        assert!(load(&w.0, &wrong).is_err());
    }
}
