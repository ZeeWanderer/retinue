use super::*;

const MAX_ENTITIES: usize = 4096;
const MAX_UNIQUES: usize = 128;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entities {
    pub tree: u64,
    pub area_key: u32,
    pub count: usize,
    pub unique_monsters: Vec<UniqueMonster>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UniqueMonster {
    pub id: u32,
    pub address: u64,
    pub name: String,
    pub life: i32,
}

struct Node {
    address: u64,
    bytes: Vec<u8>,
    id: u32,
    entity: u64,
}

impl Reader<'_> {
    pub(super) fn entities(&self, frame: u64) -> Result<Entities, String> {
        let layout = self.layout.entities.as_ref().map_err(Clone::clone)?;
        for name in [
            "NPC",
            "Monster",
            "Life",
            "Render",
            "ObjectMagicProperties",
            "Positioned",
        ] {
            if !self
                .layout
                .component_getters
                .values()
                .any(|value| value == name)
            {
                return Err(format!("{name} component getter is unavailable"));
            }
        }
        let field = frame + u64::from(layout.tree_offset);
        let bounds = self.bytes(field, 16)?;
        let tree = self.valid_pointer(word(&bounds, 0))?;
        let count = word(&bounds, 8);
        if count > MAX_ENTITIES as u64 {
            return Err("entity tree exceeds observation limit".into());
        }
        let area_key = self.u32(frame + 0x114)?;
        let header = self.bytes(tree, 32)?;
        if header[0x19] != 1 || header[0x18] > 1 {
            return Err("invalid entity tree sentinel".into());
        }
        let mut pending = vec![(word(&header, 8), tree, None, None)];
        let mut visited = BTreeSet::new();
        let mut nodes = BTreeMap::new();
        while let Some((address, parent, lower, upper)) = pending.pop() {
            if address == tree {
                continue;
            }
            if visited.len() >= count as usize || !visited.insert(address) {
                return Err("entity tree cycle or count mismatch".into());
            }
            self.valid_pointer(address)?;
            let bytes = self.bytes(address, 48)?;
            let id = u32::from_le_bytes(bytes[0x20..0x24].try_into().unwrap());
            if bytes[0x19] != 0
                || bytes[0x18] > 1
                || word(&bytes, 8) != parent
                || lower.is_some_and(|value| id <= value)
                || upper.is_some_and(|value| id >= value)
            {
                return Err("inconsistent entity tree node".into());
            }
            let entity = self.valid_pointer(word(&bytes, 0x28))?;
            if self.u32(entity + u64::from(layout.id_offset))? != id {
                return Err("entity tree identity mismatch".into());
            }
            pending.push((word(&bytes, 0x10), address, Some(id), upper));
            pending.push((word(&bytes, 0), address, lower, Some(id)));
            nodes.insert(
                id,
                Node {
                    address,
                    bytes,
                    id,
                    entity,
                },
            );
        }
        if nodes.len() != count as usize
            || nodes.first_key_value().map_or(tree, |(_, n)| n.address) != word(&header, 0)
            || nodes.last_key_value().map_or(tree, |(_, n)| n.address) != word(&header, 0x10)
        {
            return Err("inconsistent entity tree bounds".into());
        }
        let local = self.vector(frame + 0x5b8, 32)?;
        let player = *local
            .first()
            .ok_or("local player is unavailable for entity validation")?;
        if !nodes.values().any(|node| node.entity == player) {
            return Err("local player is absent from entity tree".into());
        }
        let mut unique_monsters = Vec::new();
        for node in nodes.values().filter(|node| node.id & 0xc000_0000 == 0) {
            if let Some(monster) = self.unique_monster(node)? {
                if unique_monsters.len() >= MAX_UNIQUES {
                    return Err("unique monster count exceeds observation limit".into());
                }
                unique_monsters.push(monster);
            }
        }
        // A failed or torn traversal must not turn a missing entity into a death.
        for node in nodes.values() {
            if self.bytes(node.address, 48)? != node.bytes
                || self.u32(node.entity + u64::from(layout.id_offset))? != node.id
            {
                return Err("entity tree changed during observation".into());
            }
        }
        if self.bytes(field, 16)? != bounds
            || self.bytes(tree, 32)? != header
            || self.u32(frame + 0x114)? != area_key
            || self.vector(frame + 0x5b8, 32)? != local
        {
            return Err("entity context changed during observation".into());
        }
        Ok(Entities {
            tree,
            area_key,
            count: count as usize,
            unique_monsters,
        })
    }

    fn unique_monster(&self, node: &Node) -> Result<Option<UniqueMonster>, String> {
        let slot = self
            .component_slot
            .get()
            .ok_or("component-name getter slot is unknown")?;
        let pointers = self.vector(node.entity + 0x10, 128)?;
        let mut components = BTreeMap::new();
        for component in &pointers {
            let table = self.pointer(*component)?;
            let cached = self.component_tables.borrow().get(&table).cloned();
            let entries = match cached {
                Some(entries) => entries,
                None => {
                    let getter = self.u64(table + (slot * 8) as u64)?;
                    let name = getter
                        .checked_sub(self.base)
                        .and_then(|rva| u32::try_from(rva).ok())
                        .and_then(|rva| self.layout.component_getters.get(&rva))
                        .cloned();
                    let mut entries = vec![None; 15];
                    entries[slot] = name;
                    self.component_tables
                        .borrow_mut()
                        .insert(table, entries.clone());
                    entries
                }
            };
            let name = entries[slot]
                .as_ref()
                .ok_or_else(|| format!("unsupported component-name getter at 0x{table:x}"))?;
            if components.insert(name.clone(), *component).is_some() {
                return Err(format!("duplicate {name} entity components"));
            }
        }
        if self.vector(node.entity + 0x10, 128)? != pointers {
            return Err("entity components changed during observation".into());
        }
        if components.contains_key("NPC") || !components.contains_key("Monster") {
            return Ok(None);
        }
        let get = |name: &str| {
            components
                .get(name)
                .copied()
                .ok_or_else(|| format!("monster {name} component is missing"))
        };
        let rarity = self.u32(get("ObjectMagicProperties")? + 0x144)?;
        if rarity > 3 {
            return Err("invalid monster rarity; layout may have changed".into());
        }
        if rarity != 3 || self.byte(get("Positioned")? + 0x1e1)? & 0x7f != 0 {
            return Ok(None);
        }
        let life = self.u32(get("Life")? + 0x1e0)? as i32;
        if life < 0 {
            return Err("invalid monster life; layout may have changed".into());
        }
        let render = get("Render")? + 0x160;
        let header = self.bytes(render, 32)?;
        let name = self.wide_string(render)?;
        if name.len() > 512 || self.bytes(render, 32)? != header {
            return Err("invalid or changed monster name".into());
        }
        Ok(Some(UniqueMonster {
            id: node.id,
            address: node.entity,
            name,
            life,
        }))
    }
}

fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
