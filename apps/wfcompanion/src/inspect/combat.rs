mod layout;
mod tracker;

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::memory::{self, ByteScanReport, OpenedSource, Source};
use super::query::Selection;
use super::watch::{self, Journal, Options};
use crate::game_observer::memory::ProcessMemory;
use layout::Layout;
use tracker::Tracker;

const SLOTS: usize = 55;
const MODULUS: u16 = 1000;

pub struct ProbeOptions {
    pub owner: Option<u64>,
    pub selection: Selection,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Record {
    slot: usize,
    address: u64,
    sequence: u16,
    text: String,
    displayed_amount: Option<u64>,
    glyph_tokens: Vec<String>,
    rgb: u32,
    critical_tier: i32,
    remaining_seconds: f32,
    retired: bool,
    relative_anchor: bool,
    anchor_handle: u64,
    flags_hex: String,
    raw_hex: String,
}

impl Record {
    fn same_value(&self, other: &Self) -> bool {
        self.slot == other.slot
            && self.address == other.address
            && self.sequence == other.sequence
            && self.text == other.text
            && self.rgb == other.rgb
            && self.critical_tier == other.critical_tier
            && self.relative_anchor == other.relative_anchor
            && self.anchor_handle == other.anchor_handle
            && self.flags_hex == other.flags_hex
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    native: u64,
    counter: u16,
    mode: u64,
    records: Vec<Record>,
}

impl Snapshot {
    fn same_values(&self, other: &Self) -> bool {
        self.native == other.native
            && self.counter == other.counter
            && self.mode == other.mode
            && self.records.len() == other.records.len()
            && self
                .records
                .iter()
                .zip(&other.records)
                .all(|(a, b)| a.same_value(b))
    }

    fn validate(&self) -> Result<(), String> {
        if self.counter >= MODULUS || !matches!(self.mode, 1 | 3) || self.records.len() != SLOTS {
            return Err("invalid damage-display snapshot".into());
        }
        for (index, record) in self.records.iter().enumerate() {
            if record.slot != index + 1
                || record.sequence >= MODULUS
                || !record.remaining_seconds.is_finite()
                || record.text.len() > 256
                || record.raw_hex.len() != 0x190 * 2
            {
                return Err("invalid damage-display record".into());
            }
        }
        Ok(())
    }
}

struct Session {
    source: OpenedSource,
    base: u64,
    layout: Layout,
    owner: u64,
    discovery: Option<ByteScanReport>,
    baseline: Snapshot,
}

impl Session {
    fn open(source: Source, options: ProbeOptions) -> Result<Self, String> {
        let source = OpenedSource::open(source)?;
        let layout = layout::read(&source)?;
        let base = source
            .memory
            .image_base()
            .ok_or("game image base unavailable")?;
        if [
            layout.hud_vtable_rva,
            layout.native_vtable_rva,
            layout.flash_vtable_rva,
            layout.mode_rva,
        ]
        .iter()
        .any(|&rva| {
            base.checked_add(rva)
                .and_then(|v| v.checked_add(8))
                .is_none()
        }) {
            return Err("invalid game image base".into());
        }
        let discovery = if options.owner.is_none() {
            Some(memory::find_bytes(
                &source,
                &(base + layout.hud_vtable_rva).to_le_bytes(),
                &options.selection,
                1024,
                options.max_bytes,
            )?)
        } else {
            None
        };
        let candidates = options.owner.map(|owner| vec![owner]).unwrap_or_else(|| {
            discovery
                .as_ref()
                .unwrap()
                .matches
                .iter()
                .map(|hit| hit.address)
                .collect()
        });
        let mut valid = Vec::new();
        let mut first_error = None;
        for owner in candidates {
            match snapshot(&source.memory, base, &layout, owner) {
                Ok(Some(baseline)) => valid.push((owner, baseline)),
                Ok(None) => first_error = Some("pool changed during discovery; retry".to_owned()),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if valid.len() != 1 {
            let coverage = discovery
                .as_ref()
                .map(|v| {
                    format!(
                        "scanned {} of {} bytes; incomplete={}; ",
                        v.searched_bytes, v.candidate_bytes, v.truncated
                    )
                })
                .unwrap_or_default();
            return Err(format!(
                "expected one validated HUD pool, found {}; {coverage}{}",
                valid.len(),
                first_error.unwrap_or_else(|| {
                    "use --range/--max-bytes or --owner to refine discovery".into()
                })
            ));
        }
        let (owner, baseline) = valid.pop().unwrap();
        Ok(Self {
            source,
            base,
            layout,
            owner,
            discovery,
            baseline,
        })
    }
}

pub fn probe(source: Source, options: ProbeOptions) -> Result<Value, String> {
    let session = Session::open(source, options)?;
    Ok(json!({
        "source": session.source.metadata, "owner": session.owner, "native": session.baseline.native,
        "counter": session.baseline.counter, "slots": session.baseline.records.len(),
        "mode": session.baseline.mode, "discovery": session.discovery,
        "layout": session.layout,
        "semantics": "HUD display pool; anchors are not authoritative target identities",
    }))
}

pub fn record(
    source: Source,
    probe: ProbeOptions,
    output: &Path,
    options: Options,
    mut emit: impl FnMut(&Value) -> Result<(), String>,
) -> Result<Value, String> {
    options.validate()?;
    let session = Session::open(source, probe)?;
    let mut journal = Journal::create(
        output,
        "wfinspect.combat-watch",
        json!({
            "memory": session.source.metadata, "owner": session.owner, "image_base": session.base,
            "discovery": session.discovery, "baseline": session.baseline,
            "layout": session.layout,
            "semantics": "HUD observations, not lossless hit packets; a full 1000-event wrap is undetectable; anchors are not target IDs",
        }),
        options,
    )?;
    let mut tracker = Tracker::new(session.baseline.clone());
    let mut previous = session.baseline.clone();
    let mut settle = false;
    let mut unstable = 0_u64;
    eprintln!(
        "READY owner=0x{:x} counter={} output={}",
        session.owner,
        previous.counter,
        output.display()
    );
    let summary = watch::poll(options, &mut journal, |index, journal| {
        let Some(current) = snapshot(
            &session.source.memory,
            session.base,
            &session.layout,
            session.owner,
        )?
        else {
            unstable += 1;
            journal.append("unstable_sample", &json!({"index": index}))?;
            return Ok(());
        };
        if current.native != session.baseline.native || current.mode != session.baseline.mode {
            return Err("HUD owner or display mode changed; start a new recording".into());
        }
        let changed = !current.same_values(&previous);
        if (changed || settle)
            && !journal.append("sample", &json!({"index": index, "snapshot": current}))?
        {
            return Ok(());
        }
        for event in tracker.update(current.clone()) {
            emit(&event)?;
        }
        settle = changed;
        previous = current;
        Ok(())
    })?;
    let report = json!({"polling": summary, "events": tracker.counts,
        "unstable_samples": unstable, "unsettled": tracker.pending(), "path": output});
    journal.finish(&report)?;
    Ok(report)
}

pub fn replay(
    path: &Path,
    mut emit: impl FnMut(&Value) -> Result<(), String>,
) -> Result<Value, String> {
    let mut tracker: Option<Tracker> = None;
    let mut previous_index = None;
    let mut native = 0;
    let mut mode = 0;
    let mut header = None;
    let mut summary = None;
    let complete = watch::records(path, |record| {
        match record["kind"].as_str() {
            Some("start") if tracker.is_none() => {
                let data = &record["data"];
                if data["schema"] != "wfinspect.combat-watch"
                    || data["version"] != 1
                    || data["source"]
                        .get("layout")
                        .is_some_and(|value| value["schema"] != layout::SCHEMA)
                {
                    return Err("unsupported combat recording".into());
                }
                let baseline: Snapshot = serde_json::from_value(data["source"]["baseline"].clone())
                    .map_err(|e| e.to_string())?;
                baseline.validate()?;
                native = baseline.native;
                mode = baseline.mode;
                tracker = Some(Tracker::new(baseline));
                // Legacy v1 journals already contain decoded snapshots. Replaying
                // them does not discover bindings or read executable/process memory.
                header = Some(json!({"memory": data["source"]["memory"],
                    "owner": data["source"]["owner"], "layout": data["source"]["layout"]}));
            }
            Some("sample") if tracker.is_some() => {
                let index = record["data"]["index"]
                    .as_u64()
                    .ok_or("missing sample index")?;
                if previous_index.is_some_and(|prev| index <= prev) {
                    return Err("sample indices are not increasing".into());
                }
                previous_index = Some(index);
                let snapshot: Snapshot = serde_json::from_value(record["data"]["snapshot"].clone())
                    .map_err(|e| e.to_string())?;
                snapshot.validate()?;
                if snapshot.native != native || snapshot.mode != mode {
                    return Err("recording changes HUD identity or mode".into());
                }
                for event in tracker.as_mut().unwrap().update(snapshot) {
                    emit(
                        &json!({"unix_ms": record["unix_ms"], "elapsed_us": record["elapsed_us"], "event": event}),
                    )?;
                }
            }
            Some("sampling_gap" | "unstable_sample") if tracker.is_some() => {}
            Some("summary") if tracker.is_some() => summary = Some(record["data"].clone()),
            _ => return Err("unexpected combat recording record".into()),
        }
        Ok(())
    })?;
    let tracker = tracker.ok_or("recording has no baseline")?;
    Ok(json!({"path": path, "complete": complete, "source": header,
        "events": tracker.counts, "unsettled": tracker.pending(), "recorded_summary": summary}))
}

fn bytes(memory: &ProcessMemory, address: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 || length > 65536 || !memory.supports_read_range(address, length) {
        return Err(format!(
            "invalid or unavailable memory range 0x{address:x}+{length}"
        ));
    }
    let mut data = vec![0; length];
    memory
        .read_exact_at(&mut data, address)
        .map_err(|e| format!("read 0x{address:x}: {e}"))?;
    Ok(data)
}

fn u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}
fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}
fn pointer(memory: &ProcessMemory, address: u64) -> Result<u64, String> {
    Ok(u64_at(&bytes(memory, address, 8)?, 0))
}

fn string(memory: &ProcessMemory, data: &[u8], offset: usize) -> Result<String, String> {
    let local = &data[offset..offset + 16];
    let external;
    let value = if local[15] == 255 {
        let length = (u32_at(local, 8) & 0x0fff_ffff) as usize;
        if length > 256 {
            return Err("damage text exceeds 256 bytes".into());
        }
        if length == 0 {
            return Ok(String::new());
        }
        external = bytes(memory, u64_at(local, 0), length)?;
        &external[..]
    } else {
        local
    };
    let end = value
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(value.len());
    std::str::from_utf8(&value[..end])
        .map(str::to_owned)
        .map_err(|e| e.to_string())
}

fn decode_record(memory: &ProcessMemory, slot: usize, address: u64) -> Result<Record, String> {
    let raw = bytes(memory, address, 0x190)?;
    if string(memory, &raw, 0x170)? != format!("TargetHit{slot}.Label") {
        return Err(format!("damage slot {slot} changed identity"));
    }
    let text = string(memory, &raw, 0x30)?;
    let (number, tokens) = display_text(&text);
    let record = Record {
        slot,
        address,
        sequence: u16_at(&raw, 0x16c),
        text,
        displayed_amount: number,
        glyph_tokens: tokens,
        rgb: u32_at(&raw, 0x40),
        critical_tier: u32_at(&raw, 0x4c) as i32,
        remaining_seconds: f32::from_bits(u32_at(&raw, 0x1c)),
        retired: raw[0x16e] != 0,
        relative_anchor: raw[0x63] != 0,
        anchor_handle: u64_at(&raw, 0x160),
        flags_hex: watch::hex(&[raw[0x48], raw[0x49], raw[0x4a], raw[0x50]]),
        raw_hex: watch::hex(&raw),
    };
    if record.sequence >= MODULUS || !record.remaining_seconds.is_finite() {
        return Err("invalid damage record sequence or lifetime".into());
    }
    Ok(record)
}

fn display_text(text: &str) -> (Option<u64>, Vec<String>) {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let mut tail = &text[digits..];
    let mut tokens = Vec::new();
    while let Some(tag) = tail.strip_prefix('<') {
        let Some((token, rest)) = tag.split_once('>') else {
            break;
        };
        tokens.push(token.to_owned());
        tail = rest;
    }
    let number = if tail.is_empty() {
        text[..digits].parse().ok()
    } else {
        None
    };
    (number, tokens)
}

fn snapshot(
    memory: &ProcessMemory,
    base: u64,
    layout: &Layout,
    owner: u64,
) -> Result<Option<Snapshot>, String> {
    let header = bytes(memory, owner, 0x3b8)?;
    if u64_at(&header, 0) != base + layout.hud_vtable_rva {
        return Err("HUD owner type changed".into());
    }
    let native = pointer(memory, u64_at(&header, 0x28))?;
    let flash = pointer(memory, u64_at(&header, 0x30))?;
    if native == 0 || flash == 0 {
        return Err("combat HUD is inactive; enter a mission or Simulacrum".into());
    }
    if pointer(memory, flash)? != base + layout.flash_vtable_rva {
        return Err("HUD movie type changed".into());
    }
    let state = bytes(memory, native, 0xc40)?;
    if u64_at(&state, 0) != base + layout.native_vtable_rva {
        return Err("native HUD type changed".into());
    }
    let mode = pointer(memory, base + layout.mode_rva)?;
    if !matches!(mode, 1 | 3) {
        return Err(format!("modern damage display required; mode={mode}"));
    }
    let count = u32_at(&header, 0x3b0) as usize;
    let capacity = u32_at(&header, 0x3b4) as usize;
    let native_capacity = u32_at(&state, 0xc3c) as usize;
    if count != SLOTS * 8
        || !(count..=128 * 8).contains(&capacity)
        || u32_at(&state, 0xc38) as usize != count
        || !(count..=128 * 8).contains(&native_capacity)
    {
        return Err("unexpected damage pool bounds".into());
    }
    let handles = bytes(memory, u64_at(&header, 0x3a8), count)?;
    if bytes(memory, u64_at(&state, 0xc30), count)? != handles {
        return Err("HUD pools disagree".into());
    }
    let counter = u16_at(&state, 0x2d8);
    let records = handles
        .chunks_exact(8)
        .enumerate()
        .map(|(index, handle)| {
            decode_record(memory, index + 1, pointer(memory, u64_at(handle, 0))?)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let after = bytes(memory, owner, 0x3b8)?;
    if header[0x28..0x38] != after[0x28..0x38]
        || header[0x3a8..] != after[0x3a8..]
        || pointer(memory, u64_at(&header, 0x28))? != native
        || u16_at(&bytes(memory, native + 0x2d8, 2)?, 0) != counter
    {
        return Ok(None);
    }
    let snapshot = Snapshot {
        native,
        counter,
        mode,
        records,
    };
    snapshot.validate()?;
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests;
