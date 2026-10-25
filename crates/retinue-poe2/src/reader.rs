use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use retinue_native::process::ProcessMemory;
use serde::{Deserialize, Serialize};

use crate::layout::Layout;

mod social;
pub use social::{Identity, PartyMember, Social};
mod entities;
pub use entities::{Entities, UniqueMonster};

const STATES: [&str; 12] = [
    "AreaLoadingState",
    "ChangePasswordState",
    "CreditsState",
    "EscapeState",
    "InGameState",
    "LoadingState",
    "LoginState",
    "PreGameState",
    "CreateCharacterState",
    "SelectCharacterState",
    "DeleteCharacterState",
    "WaitingState",
];

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Snapshot {
    pub scene: String,
    pub states: Vec<String>,
    pub player: Option<Player>,
    pub area: Option<Area>,
    pub panels: BTreeMap<String, Option<bool>>,
    pub social: Option<Social>,
    pub entities: Option<Entities>,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Player {
    pub name: String,
    pub class: Option<String>,
    pub level: u8,
    pub experience: u32,
    pub gold: Option<i32>,
    pub life: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Area {
    pub name: String,
    pub in_town: bool,
    pub change_key: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Issue {
    pub field: String,
    pub reason: String,
}

pub fn read(memory: &ProcessMemory, base: u64, layout: &Layout) -> Result<Snapshot, String> {
    layout.validate()?;
    let reader = Reader {
        memory,
        base,
        layout,
        component_tables: RefCell::new(BTreeMap::new()),
        component_slot: Cell::new(None),
    };
    let global = base
        .checked_add(layout.root_rva)
        .ok_or("image address overflow")?;
    let root = reader.pointer(global)?;
    let active = reader.vector(root + 0x28, 16)?;
    let templates = reader.bytes(root + 0x50, STATES.len() * 16)?;
    let mut known = BTreeMap::new();
    for (index, chunk) in templates.chunks_exact(16).enumerate() {
        let address = u64::from_le_bytes(chunk[..8].try_into().unwrap());
        if address != 0 {
            reader.valid_pointer(address)?;
            if known.insert(address, STATES[index]).is_some() {
                return Err("duplicate game-state template pointers".into());
            }
        }
    }
    let mut states = Vec::new();
    for address in &active {
        // Active storage can include shared-pointer control blocks, not only state objects.
        let Some(name) = known.get(address) else {
            continue;
        };
        if states.iter().any(|s| s == name) {
            return Err("duplicate active game state".into());
        }
        states.push((*name).to_owned());
    }
    if !active.is_empty() && states.is_empty() {
        return Err("unknown active game state; layout may have changed".into());
    }
    let mut snapshot = Snapshot {
        scene: scene(&states).into(),
        states,
        ..Snapshot::default()
    };
    let mut social_frame = None;
    if snapshot.states.iter().any(|state| state == "InGameState") {
        let ingame = reader.pointer(root + 0x90)?;
        let frame = reader.pointer(ingame + u64::from(layout.ingame_data_offset))?;
        snapshot.area = snapshot.field("area", reader.area(frame));
        if !snapshot.states.iter().any(|s| s == "AreaLoadingState") {
            snapshot.player = snapshot.field("player", reader.player(frame));
            if let Some(player) = snapshot.player.as_mut() {
                match reader.gold(frame) {
                    Ok(value) => player.gold = Some(value),
                    Err(reason) => snapshot.issues.push(Issue {
                        field: "gold".into(),
                        reason,
                    }),
                }
            }
            social_frame = Some((ingame, frame));
        }
        match reader.panels() {
            Ok(panels) => snapshot.panels = panels,
            Err(reason) => snapshot.issues.push(Issue {
                field: "panels".into(),
                reason,
            }),
        }
        if reader.pointer(ingame + u64::from(layout.ingame_data_offset))? != frame {
            return Err("in-game data changed during observation".into());
        }
    }
    if reader.pointer(global)? != root || reader.vector(root + 0x28, 16)? != active {
        return Err("game state changed during observation".into());
    }
    if let Some((ingame, frame)) = social_frame {
        let social = reader
            .social(frame, snapshot.player.as_ref())
            .and_then(|social| {
                if reader.pointer(global)? != root
                    || reader.vector(root + 0x28, 16)? != active
                    || reader.pointer(root + 0x90)? != ingame
                    || reader.pointer(ingame + u64::from(layout.ingame_data_offset))? != frame
                {
                    return Err("game state changed during social observation".into());
                }
                Ok(social)
            });
        snapshot.social = snapshot.field("social", social);
        if snapshot.player.is_some() {
            let entities = reader.entities(frame).and_then(|entities| {
                if reader.pointer(global)? != root
                    || reader.vector(root + 0x28, 16)? != active
                    || reader.pointer(root + 0x90)? != ingame
                    || reader.pointer(ingame + u64::from(layout.ingame_data_offset))? != frame
                {
                    return Err("game state changed during entity observation".into());
                }
                Ok(entities)
            });
            snapshot.entities = snapshot.field("entities", entities);
        }
    }
    Ok(snapshot)
}

impl Snapshot {
    fn field<T>(&mut self, field: &str, result: Result<T, String>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(reason) => {
                self.issues.push(Issue {
                    field: field.into(),
                    reason,
                });
                None
            }
        }
    }
}

fn scene(states: &[String]) -> &'static str {
    let has = |name| states.iter().any(|state| state == name);
    if has("EscapeState") {
        "escape_menu"
    } else if has("AreaLoadingState") {
        "loading"
    } else if has("InGameState") {
        "in_game"
    } else if has("SelectCharacterState") {
        "select_character"
    } else if has("LoginState") {
        "login"
    } else {
        "unknown"
    }
}

struct Reader<'a> {
    memory: &'a ProcessMemory,
    base: u64,
    layout: &'a Layout,
    component_tables: RefCell<BTreeMap<u64, Vec<Option<String>>>>,
    component_slot: Cell<Option<usize>>,
}

impl Reader<'_> {
    fn bytes(&self, address: u64, length: usize) -> Result<Vec<u8>, String> {
        if length > 8192 || !self.memory.supports_read_range(address, length) {
            return Err(format!("unreadable or oversized field at 0x{address:x}"));
        }
        let mut bytes = vec![0; length];
        self.memory
            .read_exact_at(&mut bytes, address)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    }

    fn u64(&self, address: u64) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.bytes(address, 8)?.try_into().unwrap(),
        ))
    }

    fn u32(&self, address: u64) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.bytes(address, 4)?.try_into().unwrap(),
        ))
    }

    fn byte(&self, address: u64) -> Result<u8, String> {
        Ok(self.bytes(address, 1)?[0])
    }

    fn valid_pointer(&self, address: u64) -> Result<u64, String> {
        // Sparse captures require the referenced fields, not every object's first bytes.
        if address < 65536
            || address > 0x0000_7fff_ffff_0000
            || !address.is_multiple_of(8)
            || !self
                .memory
                .mapped_region(address, 8)
                .is_some_and(|region| region.permissions.starts_with('r'))
        {
            return Err(format!("invalid native pointer 0x{address:x}"));
        }
        Ok(address)
    }

    fn pointer(&self, address: u64) -> Result<u64, String> {
        self.valid_pointer(self.u64(address)?)
    }

    fn vector(&self, address: u64, limit: usize) -> Result<Vec<u64>, String> {
        let bounds = self.bytes(address, 16)?;
        let begin = u64::from_le_bytes(bounds[..8].try_into().unwrap());
        let end = u64::from_le_bytes(bounds[8..].try_into().unwrap());
        let length = end
            .checked_sub(begin)
            .filter(|n| n.is_multiple_of(8) && *n <= (limit * 8) as u64)
            .ok_or("invalid native pointer vector")?;
        if length == 0 {
            return Ok(Vec::new());
        }
        self.valid_pointer(begin)?;
        self.bytes(begin, length as usize)?
            .chunks_exact(8)
            .map(|chunk| self.valid_pointer(u64::from_le_bytes(chunk.try_into().unwrap())))
            .collect()
    }

    fn wide(&self, address: u64, length: usize) -> Result<String, String> {
        if length == 0 || length > 512 {
            return Err("invalid native string length".into());
        }
        let bytes = self.bytes(address, length * 2)?;
        let units = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        if units.contains(&0) {
            return Err("embedded null in native string".into());
        }
        let text = String::from_utf16(&units).map_err(|e| format!("invalid native UTF-16: {e}"))?;
        if text.chars().any(char::is_control) {
            return Err("control character in native string".into());
        }
        Ok(text)
    }

    fn wide_z(&self, address: u64) -> Result<String, String> {
        if address < 65536 {
            return Err("null native string".into());
        }
        for length in 0..512 {
            if self.bytes(address + length * 2, 2)? == [0, 0] {
                return self.wide(address, length as usize);
            }
        }
        Err("unterminated native string".into())
    }

    fn wide_string(&self, object: u64) -> Result<String, String> {
        let length = self.u64(object + 16)?;
        let capacity = self.u64(object + 24)?;
        if length > capacity || capacity > 4096 {
            return Err("invalid native string capacity".into());
        }
        let address = if capacity <= 7 {
            object
        } else {
            self.pointer(object)?
        };
        self.wide(address, length as usize)
    }

    fn area(&self, frame: u64) -> Result<Area, String> {
        let zone = self.pointer(frame + 0x98)?;
        let name = self.wide_z(self.u64(zone + 8)?)?;
        if name.len() > 512 {
            return Err("area name exceeds limit".into());
        }
        let town = self.byte(zone + 0x14)?;
        Ok(Area {
            in_town: town == 1 || (town == 0 && name.contains("Hideout")),
            name,
            change_key: self.u32(frame + 0x114)?,
        })
    }

    fn components(&self, entity: u64) -> Result<BTreeMap<String, u64>, String> {
        let components = self.vector(entity + 0x10, 128)?;
        let mut tables = Vec::new();
        let mut slots = BTreeSet::new();
        for component in components {
            let vtable = self.pointer(component)?;
            let cached = self.component_tables.borrow().get(&vtable).cloned();
            let entries = match cached {
                Some(entries) => entries,
                None => {
                    let entries = self
                        .bytes(vtable, 15 * 8)?
                        .chunks_exact(8)
                        .map(|b| {
                            u64::from_le_bytes(b.try_into().unwrap())
                                .checked_sub(self.base)
                                .and_then(|rva| u32::try_from(rva).ok())
                                .and_then(|rva| self.layout.component_getters.get(&rva))
                                .cloned()
                        })
                        .collect::<Vec<_>>();
                    self.component_tables
                        .borrow_mut()
                        .insert(vtable, entries.clone());
                    entries
                }
            };
            for (index, name) in entries.iter().enumerate() {
                if name.as_deref() == Some("Stats") {
                    slots.insert(index);
                }
            }
            tables.push((component, entries));
        }
        if slots.len() != 1 {
            return Err("component-name getter missing or ambiguous".into());
        }
        let index = *slots.first().unwrap();
        if self.component_slot.get().is_some_and(|slot| slot != index) {
            return Err("inconsistent component-name getter slots".into());
        }
        self.component_slot.set(Some(index));
        let mut result = BTreeMap::new();
        for (component, entries) in tables {
            if let Some(name) = &entries[index]
                && result.insert(name.clone(), component).is_some()
            {
                return Err(format!("duplicate {name} components"));
            }
        }
        Ok(result)
    }

    fn player(&self, frame: u64) -> Result<Player, String> {
        let entities = self.vector(frame + 0x5b8, 32)?;
        let entity = *entities
            .first()
            .ok_or("local player entity is not available")?;
        let components = self.components(entity)?;
        let player = *components
            .get("Player")
            .ok_or("Player component is not available")?;
        let name = self.wide_string(player + 0x1b0)?;
        if name.len() > 256 {
            return Err("character name exceeds limit".into());
        }
        let level = self.byte(player + 0x204)?;
        if !(1..=100).contains(&level) {
            return Err("invalid character level; layout may have changed".into());
        }
        let class = self
            .pointer(player + 0x1a8)
            .and_then(|p| self.u64(p + 0x10))
            .and_then(|p| self.wide_z(p))
            .ok()
            .filter(|name| name.len() <= 128);
        let life = components
            .get("Life")
            .map(|component| self.u32(component + 0x1e0))
            .transpose()?
            .map(|raw| raw as i32);
        if life.is_some_and(|value| !(0..100_000_000).contains(&value)) {
            return Err("invalid character life; layout may have changed".into());
        }
        Ok(Player {
            name,
            class,
            level,
            experience: self.u32(player + 0x1d8)?,
            gold: None,
            life,
        })
    }

    fn gold(&self, frame: u64) -> Result<i32, String> {
        let layout = self.layout.gold.as_ref().map_err(Clone::clone)?;
        let server = self.pointer(frame + u64::from(layout.server_offset))?;
        let inventory = self.pointer(server + 0x60)?;
        let data = self.pointer(inventory + u64::from(layout.data_offset))?;
        Ok(self.u32(data + u64::from(layout.value_offset))? as i32)
    }

    fn panels(&self) -> Result<BTreeMap<String, Option<bool>>, String> {
        let layout = self.layout.panels.as_ref().map_err(Clone::clone)?;
        let root = self.pointer(self.base + layout.root_rva + 8)?;
        let ui = self.pointer(root + 8)?;
        let mut panels = BTreeMap::new();
        for (name, offset) in [
            ("passive_tree", 0x718),
            ("stash", 0x6f8),
            ("inventory", 0x6f0),
            ("character_sheet", 0x728),
            ("market", 0xa60),
        ] {
            let panel = self.u64(ui + offset)?;
            let visible = if panel == 0 {
                None
            } else {
                self.valid_pointer(panel)?;
                Some(self.u32(panel + u64::from(layout.flags_offset))? & 0x800 != 0)
            };
            panels.insert(name.into(), visible);
        }
        Ok(panels)
    }
}

#[cfg(test)]
mod tests;
