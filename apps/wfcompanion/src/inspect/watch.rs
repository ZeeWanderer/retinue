use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::ops::Range;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::memory::{OpenedSource, Source};

const MAX_LINE: usize = 2 * 1024 * 1024;
const SUMMARY_RESERVE: u64 = 16384;

#[derive(Clone, Copy)]
pub struct Options {
    pub duration: Duration,
    pub interval: Duration,
    pub max_bytes: u64,
}

impl Options {
    pub fn validate(self) -> Result<(), String> {
        if self.duration.is_zero() || self.duration > Duration::from_secs(1800) {
            return Err("recording duration must be >0 and <=1800 seconds".into());
        }
        if !(Duration::from_millis(10)..=Duration::from_secs(5)).contains(&self.interval) {
            return Err("poll interval must be between 10 and 5000 ms".into());
        }
        if !(65536..=1024 * 1024 * 1024).contains(&self.max_bytes) {
            return Err("recording byte limit must be between 64 KiB and 1 GiB".into());
        }
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Summary {
    pub samples: u64,
    pub skipped_polls: u64,
    pub elapsed_ms: u128,
    pub sample_us_mean: u128,
    pub sample_us_max: u128,
    pub unavailable_reads: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained_through_sample: Option<u64>,
    pub stop: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(super) struct Journal {
    file: File,
    started: Instant,
    bytes: u64,
    limit: u64,
    pub full: bool,
}

impl Journal {
    pub fn create(
        path: &Path,
        schema: &str,
        header: Value,
        options: Options,
    ) -> Result<Self, String> {
        options.validate()?;
        let mut journal = Self {
            file: OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(|e| format!("could not create {}: {e}", path.display()))?,
            started: Instant::now(),
            bytes: 0,
            limit: options.max_bytes,
            full: false,
        };
        if !journal.append(
            "start",
            &json!({
                "schema": schema, "version": 1, "interval_ms": options.interval.as_millis(),
                "seconds": options.duration.as_secs_f64(), "max_bytes": options.max_bytes,
                "source": header,
            }),
        )? {
            return Err("recording header exceeds byte limit".into());
        }
        Ok(journal)
    }

    pub fn append(&mut self, kind: &str, data: &impl Serialize) -> Result<bool, String> {
        let mut line = serde_json::to_vec(&json!({
            "kind": kind, "unix_ms": super::unix_time_ms(),
            "elapsed_us": self.started.elapsed().as_micros(), "data": data,
        }))
        .map_err(|e| e.to_string())?;
        line.push(b'\n');
        if line.len() > MAX_LINE {
            return Err("record exceeds 2 MiB line limit".into());
        }
        let reserve = if kind == "summary" {
            0
        } else {
            SUMMARY_RESERVE
        };
        if self.bytes + line.len() as u64 + reserve > self.limit {
            self.full = true;
            return Ok(false);
        }
        self.file.write_all(&line).map_err(|e| e.to_string())?;
        self.bytes += line.len() as u64;
        Ok(true)
    }

    pub fn finish(&mut self, summary: &impl Serialize) -> Result<(), String> {
        if !self.append("summary", summary)? {
            return Err("recording summary exceeds byte limit".into());
        }
        self.file.sync_data().map_err(|e| e.to_string())
    }
}

pub(super) fn poll(
    options: Options,
    journal: &mut Journal,
    mut sample: impl FnMut(u64, &mut Journal) -> Result<(), String>,
) -> Result<Summary, String> {
    options.validate()?;
    let start = Instant::now();
    let mut next = Duration::ZERO;
    let mut total_us = 0;
    let mut summary = Summary::default();
    while start.elapsed() < options.duration && !journal.full {
        std::thread::sleep(next.saturating_sub(start.elapsed()));
        if start.elapsed() >= options.duration {
            break;
        }
        let tick = Instant::now();
        let result = sample(summary.samples, journal);
        let micros = tick.elapsed().as_micros();
        summary.samples += 1;
        total_us += micros;
        summary.sample_us_max = summary.sample_us_max.max(micros);
        if let Err(error) = result {
            summary.error = Some(error);
            break;
        }
        let (scheduled, skipped) = next_poll(next, start.elapsed(), options.interval);
        next = scheduled.min(options.duration);
        if skipped != 0 {
            summary.skipped_polls += skipped;
            journal.append("sampling_gap", &json!({"skipped_polls": skipped}))?;
        }
    }
    summary.elapsed_ms = start.elapsed().as_millis();
    summary.sample_us_mean = total_us / u128::from(summary.samples.max(1));
    summary.stop = if summary.error.is_some() {
        "error"
    } else if journal.full {
        "byte_limit"
    } else {
        "duration"
    }
    .into();
    Ok(summary)
}

fn next_poll(previous: Duration, now: Duration, interval: Duration) -> (Duration, u64) {
    let next = previous + interval;
    let skipped = if now >= next {
        ((now - next).as_nanos() / interval.as_nanos() + 1) as u64
    } else {
        0
    };
    (next + interval * skipped as u32, skipped)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub address: u64,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn memory(
    source: Source,
    ranges: &[Range<u64>],
    output: &Path,
    options: Options,
) -> Result<Summary, String> {
    options.validate()?;
    validate_ranges(ranges)?;
    let source = OpenedSource::open(source)?;
    let mut journal = Journal::create(
        output,
        "wfinspect.memory-watch",
        json!({
            "memory": source.metadata, "ranges": ranges,
            "semantics": "sampled bytes; changes between polls can be missed; ranges are not atomic",
        }),
        options,
    )?;
    eprintln!("Recording {}", output.display());
    let mut previous = Vec::new();
    let mut unavailable = 0;
    let mut retained_through = None;
    let mut summary = poll(options, &mut journal, |index, journal| {
        let blocks = ranges
            .iter()
            .map(|range| {
                let length = (range.end - range.start) as usize;
                let mut data = vec![0; length];
                let result = if source.memory.supports_read_range(range.start, length) {
                    source
                        .memory
                        .read_exact_at(&mut data, range.start)
                        .map_err(|e| e.to_string())
                } else {
                    Err("outside readable mappings or capture".into())
                };
                Block {
                    address: range.start,
                    length,
                    hex: result.as_ref().ok().map(|_| hex(&data)),
                    error: result.err(),
                }
            })
            .collect::<Vec<_>>();
        unavailable += blocks.iter().filter(|block| block.error.is_some()).count() as u64;
        if blocks != previous {
            if !journal.append("sample", &json!({"index": index, "blocks": blocks}))? {
                return Ok(());
            }
            previous = blocks;
        }
        retained_through = Some(index);
        Ok(())
    })?;
    summary.unavailable_reads = unavailable;
    summary.retained_through_sample = retained_through;
    journal.finish(&summary)?;
    Ok(summary)
}

fn validate_ranges(ranges: &[Range<u64>]) -> Result<(), String> {
    let mut ordered = ranges.to_vec();
    ordered.sort_by_key(|range| range.start);
    if ordered.is_empty()
        || ordered.len() > 64
        || ordered
            .iter()
            .any(|r| r.start >= r.end || r.end - r.start > 65536)
        || ordered.iter().map(|r| r.end - r.start).sum::<u64>() > 65536
        || ordered.windows(2).any(|w| w[0].end > w[1].start)
    {
        return Err("watch requires 1..64 nonoverlapping ranges totaling at most 64 KiB".into());
    }
    Ok(())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 15) as usize] as char);
    }
    text
}

pub(super) fn records(
    path: &Path,
    mut accept: impl FnMut(Value) -> Result<(), String>,
) -> Result<bool, String> {
    let mut input = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut complete = false;
    loop {
        let mut line = Vec::new();
        let bytes = (&mut input)
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if bytes == 0 {
            return Ok(complete);
        }
        if complete {
            return Err("data follows recording summary".into());
        }
        if bytes > MAX_LINE {
            return Err("record exceeds 2 MiB line limit".into());
        }
        if line.last() != Some(&b'\n') {
            return Ok(false);
        }
        let value: Value =
            serde_json::from_slice(&line).map_err(|e| format!("invalid recording: {e}"))?;
        complete = value["kind"] == "summary";
        accept(value)?;
    }
}

pub fn replay_memory(path: &Path, sample: Option<u64>) -> Result<Value, String> {
    let mut header = None;
    let mut samples = 0;
    let mut selected = None;
    let mut previous = None;
    let mut summary = None;
    let complete = records(path, |record| {
        match record["kind"].as_str() {
            Some("start") if header.is_none() => {
                if record["data"]["schema"] != "wfinspect.memory-watch"
                    || record["data"]["version"] != 1
                {
                    return Err("unsupported memory recording".into());
                }
                header = Some(record["data"].clone());
            }
            Some("sample") if header.is_some() => {
                let index = record["data"]["index"]
                    .as_u64()
                    .ok_or("missing sample index")?;
                if previous.is_some_and(|prev| index <= prev) {
                    return Err("sample indices are not increasing".into());
                }
                previous = Some(index);
                samples += 1;
                if sample.is_some_and(|requested| index <= requested) {
                    selected = Some(record);
                }
            }
            Some("sampling_gap") if header.is_some() => {}
            Some("summary") if header.is_some() => summary = Some(record["data"].clone()),
            _ => return Err("unexpected memory recording record".into()),
        }
        Ok(())
    })?;
    if header.is_none() {
        return Err("recording has no header".into());
    }
    if let Some(requested) = sample {
        if complete
            && summary
                .as_ref()
                .and_then(|v| v["retained_through_sample"].as_u64())
                .is_none_or(|last| requested > last)
        {
            return Err("sample is beyond retained recording coverage".into());
        }
        if selected.is_none() {
            return Err("no retained sample at that index".into());
        }
        if !complete && previous.is_some_and(|last| requested > last) {
            return Err(
                "cannot extrapolate beyond the last sample of an interrupted recording".into(),
            );
        }
    }
    Ok(json!({"path": path, "complete": complete, "header": header,
        "stored_samples": samples, "summary": summary, "sample": selected}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_samples_skip_deadlines_without_catchup_bursts() {
        let ms = Duration::from_millis;
        assert_eq!(next_poll(ms(0), ms(7), ms(50)), (ms(50), 0));
        assert_eq!(next_poll(ms(0), ms(175), ms(50)), (ms(200), 3));
        assert_eq!(next_poll(ms(50), ms(100), ms(50)), (ms(150), 1));
    }

    #[test]
    fn watch_bounds_and_hex_are_exact() {
        assert!(validate_ranges(&[100..108, 120..128]).is_ok());
        assert!(validate_ranges(&[100..108, 107..109]).is_err());
        assert!(validate_ranges(&[]).is_err());
        assert!(validate_ranges(&[0..65537]).is_err());
        assert_eq!(hex(&[0, 15, 128, 255]), "000f80ff");
        let options = Options {
            duration: Duration::from_secs(1),
            interval: Duration::from_millis(50),
            max_bytes: 65536,
        };
        assert!(options.validate().is_ok());
        assert!(
            Options {
                interval: Duration::from_millis(1),
                ..options
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn private_recordings_preserve_existing_files_and_reserve_a_summary() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "wfinspect-watch-budget-{}.jsonl",
            std::process::id()
        ));
        let options = Options {
            duration: Duration::from_secs(1),
            interval: Duration::from_millis(50),
            max_bytes: 65536,
        };
        let mut journal = Journal::create(&path, "test", json!({}), options).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(Journal::create(&path, "test", json!({}), options).is_err());
        while journal.append("sample", &"x".repeat(2000)).unwrap() {}
        assert!(journal.full);
        journal.finish(&json!({"stop": "byte_limit"})).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= options.max_bytes);
        assert!(records(&path, |_| Ok(())).unwrap());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_retains_error_samples_and_detects_interrupted_files() {
        let path = std::env::temp_dir().join(format!(
            "wfinspect-memory-fixture-{}.jsonl",
            std::process::id()
        ));
        let options = Options {
            duration: Duration::from_secs(1),
            interval: Duration::from_millis(50),
            max_bytes: 65536,
        };
        let mut journal = Journal::create(
            &path,
            "wfinspect.memory-watch",
            json!({"synthetic_test": true}),
            options,
        )
        .unwrap();
        journal
            .append(
                "sample",
                &json!({"index": 0, "blocks": [{"address": 65536, "length": 1, "hex": "ff"}]}),
            )
            .unwrap();
        journal.append("sample", &json!({"index": 3, "blocks": [{"address": 65536, "length": 1, "error": "unmapped"}]})).unwrap();
        assert_eq!(
            replay_memory(&path, Some(2)).unwrap()["sample"]["data"]["blocks"][0]["hex"],
            "ff"
        );
        assert_eq!(
            replay_memory(&path, Some(3)).unwrap()["sample"]["data"]["blocks"][0]["error"],
            "unmapped"
        );
        assert!(replay_memory(&path, Some(4)).is_err());
        assert_eq!(replay_memory(&path, None).unwrap()["complete"], false);
        journal
            .finish(&json!({"samples": 5, "retained_through_sample": 4}))
            .unwrap();
        assert_eq!(replay_memory(&path, Some(4)).unwrap()["complete"], true);
        assert!(replay_memory(&path, Some(5)).is_err());
        eprintln!("memory replay fixture: {}", path.display());
    }

    #[test]
    fn byte_limit_does_not_extend_last_bytes_over_an_unstored_change() {
        let path = std::env::temp_dir().join(format!(
            "wfinspect-watch-coverage-{}.jsonl",
            std::process::id()
        ));
        let options = Options {
            duration: Duration::from_secs(1),
            interval: Duration::from_millis(50),
            max_bytes: 65536,
        };
        let mut journal =
            Journal::create(&path, "wfinspect.memory-watch", json!({}), options).unwrap();
        journal
            .append("sample", &json!({"index": 0, "blocks": []}))
            .unwrap();
        assert!(
            !journal
                .append("sample", &json!({"index": 1, "blocks": "x".repeat(65536)}))
                .unwrap()
        );
        journal
            .finish(&json!({"samples": 2, "retained_through_sample": 0, "stop": "byte_limit"}))
            .unwrap();
        assert!(replay_memory(&path, Some(0)).is_ok());
        assert!(replay_memory(&path, Some(1)).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_and_partial_lines_are_not_parsed_as_valid_records() {
        let path = std::env::temp_dir().join(format!(
            "wfinspect-watch-truncated-{}.jsonl",
            std::process::id()
        ));
        std::fs::write(&path, b"{\"kind\":\"start\"}").unwrap();
        assert!(!records(&path, |_| panic!("partial line was accepted")).unwrap());
        std::fs::write(&path, vec![b'x'; MAX_LINE + 1]).unwrap();
        assert!(records(&path, |_| Ok(())).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
