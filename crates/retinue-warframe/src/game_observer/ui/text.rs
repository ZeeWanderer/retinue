use std::collections::{BTreeSet, HashMap};
use std::io;
use std::time::Instant;

use memchr::memmem;

use super::{
    CHUNK, TextMatch, TextReference, TextScan, TextScanMetrics, TextTerm, address_low_bits,
    address_may_match,
};
use crate::game_observer::memory::{ProcessMemory, ScanRange};

const MAX_TERMS: usize = 16;
const MAX_TERM_BYTES: usize = 512;
const MAX_MATCHES: usize = 128;
const MAX_REFERENCES: usize = 128;
const CONTEXT_BEFORE: usize = 48;
const CONTEXT_AFTER: usize = 80;

struct Pattern {
    term: usize,
    encoding: &'static str,
    bytes: Vec<u8>,
}

pub(super) fn scan(memory: &ProcessMemory, requested: &[String]) -> Result<TextScan, String> {
    let started = Instant::now();
    let terms = validated_terms(requested)?;
    let patterns = patterns(&terms);
    let regions = memory.scan_ranges();
    let mapped_bytes_per_pass = regions
        .iter()
        .map(|region| region.end.saturating_sub(region.start))
        .sum();
    let mut results = terms
        .into_iter()
        .map(|term| TextTerm {
            term,
            matches: Vec::new(),
            truncated: false,
        })
        .collect::<Vec<_>>();

    let text_started = Instant::now();
    scan_matches(memory, &regions, &patterns, &mut results)?;
    let text_scan_ms = text_started.elapsed().as_millis();

    let reference_started = Instant::now();
    scan_references(memory, &regions, &mut results)
        .map_err(|error| format!("could not scan Warframe UI text references: {error}"))?;
    let reference_scan_ms = reference_started.elapsed().as_millis();

    Ok(TextScan {
        pid: memory.pid(),
        terms: results,
        metrics: TextScanMetrics {
            regions: regions.len(),
            mapped_bytes_per_pass,
            text_scan_ms,
            reference_scan_ms,
            total_ms: started.elapsed().as_millis(),
        },
    })
}

pub(super) fn display_text_pointer(bytes: &[u8]) -> Option<u64> {
    let pointer = u64::from_le_bytes(bytes.get(0xb0..0xb8)?.try_into().ok()?);
    let repeated = |left: usize, right: usize| {
        let left = u32::from_le_bytes(bytes.get(left..left + 4)?.try_into().ok()?);
        let right = u32::from_le_bytes(bytes.get(right..right + 4)?.try_into().ok()?);
        (left == right && left <= 16_384).then_some(left)
    };
    matches!(
        (repeated(0x20, 0x24), repeated(0x60, 0x64)),
        (Some(first), Some(second)) if first > 0 || second > 0
    )
    .then_some(pointer)
}

fn scan_matches(
    memory: &ProcessMemory,
    regions: &[ScanRange],
    patterns: &[Pattern],
    results: &mut [TextTerm],
) -> Result<(), String> {
    let overlap = patterns
        .iter()
        .map(|pattern| pattern.bytes.len())
        .max()
        .unwrap_or(1)
        .saturating_sub(1);
    let mut buffer = vec![0_u8; CHUNK];
    let mut tail = Vec::new();
    for region in regions {
        let label = region_label(region);
        let mut offset = region.start;
        tail.clear();
        while offset < region.end {
            let wanted = usize::try_from((region.end - offset).min(CHUNK as u64)).unwrap();
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if matches!(error.raw_os_error(), Some(5 | 14)) => break,
                Err(error) => {
                    return Err(format!("could not scan Warframe UI text: {error}"));
                }
            };
            let base = offset.saturating_sub(tail.len() as u64);
            let mut searchable = Vec::with_capacity(tail.len() + read);
            searchable.extend_from_slice(&tail);
            searchable.extend_from_slice(&buffer[..read]);
            for pattern in patterns {
                let result = &mut results[pattern.term];
                if result.matches.len() >= MAX_MATCHES {
                    result.truncated = true;
                    continue;
                }
                for index in memmem::find_iter(&searchable, &pattern.bytes) {
                    let address = base + index as u64;
                    if result
                        .matches
                        .iter()
                        .any(|found| found.address == address && found.encoding == pattern.encoding)
                    {
                        continue;
                    }
                    if result.matches.len() >= MAX_MATCHES {
                        result.truncated = true;
                        break;
                    }
                    result.matches.push(TextMatch {
                        address,
                        encoding: pattern.encoding,
                        region: label.clone(),
                        context: context(&searchable, index, pattern.bytes.len()),
                        references: Vec::new(),
                        references_truncated: false,
                    });
                }
            }
            tail.clear();
            tail.extend_from_slice(&searchable[searchable.len().saturating_sub(overlap)..]);
            offset += read as u64;
        }
    }
    Ok(())
}

fn validated_terms(requested: &[String]) -> Result<Vec<String>, String> {
    if requested.is_empty() {
        return Err("UI text scan requires at least one term".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut terms = Vec::new();
    for term in requested {
        if term.is_empty() {
            return Err("UI text scan terms must not be empty".to_owned());
        }
        let utf16_bytes = term.encode_utf16().count() * 2;
        if term.len() > MAX_TERM_BYTES || utf16_bytes > MAX_TERM_BYTES {
            return Err(format!(
                "UI text scan term exceeds {MAX_TERM_BYTES} encoded bytes"
            ));
        }
        if seen.insert(term.clone()) {
            terms.push(term.clone());
        }
    }
    if terms.len() > MAX_TERMS {
        return Err(format!("UI text scan accepts at most {MAX_TERMS} terms"));
    }
    Ok(terms)
}

fn patterns(terms: &[String]) -> Vec<Pattern> {
    terms
        .iter()
        .enumerate()
        .flat_map(|(term, value)| {
            let utf16 = value
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            [
                Pattern {
                    term,
                    encoding: "utf8",
                    bytes: value.as_bytes().to_vec(),
                },
                Pattern {
                    term,
                    encoding: "utf16le",
                    bytes: utf16,
                },
            ]
        })
        .collect()
}

fn scan_references(
    memory: &ProcessMemory,
    regions: &[ScanRange],
    terms: &mut [TextTerm],
) -> io::Result<()> {
    let mut targets = HashMap::<u64, Vec<(usize, usize)>>::new();
    for (term_index, term) in terms.iter().enumerate() {
        for (match_index, found) in term.matches.iter().enumerate() {
            targets
                .entry(found.address)
                .or_default()
                .push((term_index, match_index));
        }
    }
    if targets.is_empty() {
        return Ok(());
    }
    let min_target = targets.keys().copied().min().unwrap();
    let max_target = targets.keys().copied().max().unwrap();
    let low_bits = address_low_bits(targets.keys().copied());
    let mut buffer = vec![0_u8; CHUNK];

    for region in regions {
        let mut offset = region.start;
        while offset < region.end {
            let wanted = usize::try_from((region.end - offset).min(CHUNK as u64)).unwrap();
            let read = match memory.read_at(&mut buffer[..wanted], offset) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if matches!(error.raw_os_error(), Some(5 | 14)) => break,
                Err(error) => return Err(error),
            };
            let alignment = usize::try_from((8 - (offset & 7)) & 7).unwrap();
            for index in (alignment..read.saturating_sub(7)).step_by(8) {
                let target = u64::from_le_bytes(buffer[index..index + 8].try_into().unwrap());
                if !address_may_match(target, min_target, max_target, &low_bits) {
                    continue;
                }
                let Some(locations) = targets.get(&target) else {
                    continue;
                };
                let pointer_address = offset + index as u64;
                let display_object_candidate =
                    pointer_address.checked_sub(0xb0).filter(|address| {
                        memory
                            .regions()
                            .iter()
                            .any(|region| region.supports_heap() && region.contains(*address))
                    });
                for &(term_index, match_index) in locations {
                    let found = &mut terms[term_index].matches[match_index];
                    if found.references.len() >= MAX_REFERENCES {
                        found.references_truncated = true;
                        continue;
                    }
                    found.references.push(TextReference {
                        pointer_address,
                        display_object_candidate,
                    });
                }
            }
            offset += read as u64;
        }
    }
    Ok(())
}

fn region_label(region: &ScanRange) -> String {
    if region.path.is_empty() {
        format!("{} anonymous", region.permissions)
    } else {
        format!("{} {}", region.permissions, region.path)
    }
}

fn context(bytes: &[u8], index: usize, length: usize) -> String {
    let start = index.saturating_sub(CONTEXT_BEFORE);
    let end = index
        .saturating_add(length)
        .saturating_add(CONTEXT_AFTER)
        .min(bytes.len());
    bytes[start..end]
        .iter()
        .map(|value| match value {
            b' '..=b'~' => char::from(*value),
            b'\n' | b'\r' | b'\t' => ' ',
            _ => '.',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_deduplicates_terms() {
        assert_eq!(
            validated_terms(&["NEO ERA".to_owned(), "NEO ERA".to_owned()]).unwrap(),
            vec!["NEO ERA".to_owned()]
        );
        assert!(validated_terms(&[]).is_err());
        assert!(validated_terms(&[String::new()]).is_err());
        assert!(validated_terms(&["x".repeat(MAX_TERM_BYTES + 1)]).is_err());
    }

    #[test]
    fn validates_display_text_shape() {
        let mut bytes = [0_u8; 0xb8];
        bytes[0x20..0x24].copy_from_slice(&80_u32.to_le_bytes());
        bytes[0x24..0x28].copy_from_slice(&80_u32.to_le_bytes());
        bytes[0xb0..0xb8].copy_from_slice(&0x1234_u64.to_le_bytes());
        assert_eq!(display_text_pointer(&bytes), Some(0x1234));
        bytes[0x24..0x28].copy_from_slice(&79_u32.to_le_bytes());
        assert_eq!(display_text_pointer(&bytes), None);
    }
}
