use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA: &str = "wfinspect.ghidra-report";
const SCHEMA_VERSION: u32 = 1;
const MAX_REPORT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
struct Report {
    schema: String,
    schema_version: u32,
    producer: Producer,
    program: Program,
    #[serde(default)]
    queries: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Producer {
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct Program {
    name: String,
    path: String,
    format: String,
    sha256: String,
    image_base: String,
    language: String,
    compiler: String,
}

#[derive(Debug, Serialize)]
pub struct ReportSummary {
    pub path: PathBuf,
    pub schema: String,
    pub schema_version: u32,
    pub producer: Producer,
    pub program: ProgramSummary,
    pub query_count: usize,
    pub query_kinds: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct ProgramSummary {
    pub name: String,
    pub path: String,
    pub format: String,
    pub sha256: String,
    pub image_base: String,
    pub language: String,
    pub compiler: String,
}

#[derive(Debug, Serialize)]
pub struct QueryList<S = ReportSummary> {
    pub report: S,
    pub queries: Vec<QuerySummary>,
}

#[derive(Debug, Serialize)]
pub struct QuerySummary {
    pub index: usize,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct QueryResult<S = ReportSummary> {
    pub report: S,
    pub index: usize,
    pub query: Value,
}

pub fn verify(path: &Path, expected: Option<&str>) -> Result<ReportSummary, String> {
    let report = load(path, expected)?;
    Ok(summary(path, &report))
}

pub fn list(path: &Path, expected: Option<&str>) -> Result<QueryList, String> {
    let report = load(path, expected)?;
    let queries = report
        .queries
        .iter()
        .enumerate()
        .map(|(index, query)| QuerySummary {
            index,
            kind: query
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned(),
            input: query.get("input").cloned(),
        })
        .collect();
    Ok(QueryList {
        report: summary(path, &report),
        queries,
    })
}

pub fn get(path: &Path, index: usize, expected: Option<&str>) -> Result<QueryResult, String> {
    let report = load(path, expected)?;
    let query = report
        .queries
        .get(index)
        .cloned()
        .ok_or_else(|| format!("Ghidra report query index out of range: {index}"))?;
    Ok(QueryResult {
        report: summary(path, &report),
        index,
        query,
    })
}

fn load(path: &Path, expected: Option<&str>) -> Result<Report, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if metadata.len() > MAX_REPORT_BYTES {
        return Err("Ghidra report exceeds 256 MiB limit".to_owned());
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let report: Report = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid Ghidra report: {error}"))?;
    validate(&report, expected)?;
    Ok(report)
}

fn summary(path: &Path, report: &Report) -> ReportSummary {
    let executable_sha256 = report.program.sha256.to_ascii_lowercase();

    let mut query_kinds = BTreeMap::new();
    for query in &report.queries {
        let kind = query
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        *query_kinds.entry(kind).or_insert(0) += 1;
    }
    ReportSummary {
        path: path.to_owned(),
        schema: report.schema.clone(),
        schema_version: report.schema_version,
        producer: report.producer.clone(),
        program: ProgramSummary {
            name: report.program.name.clone(),
            path: report.program.path.clone(),
            format: report.program.format.clone(),
            sha256: executable_sha256,
            image_base: report.program.image_base.clone(),
            language: report.program.language.clone(),
            compiler: report.program.compiler.clone(),
        },
        query_count: report.queries.len(),
        query_kinds,
    }
}

fn validate(report: &Report, expected: Option<&str>) -> Result<(), String> {
    if report.schema != SCHEMA || report.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported Ghidra report schema {} version {}",
            report.schema, report.schema_version
        ));
    }
    if report.producer.name != "ExportWfinspectReport" || report.producer.version != 1 {
        return Err(format!(
            "unsupported Ghidra report producer {} version {}",
            report.producer.name, report.producer.version
        ));
    }
    if !valid_sha256(&report.program.sha256) {
        return Err("Ghidra report has invalid executable SHA-256".to_owned());
    }
    if let Some(expected) = expected {
        if !valid_sha256(expected) {
            return Err(format!("invalid executable SHA-256: {expected}"));
        }
        let expected_sha = expected;
        if !report.program.sha256.eq_ignore_ascii_case(expected_sha) {
            return Err(format!(
                "Ghidra report executable {} does not match {}",
                report.program.sha256, expected_sha
            ));
        }
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn report(sha256: &str) -> Report {
        Report {
            schema: SCHEMA.to_owned(),
            schema_version: SCHEMA_VERSION,
            producer: Producer {
                name: "ExportWfinspectReport".to_owned(),
                version: 1,
            },
            program: Program {
                name: "program.exe".to_owned(),
                path: "program.exe".to_owned(),
                format: "Portable Executable".to_owned(),
                sha256: sha256.to_owned(),
                image_base: "140000000".to_owned(),
                language: "x86:LE:64:default".to_owned(),
                compiler: "windows".to_owned(),
            },
            queries: Vec::new(),
        }
    }

    #[test]
    fn validates_exact_build() {
        let sha = "d01b5cb5cff51afc5ffb7d3af051674aafa84000bee764780ff71d9d073cad93";
        assert!(validate(&report(sha), Some("d01b5cb5cff5")).is_err());
        assert!(validate(&report(sha), Some(sha)).is_ok());
        assert!(validate(&report(&"0".repeat(64)), Some(sha)).is_err());
    }

    #[test]
    fn rejects_unversioned_or_invalid_reports() {
        let mut value = report(&"0".repeat(64));
        value.schema_version = 2;
        assert!(validate(&value, None).is_err());
        value.schema_version = 1;
        value.program.sha256 = "bad".to_owned();
        assert!(validate(&value, None).is_err());
    }

    #[test]
    fn lists_and_returns_validated_queries() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "wfinspect-report-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut value = report(&"0".repeat(64));
        value.queries.push(serde_json::json!({
            "kind": "string",
            "input": "SyncInventoryFromDB",
            "matches": []
        }));
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        let listed = list(&path, None).unwrap();
        assert_eq!(listed.queries.len(), 1);
        assert_eq!(listed.queries[0].kind, "string");
        let selected = get(&path, 0, None).unwrap();
        assert_eq!(selected.query["input"], "SyncInventoryFromDB");
        assert!(get(&path, 1, None).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
