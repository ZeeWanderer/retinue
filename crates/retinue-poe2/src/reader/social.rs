use super::{Player, Reader};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Social {
    pub local: Option<Identity>,
    pub party: Vec<PartyMember>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Identity {
    pub character: String,
    pub account: String,
    pub league: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PartyMember {
    pub character: String,
    pub account: String,
    pub league: Option<String>,
    pub class_id: Option<u8>,
    pub level: Option<u8>,
}

impl Reader<'_> {
    pub(super) fn social(&self, frame: u64, local: Option<&Player>) -> Result<Social, String> {
        let layout = self.layout.social.as_ref().map_err(Clone::clone)?;
        let server = self.pointer(frame + u64::from(layout.server_offset))?;
        let party_at = server + u64::from(layout.party_offset);
        let records_at = server + u64::from(layout.records_offset);
        let party = self.record_vector(party_at, 0x50, 64)?;
        let records = self.record_vector(records_at, 0x210, 0x512)?;
        let mut members = Vec::new();
        let mut wanted = BTreeSet::new();
        for address in (party.0..party.1).step_by(0x50) {
            let account = self.social_string(address + 8)?;
            let character = self.social_string(address + 0x28)?;
            if account.is_empty() || character.is_empty() {
                return Err("empty party identity".into());
            }
            if !wanted.insert(character.clone()) {
                return Err("duplicate party character".into());
            }
            members.push(PartyMember {
                character,
                account,
                league: None,
                class_id: None,
                level: None,
            });
        }
        if let Some(local) = local {
            wanted.insert(local.name.clone());
        }
        let mut details = BTreeMap::new();
        for address in (records.0..records.1).step_by(0x210) {
            let online = self.byte(address + 0x202)?;
            if online > 1 {
                return Err("invalid social online flag; layout may have changed".into());
            }
            if online == 0 || wanted.is_empty() {
                continue;
            }
            let character = self.social_string(address + 0x40)?;
            if !wanted.contains(&character) {
                continue;
            }
            let account = self.social_string(address)?;
            let league = self.social_string(address + 0x60)?;
            let class_id = self.byte(address + 0x200)?;
            let level = self.byte(address + 0x201)?;
            if account.is_empty() || !(1..=100).contains(&level) {
                return Err("invalid social character identity".into());
            }
            let detail = PartyMember {
                character: character.clone(),
                account,
                league: (!league.is_empty()).then_some(league),
                class_id: (class_id != 255).then_some(class_id),
                level: Some(level),
            };
            if details.insert(character, detail).is_some() {
                return Err("ambiguous social character identity".into());
            }
        }
        for member in &mut members {
            if let Some(detail) = details.get(&member.character) {
                if member.account != detail.account {
                    return Err("party account changed during observation".into());
                }
                *member = detail.clone();
            }
        }
        let local = local.and_then(|player| {
            let detail = details.get(&player.name)?;
            let league = detail.league.as_ref()?;
            (detail.level == Some(player.level)).then(|| Identity {
                character: player.name.clone(),
                account: detail.account.clone(),
                league: league.clone(),
            })
        });
        if self.record_vector(party_at, 0x50, 64)? != party
            || self.record_vector(records_at, 0x210, 0x512)? != records
            || self.pointer(frame + u64::from(layout.server_offset))? != server
        {
            return Err("social vectors changed during observation".into());
        }
        Ok(Social {
            local,
            party: members,
        })
    }

    fn social_string(&self, address: u64) -> Result<String, String> {
        let header = self.bytes(address, 32)?;
        let length = u64::from_le_bytes(header[16..24].try_into().unwrap());
        let capacity = u64::from_le_bytes(header[24..].try_into().unwrap());
        let text = if length == 0 && capacity <= 4096 {
            String::new()
        } else {
            self.wide_string(address)?
        };
        if text.len() > 256 || self.bytes(address, 32)? != header {
            return Err("invalid or changing social string".into());
        }
        Ok(text)
    }

    fn record_vector(
        &self,
        address: u64,
        stride: u64,
        limit: u64,
    ) -> Result<(u64, u64, u64), String> {
        let bytes = self.bytes(address, 24)?;
        let begin = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let capacity = u64::from_le_bytes(bytes[16..].try_into().unwrap());
        if begin == 0 && end == 0 && capacity == 0 {
            return Ok((0, 0, 0));
        }
        self.valid_pointer(begin)?;
        let length = end.checked_sub(begin).ok_or("reversed social vector")?;
        let reserved = capacity
            .checked_sub(begin)
            .ok_or("invalid social vector capacity")?;
        if length > reserved
            || length > stride * limit
            || reserved > stride * limit * 2
            || !length.is_multiple_of(stride)
            || !reserved.is_multiple_of(stride)
        {
            return Err("invalid or oversized social vector".into());
        }
        Ok((begin, end, capacity))
    }
}
