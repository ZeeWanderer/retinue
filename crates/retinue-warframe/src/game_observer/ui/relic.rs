use std::collections::BTreeSet;

use serde::Serialize;

use super::display;
use super::registry;
use super::{DisplayScanMetrics, Snapshot};
use crate::game_observer::adapter::ScaleformLayout;
use crate::game_observer::memory::ProcessMemory;

const SELECTION_MOVIE: &str = "/Lotus/Interface/ThemedProjectionManager.swf";
const REWARD_MOVIE: &str = "/Lotus/Interface/ProjectionRewardChoice.swf";
const FLASH_RECORD_OFFSET: u64 = 0xc0;
const REWARD_INSTANCE_NAME: &[u8] = b"ItemName";
const MAX_REWARD_NAME_BYTES: usize = 160;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RelicEra {
    Lith,
    Meso,
    Neo,
    Axi,
    All,
    Requiem,
}

impl RelicEra {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lith => "lith",
            Self::Meso => "meso",
            Self::Neo => "neo",
            Self::Axi => "axi",
            Self::All => "all",
            Self::Requiem => "requiem",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct RelicSelection {
    pub era: RelicEra,
    pub movie_record: u64,
    pub scan: DisplayScanMetrics,
}

#[derive(Clone, Debug, Serialize)]
pub struct RelicRewardText {
    pub names: Vec<String>,
    pub movie_record: u64,
    pub scan: DisplayScanMetrics,
}

pub(super) fn selection(pid: u32, layout: ScaleformLayout) -> Result<RelicSelection, String> {
    let memory = crate::game_observer::memory::open(pid)?;
    let scan =
        registry::scan(&memory, layout).map_err(|(stage, reason)| format!("{stage}: {reason}"))?;
    selection_from_snapshot(&memory, layout, &scan.snapshot)
}

pub(super) fn selection_from_snapshot(
    memory: &ProcessMemory,
    layout: ScaleformLayout,
    snapshot: &Snapshot,
) -> Result<RelicSelection, String> {
    let movies = snapshot
        .movies
        .iter()
        .filter(|movie| movie.path == SELECTION_MOVIE)
        .collect::<Vec<_>>();
    if movies.is_empty() {
        return Err("selection_movie: not loaded".to_owned());
    }
    let image = memory
        .image_base()
        .ok_or_else(|| "selection_state: Warframe image base not found".to_owned())?;
    let labels = [
        b"EQUIP FOR MISSION".as_slice(),
        b"LITH ERA".as_slice(),
        b"MESO ERA".as_slice(),
        b"NEO ERA".as_slice(),
        b"AXI ERA".as_slice(),
        b"REQUIEM".as_slice(),
    ];
    let mut failures = Vec::new();
    for movie in movies {
        let Some(flash_object) = movie.record_address.checked_sub(FLASH_RECORD_OFFSET) else {
            failures.push("invalid movie record".to_owned());
            continue;
        };
        match display::scan_labels(memory, layout, image, flash_object, &labels).and_then(
            |(matches, metrics)| classify_selection_labels(&matches).map(|era| (era, metrics)),
        ) {
            Ok((era, metrics)) => {
                return Ok(RelicSelection {
                    era,
                    movie_record: movie.record_address,
                    scan: metrics,
                });
            }
            Err(error) => failures.push(error),
        }
    }
    Err(format!("selection_state: {}", failures.join("; ")))
}

pub(super) fn rewards(pid: u32, layout: ScaleformLayout) -> Result<RelicRewardText, String> {
    let memory = crate::game_observer::memory::open(pid)?;
    let scan =
        registry::scan(&memory, layout).map_err(|(stage, reason)| format!("{stage}: {reason}"))?;
    rewards_from_snapshot(&memory, layout, &scan.snapshot)
}

pub(super) fn rewards_from_snapshot(
    memory: &ProcessMemory,
    layout: ScaleformLayout,
    snapshot: &Snapshot,
) -> Result<RelicRewardText, String> {
    let movies = snapshot
        .movies
        .iter()
        .filter(|movie| movie.path == REWARD_MOVIE)
        .collect::<Vec<_>>();
    if movies.is_empty() {
        return Err("reward_movie: not loaded".to_owned());
    }
    let image = memory
        .image_base()
        .ok_or_else(|| "reward_text: Warframe image base not found".to_owned())?;
    let mut failures = Vec::new();
    for movie in movies {
        let Some(flash_object) = movie.record_address.checked_sub(FLASH_RECORD_OFFSET) else {
            failures.push("invalid movie record".to_owned());
            continue;
        };
        match display::scan_named_text(
            memory,
            layout,
            image,
            flash_object,
            REWARD_INSTANCE_NAME,
            MAX_REWARD_NAME_BYTES,
        ) {
            Ok((values, metrics)) => match reward_names(values) {
                Ok(names) => {
                    return Ok(RelicRewardText {
                        names,
                        movie_record: movie.record_address,
                        scan: metrics,
                    });
                }
                Err(error) => failures.push(error),
            },
            Err(error) => failures.push(error),
        }
    }
    Err(format!("reward_text: {}", failures.join("; ")))
}

fn classify_selection_labels(matches: &BTreeSet<usize>) -> Result<RelicEra, String> {
    if !matches.contains(&0) {
        return Err("missing EQUIP FOR MISSION marker".to_owned());
    }
    let normal = [
        (RelicEra::Lith, 1),
        (RelicEra::Meso, 2),
        (RelicEra::Neo, 3),
        (RelicEra::Axi, 4),
    ]
    .into_iter()
    .filter_map(|(era, index)| matches.contains(&index).then_some(era))
    .collect::<Vec<_>>();
    classify_labels(normal.as_slice(), matches.contains(&5))
}

fn classify_labels(normal: &[RelicEra], requiem: bool) -> Result<RelicEra, String> {
    match normal {
        [era] => Ok(*era),
        [RelicEra::Lith, RelicEra::Meso, RelicEra::Neo, RelicEra::Axi] => Ok(RelicEra::All),
        [] if requiem => Ok(RelicEra::Requiem),
        _ => Err(format!("ambiguous era labels: {normal:?}")),
    }
}

fn plausible_reward_name(name: &str) -> bool {
    (3..=96).contains(&name.len())
        && !name.trim_end_matches('.').eq_ignore_ascii_case("loading")
        && name.bytes().any(|byte| byte.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|byte| matches!(byte, b' '..=b'~') && byte != b'<' && byte != b'>')
}

fn reward_names(values: Vec<String>) -> Result<Vec<String>, String> {
    if !(1..=4).contains(&values.len()) {
        return Err(format!(
            "expected 1..4 ItemName values, found {}",
            values.len()
        ));
    }
    let names = values
        .into_iter()
        .map(|name| name.trim().to_owned())
        .collect::<Vec<_>>();
    if names.iter().any(|name| !plausible_reward_name(name)) {
        return Err("ItemName labels are not ready".into());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_daemon_era_names() {
        assert_eq!(RelicEra::Lith.as_str(), "lith");
        assert_eq!(RelicEra::All.as_str(), "all");
        assert_eq!(RelicEra::Requiem.as_str(), "requiem");
    }

    #[test]
    fn rejects_markup_as_reward_name() {
        assert!(plausible_reward_name("2 X Forma Blueprint"));
        assert!(!plausible_reward_name("<font>Forma Blueprint</font>"));
    }

    #[test]
    fn rejects_loading_and_partial_reward_sets_without_dropping_slots() {
        assert!(reward_names(vec!["Loading...".into(); 4]).is_err());
        for incomplete in [
            "Loading...",
            "LOADING",
            " Loading... ",
            "",
            "...",
            "<b>Forma Blueprint</b>",
        ] {
            assert!(
                reward_names(vec![
                    "Forma Blueprint".into(),
                    incomplete.into(),
                    "Burston Prime Stock".into(),
                ])
                .is_err(),
                "{incomplete}"
            );
        }
        assert!(reward_names(Vec::new()).is_err());
        assert!(reward_names(vec!["Forma Blueprint".into(); 5]).is_err());
    }

    #[test]
    fn ready_rewards_preserve_count_order_duplicates_and_unknown_items() {
        let names = [
            "Forma Blueprint",
            "2 X Forma Blueprint",
            "Forma Blueprint",
            "New Item Blueprint",
        ];
        for count in 1..=4 {
            let values = names[..count]
                .iter()
                .map(|name| format!(" {name} "))
                .collect();
            assert_eq!(reward_names(values).unwrap(), names[..count]);
        }
    }

    #[test]
    fn classifies_known_relic_selection_shapes() {
        assert_eq!(classify_labels(&[RelicEra::Neo], false), Ok(RelicEra::Neo));
        assert_eq!(
            classify_labels(
                &[RelicEra::Lith, RelicEra::Meso, RelicEra::Neo, RelicEra::Axi,],
                false,
            ),
            Ok(RelicEra::All)
        );
        assert_eq!(classify_labels(&[], true), Ok(RelicEra::Requiem));
        assert!(classify_labels(&[RelicEra::Lith, RelicEra::Meso], false).is_err());
    }
}
