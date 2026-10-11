use crate::game_observer::adapter;
use retinue_native::inspect::report as native;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ReportSummary {
    #[serde(flatten)]
    pub report: native::ReportSummary,
    pub adapter: AdapterMatch,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AdapterMatch {
    Supported { id: &'static str },
    UnknownBuild,
}

fn expected_sha(expected: Option<&str>) -> Option<&str> {
    expected.map(|key| adapter::resolve_key(key).map_or(key, |adapter| adapter.sha256))
}

fn annotate(report: native::ReportSummary) -> ReportSummary {
    let adapter = match adapter::resolve(&report.program.sha256) {
        Some(adapter) => AdapterMatch::Supported { id: adapter.id },
        None => AdapterMatch::UnknownBuild,
    };
    ReportSummary { report, adapter }
}

pub fn verify(path: &Path, expected: Option<&str>) -> Result<ReportSummary, String> {
    native::verify(path, expected_sha(expected)).map(annotate)
}

pub fn list(
    path: &Path,
    expected: Option<&str>,
) -> Result<native::QueryList<ReportSummary>, String> {
    let result = native::list(path, expected_sha(expected))?;
    Ok(native::QueryList {
        report: annotate(result.report),
        queries: result.queries,
    })
}

pub fn get(
    path: &Path,
    index: usize,
    expected: Option<&str>,
) -> Result<native::QueryResult<ReportSummary>, String> {
    let result = native::get(path, index, expected_sha(expected))?;
    Ok(native::QueryResult {
        report: annotate(result.report),
        index: result.index,
        query: result.query,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_keys_resolve_only_in_game_layer() {
        assert_eq!(
            expected_sha(Some("d01b5cb5cff5")),
            Some("d01b5cb5cff51afc5ffb7d3af051674aafa84000bee764780ff71d9d073cad93")
        );
        assert_eq!(expected_sha(None), None);
    }
}
