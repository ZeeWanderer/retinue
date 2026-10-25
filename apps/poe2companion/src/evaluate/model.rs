use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Choice {
    pub id: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Property {
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Modifier {
    pub key: String,
    pub text: String,
    pub section: String,
    pub id: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub supported: bool,
    pub resolution: Option<String>,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub quick_selected: bool,
    #[serde(default)]
    pub exact_selected: bool,
    #[serde(default)]
    pub base_selected: bool,
    #[serde(default)]
    pub ranges: ProfileRanges,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProfileRanges {
    pub allmods: Range,
    pub fuzzy: Range,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Condition {
    pub id: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Item {
    pub name: String,
    pub base: String,
    pub rarity: String,
    pub category: Option<String>,
    pub ranking_status: Option<String>,
    #[serde(default)]
    pub base_required: bool,
    #[serde(default)]
    pub auto_search: bool,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    pub default_profile: Option<Profile>,
    pub trade_unavailable: Option<String>,
    pub item_level: Option<u32>,
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub corrupted: bool,
    #[serde(default)]
    pub unidentified: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct League {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Evaluation {
    pub item: Item,
    pub leagues: Vec<League>,
    pub league: String,
    #[serde(default)]
    pub search_options: BTreeMap<String, Vec<Choice>>,
    #[serde(default)]
    pub prediction_available: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Estimate {
    pub amount: f64,
    pub currency: String,
    pub uncertainty: Option<Uncertainty>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Uncertainty {
    pub low: f64,
    pub high: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Listing {
    pub id: String,
    pub name: String,
    pub base: String,
    pub item_level: Option<u32>,
    pub gem_level: Option<u32>,
    pub required_level: Option<u32>,
    pub gem_sockets: Option<u32>,
    pub item: Option<Item>,
    #[serde(default, rename = "type")]
    pub trade_type: String,
    #[serde(default)]
    pub quality: u32,
    pub indexed_at: Option<u64>,
    pub amount: f64,
    pub currency: String,
    pub account: String,
    pub online: bool,
    pub indexed: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Results {
    pub search_id: String,
    pub url: String,
    pub total: u64,
    #[serde(default)]
    pub returned: u64,
    pub listings: Vec<Listing>,
    #[serde(default)]
    pub next_offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Restoration {
    pub results: Option<Results>,
    pub auto_search_allowed: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListingPage {
    pub search_id: String,
    pub offset: u32,
    pub next_offset: Option<u32>,
    pub listings: Vec<Listing>,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub enabled: bool,
    pub min: String,
    pub max: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Profile {
    #[serde(rename = "bases")]
    Base,
    #[serde(rename = "nomods")]
    Quick,
    #[serde(rename = "allmods")]
    Exact,
    #[serde(rename = "fuzzy")]
    Broad,
}

impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "Crafting Base",
            Self::Quick => "Quick Price",
            Self::Exact => "Exact Match",
            Self::Broad => "Broad (-10%)",
        }
    }
}

#[derive(Default)]
pub struct Model {
    pub budget: super::budget::Budget,
    pub visible: bool,
    pub generation: u64,
    pub copying: bool,
    pub loading: bool,
    pub text: String,
    pub evaluation: Option<Evaluation>,
    pub filters: Vec<Filter>,
    pub profile: Option<Profile>,
    pub conditions: Vec<bool>,
    pub league: String,
    pub status: String,
    character: Option<String>,
    character_league: Option<String>,
    league_character: Option<String>,
    pub rarity: Option<String>,
    pub currency: Option<String>,
    pub indexed: Option<String>,
    pub descending: bool,
    pub level: Filter,
    pub base: bool,
    pub results: Option<Results>,
    last_auto_attempt: Option<String>,
    pub error: Option<String>,
    pub fetching: bool,
    pub fetch_error: Option<String>,
    pub retry_at: Option<std::time::Instant>,
    cooldown_display: u64,
    pub prediction_generation: u64,
    pub predicting: bool,
    pub estimate: Option<Estimate>,
}

#[derive(Default)]
pub struct Presentation {
    pub game_active: bool,
    pub keyboard: bool,
}

impl Presentation {
    pub fn visible(&self, model: &Model) -> bool {
        model.visible && (self.game_active || self.keyboard)
    }

    pub fn blur(&mut self) {
        self.keyboard = false;
    }
}

impl Model {
    pub fn new(league: String) -> Self {
        Self {
            league,
            status: "securable".into(),
            base: true,
            ..Self::default()
        }
    }
    pub fn observe_session(&mut self, snapshot: &retinue_poe2::reader::Snapshot) -> bool {
        if let Some(player) = &snapshot.player {
            if self.character.as_deref() != Some(&player.name) {
                self.character = Some(player.name.clone());
                self.character_league = None;
            }
            if let Some(identity) = snapshot.social.as_ref().and_then(|s| s.local.as_ref())
                && identity.character == player.name
            {
                self.character_league = Some(identity.league.clone());
            }
        } else if matches!(snapshot.scene.as_str(), "login" | "select_character") {
            self.character = None;
            self.character_league = None;
            self.league_character = None;
        }
        let previous = self.league.clone();
        self.update_league();
        if self.league == previous {
            return false;
        }
        self.clear_prediction();
        self.invalidate();
        true
    }
    fn update_league(&mut self) {
        let Some(evaluation) = &self.evaluation else {
            return;
        };
        let available = |id: &str| evaluation.leagues.iter().any(|l| l.id == id);
        if self.character != self.league_character
            && let Some(league) = &self.character_league
            && available(league)
        {
            self.league = league.clone();
            self.league_character = self.character.clone();
        } else if !available(&self.league) {
            self.league = evaluation.league.clone();
        }
    }
    pub fn copy(&mut self) -> u64 {
        self.clear_prediction();
        self.invalidate();
        self.visible = false;
        self.copying = true;
        self.evaluation = None;
        self.text.clear();
        self.filters.clear();
        self.profile = None;
        self.conditions.clear();
        self.level = Filter::default();
        self.generation
    }
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.error = None;
        self.results = None;
        self.fetching = false;
        self.fetch_error = None;
    }
    pub fn dismiss(&mut self) {
        self.clear_prediction();
        self.invalidate();
        self.copying = false;
        self.visible = false;
    }
    pub fn copied(&mut self, id: u64, text: String) -> bool {
        if id != self.generation {
            return false;
        }
        self.copying = false;
        self.visible = true;
        self.loading = true;
        self.text = text;
        true
    }
    pub fn evaluated(&mut self, id: u64, evaluation: Evaluation) {
        if id != self.generation {
            return;
        }
        self.loading = false;
        self.error = None;
        self.profile = None;
        if let Some(options) = evaluation.search_options.get("status")
            && !options
                .iter()
                .any(|o| o.id.as_deref() == Some(self.status.as_str()))
            && let Some(id) = options
                .iter()
                .find(|o| o.id.as_deref() == Some("securable"))
                .or_else(|| options.first())
                .and_then(|o| o.id.clone())
        {
            self.status = id;
        }
        self.filters = evaluation
            .item
            .modifiers
            .iter()
            .map(|m| Filter {
                min: m.ranges.allmods.min.map(number).unwrap_or_default(),
                max: m.ranges.allmods.max.map(number).unwrap_or_default(),
                enabled: m.selected && m.supported,
            })
            .collect();
        self.rarity = Some(evaluation.item.rarity.to_ascii_lowercase());
        self.conditions = evaluation
            .item
            .conditions
            .iter()
            .map(|c| c.id.is_some())
            .collect();
        self.level = Filter {
            min: evaluation
                .item
                .item_level
                .map(|v| v.to_string())
                .unwrap_or_default(),
            ..Filter::default()
        };
        self.base = evaluation.item.base_required || evaluation.item.category.is_none();
        let profile = evaluation.item.default_profile;
        self.evaluation = Some(evaluation);
        self.update_league();
        if let Some(profile) = profile {
            self.apply_profile(profile);
        }
    }
    pub fn apply_profile(&mut self, profile: Profile) {
        let Some(evaluation) = &self.evaluation else {
            return;
        };
        if !evaluation.item.profiles.contains(&profile) {
            return;
        }
        for (modifier, filter) in evaluation.item.modifiers.iter().zip(&mut self.filters) {
            let range = match profile {
                Profile::Broad => &modifier.ranges.fuzzy,
                _ => &modifier.ranges.allmods,
            };
            filter.min = range.min.map(number).unwrap_or_default();
            filter.max = range.max.map(number).unwrap_or_default();
            filter.enabled = modifier.supported
                && match profile {
                    Profile::Base => modifier.base_selected,
                    Profile::Quick => modifier.quick_selected,
                    Profile::Exact => modifier.exact_selected,
                    Profile::Broad => filter.enabled,
                };
        }
        self.base = profile == Profile::Base
            || evaluation.item.base_required
            || evaluation.item.category.is_none();
        self.level = Filter {
            min: evaluation
                .item
                .item_level
                .map(|v| v.to_string())
                .unwrap_or_default(),
            enabled: profile == Profile::Base && evaluation.item.item_level.is_some(),
            ..Filter::default()
        };
        self.profile = Some(profile);
    }
    pub fn searched(&mut self, id: u64, results: Results) {
        if id == self.generation {
            self.loading = false;
            self.error = None;
            self.results = Some(results);
            self.fetch_error = None;
            self.fetching = false;
        }
    }
    pub fn restore(&mut self) -> Option<Value> {
        if self.loading {
            return None;
        }
        let mut request = self.query().ok()?;
        request["op"] = json!("trade_restore");
        self.generation = self.generation.wrapping_add(1);
        Some(request)
    }
    pub fn restored(&mut self, id: u64, reply: Restoration, requested: bool) -> bool {
        if id != self.generation || self.loading {
            return false;
        }
        if let Some(results) = reply.results {
            self.searched(id, results);
            return false;
        }
        let eligible = self.evaluation.as_ref().is_some_and(|e| {
            e.item.trade_unavailable.is_none() && (requested || e.item.auto_search)
        });
        if eligible && (requested || self.last_auto_attempt.as_ref() != Some(&self.text)) {
            self.last_auto_attempt = Some(self.text.clone());
            return reply.auto_search_allowed && self.retry_after() == 0;
        }
        false
    }
    pub fn fetch(&mut self, scroll: usize, visible: usize) -> Option<Value> {
        if self.loading || self.fetching || self.retry_after() > 0 || self.fetch_error.is_some() {
            return None;
        }
        let results = self.results.as_ref()?;
        let offset = results.next_offset?;
        if scroll.saturating_add(visible) < results.listings.len().saturating_sub(2) {
            return None;
        }
        self.fetching = true;
        self.fetch_error = None;
        Some(json!({"op":"trade_fetch", "search_id":results.search_id, "offset":offset}))
    }
    pub fn fetched(&mut self, id: u64, page: ListingPage) {
        if id != self.generation || !self.fetching {
            return;
        }
        let Some(results) = self.results.as_mut() else {
            return;
        };
        if page.search_id != results.search_id || Some(page.offset) != results.next_offset {
            return;
        }
        let mut seen: std::collections::HashSet<_> =
            results.listings.iter().map(|row| row.id.clone()).collect();
        results.listings.extend(
            page.listings
                .into_iter()
                .filter(|row| seen.insert(row.id.clone())),
        );
        results.next_offset = page.next_offset;
        self.fetching = false;
        self.fetch_error = None;
    }
    pub fn retry_after(&self) -> u64 {
        self.retry_at
            .map(|at| {
                at.saturating_duration_since(std::time::Instant::now())
                    .as_secs_f64()
                    .ceil() as u64
            })
            .unwrap_or(0)
    }
    pub fn tick_cooldown(&mut self) -> bool {
        let seconds = self.retry_after();
        let changed = seconds != self.cooldown_display;
        self.cooldown_display = seconds;
        changed
    }
    pub fn cooldown_deadline(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        self.retry_at
            .filter(|_| self.cooldown_display > 0)
            .map(|at| (at - std::time::Duration::from_secs(self.cooldown_display - 1)).max(now))
    }
    pub fn failed(&mut self, id: u64, error: String) {
        if id == self.generation {
            self.visible = true;
            self.copying = false;
            self.loading = false;
            if self.results.is_some() {
                self.fetch_error = Some(error);
            } else {
                self.error = Some(error);
            }
            self.fetching = false;
        }
    }
    pub fn clear_prediction(&mut self) {
        self.prediction_generation = self.prediction_generation.wrapping_add(1);
        self.predicting = false;
        self.estimate = None;
    }
    pub fn predict(&mut self) -> Option<(u64, Value)> {
        self.clear_prediction();
        if self.league.is_empty() || !self.evaluation.as_ref()?.prediction_available {
            return None;
        }
        self.predicting = true;
        Some((
            self.prediction_generation,
            json!({"op":"trade_predict",
            "text":self.text, "league":self.league}),
        ))
    }
    pub fn predicted(&mut self, id: u64, estimate: Option<Estimate>) {
        if id == self.prediction_generation {
            self.predicting = false;
            self.estimate = estimate;
        }
    }
    pub fn search(&mut self) -> Result<Value, String> {
        if self.loading {
            return Err("A request is already running".into());
        }
        if self.retry_after() > 0 {
            return Err("Trade requests are cooling down".into());
        }
        let mut request = self.query()?;
        request["refresh"] = json!(true);
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.fetching = false;
        self.fetch_error = None;
        Ok(request)
    }

    fn query(&self) -> Result<Value, String> {
        let evaluation = self.evaluation.as_ref().ok_or("No item to search")?;
        if evaluation.item.trade_unavailable.is_some() {
            return Err("This item is not listed in the trade catalog".into());
        }
        if !evaluation.leagues.iter().any(|l| l.id == self.league) {
            return Err("Choose your league before searching".into());
        }
        let mut filters = Vec::new();
        for (modifier, filter) in evaluation.item.modifiers.iter().zip(&self.filters) {
            if !filter.enabled {
                continue;
            }
            let id = modifier
                .id
                .as_ref()
                .filter(|_| modifier.supported)
                .ok_or("Unknown trade modifier")?;
            let min = parse_number(&filter.min)?;
            let max = parse_number(&filter.max)?;
            if min.zip(max).is_some_and(|(min, max)| min > max) {
                return Err("Minimum must not exceed maximum".into());
            }
            let mut entry = json!({"id":id});
            if let Some(value) = min {
                entry["min"] = json!(value);
            }
            if let Some(value) = max {
                entry["max"] = json!(value);
            }
            filters.push(entry);
        }
        let mut request = json!({"op":"trade_search", "text":self.text, "league":self.league,
            "status":self.status, "currency":self.currency, "indexed":self.indexed,
            "sort":if self.descending { "desc" } else { "asc" },
            "base":self.base, "filters":filters});
        request["conditions"] = json!(
            evaluation
                .item
                .conditions
                .iter()
                .zip(&self.conditions)
                .filter(|(_, enabled)| **enabled)
                .filter_map(|(c, _)| c.id.as_ref())
                .collect::<Vec<_>>()
        );
        if matches!(evaluation.item.rarity.as_str(), "Normal" | "Magic" | "Rare") {
            request["rarity"] = json!(self.rarity);
        }
        if self.level.enabled {
            let mut range = json!({});
            for (key, text) in [("min", &self.level.min), ("max", &self.level.max)] {
                if let Some(value) = parse_number(text)? {
                    if value.fract() != 0.0 || !(1.0..=100.0).contains(&value) {
                        return Err("Item level must be a whole number from 1 to 100".into());
                    }
                    range[key] = json!(value as u32);
                }
            }
            if range["min"]
                .as_u64()
                .zip(range["max"].as_u64())
                .is_some_and(|(a, b)| a > b)
            {
                return Err("Minimum must not exceed maximum".into());
            }
            request["item_level"] = range;
        }
        Ok(request)
    }

    pub fn choice_label(&self, key: &str) -> &str {
        let value = match key {
            "status" => Some(self.status.as_str()),
            "rarity" => self.rarity.as_deref(),
            "currency" => self.currency.as_deref(),
            "indexed" => self.indexed.as_deref(),
            _ => None,
        };
        self.evaluation
            .as_ref()
            .and_then(|e| e.search_options.get(key))
            .and_then(|options| options.iter().find(|o| o.id.as_deref() == value))
            .map(|o| o.text.as_str())
            .unwrap_or(match key {
                "status" => "Instant Buyout",
                "currency" => "Exalted Orb Equivalent",
                "indexed" => "Any Time",
                "rarity" => value.unwrap_or("Any"),
                _ => "",
            })
    }
}

pub fn numeric_edit(text: &str) -> bool {
    if text.len() > 16 {
        return false;
    }
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut decimal = false;
    text.chars()
        .all(|c| c.is_ascii_digit() || (c == '.' && !std::mem::replace(&mut decimal, true)))
}

pub fn parse_number(text: &str) -> Result<Option<f64>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= 1_000_000_000.0)
        .map(Some)
        .ok_or_else(|| "Enter a valid numeric range".into())
}

pub fn validate_item_text(text: String) -> Result<String, String> {
    if text.len() > 32 * 1024
        || text.contains('\0')
        || !text
            .lines()
            .take(3)
            .any(|line| line.starts_with("Rarity: "))
        || !text.contains("--------")
    {
        return Err("clipboard does not contain supported item text".into());
    }
    Ok(text)
}
pub fn number(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paged_model() -> Model {
        let mut m = Model::default();
        m.searched(
            0,
            Results {
                search_id: "search".into(),
                returned: 25,
                next_offset: Some(0),
                ..Results::default()
            },
        );
        m
    }

    fn page(offset: u32, end: u32, next: Option<u32>) -> ListingPage {
        ListingPage {
            search_id: "search".into(),
            offset,
            next_offset: next,
            listings: (offset..end)
                .map(|i| Listing {
                    id: i.to_string(),
                    ..Listing::default()
                })
                .collect(),
        }
    }

    #[test]
    fn pages_follow_visible_threshold_and_preserve_server_order() {
        let mut m = paged_model();
        assert_eq!(m.fetch(0, 8).unwrap()["offset"], 0);
        assert!(m.fetch(0, 8).is_none());
        m.fetched(0, page(0, 10, Some(10)));
        assert_eq!(m.fetch(0, 8).unwrap()["offset"], 10);
        m.fetched(0, page(10, 20, Some(20)));
        assert!(m.fetch(0, 8).is_none());
        assert!(m.fetch(9, 8).is_none());
        assert_eq!(m.fetch(10, 8).unwrap()["offset"], 20);
        m.fetched(0, page(20, 25, None));
        assert!(m.fetch(20, 8).is_none());
        assert_eq!(
            m.results
                .unwrap()
                .listings
                .iter()
                .map(|l| l.id.clone())
                .collect::<Vec<_>>(),
            (0..25).map(|i| i.to_string()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deleted_rows_advance_and_duplicate_or_stale_replies_do_not_append() {
        let mut m = paged_model();
        m.fetch(0, 8).unwrap();
        m.fetched(0, page(0, 0, Some(10)));
        assert_eq!(m.fetch(0, 8).unwrap()["offset"], 10);
        m.fetched(0, page(0, 10, Some(10)));
        assert!(m.fetching);
        assert!(m.results.as_ref().unwrap().listings.is_empty());
        m.fetched(0, page(10, 20, Some(20)));
        m.fetched(0, page(10, 20, Some(20)));
        assert_eq!(m.results.as_ref().unwrap().listings.len(), 10);
        m.invalidate();
        m.fetched(0, page(20, 25, None));
        assert!(m.results.is_none());
        assert!(!m.fetching);
    }

    #[test]
    fn failed_fetch_keeps_rows_without_automatic_retry_or_search_error_plaque() {
        let mut m = paged_model();
        m.fetch(0, 8).unwrap();
        m.fetched(0, page(0, 10, Some(10)));
        m.fetch(0, 8).unwrap();
        m.failed(0, "rate limited".into());
        m.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        assert_eq!(m.results.as_ref().unwrap().listings.len(), 10);
        assert_eq!(m.results.as_ref().unwrap().next_offset, Some(10));
        assert!(m.error.is_none());
        assert_eq!(m.fetch_error.as_deref(), Some("rate limited"));
        assert_eq!(m.retry_after(), 30);
        assert!(m.tick_cooldown());
        assert!(!m.tick_cooldown());
        assert!(m.fetch(9, 8).is_none());
        m.retry_at = Some(std::time::Instant::now());
        assert!(m.tick_cooldown());
        assert!(m.fetch(9, 8).is_none());
        m.invalidate();
        assert!(m.fetch_error.is_none());
    }

    #[test]
    fn cooldown_wakes_at_display_boundary_and_stops_when_finished() {
        let now = std::time::Instant::now();
        let mut model = Model::default();
        assert_eq!(model.cooldown_deadline(now), None);
        model.retry_at = Some(now + std::time::Duration::from_millis(2500));
        model.cooldown_display = 3;
        assert_eq!(
            model.cooldown_deadline(now),
            Some(now + std::time::Duration::from_millis(500))
        );
        let late = now + std::time::Duration::from_secs(4);
        assert_eq!(model.cooldown_deadline(late), Some(late));
        model.cooldown_display = 0;
        assert_eq!(model.cooldown_deadline(late), None);
    }

    #[test]
    fn prediction_survives_search_edits_but_not_a_new_item_league_or_close() {
        let mut m = Model::default();
        m.league = "A".into();
        m.evaluation = Some(Evaluation {
            prediction_available: true,
            ..Evaluation::default()
        });
        let (first, request) = m.predict().unwrap();
        assert_eq!(request["league"], "A");
        m.invalidate();
        let estimate = || {
            Some(Estimate {
                amount: 40.0,
                currency: "exalted".into(),
                uncertainty: None,
            })
        };
        m.predicted(first, estimate());
        assert_eq!(m.estimate.as_ref().unwrap().amount, 40.0);
        m.failed(m.generation, "search failed".into());
        assert!(m.estimate.is_some());
        m.league = "B".into();
        let (second, _) = m.predict().unwrap();
        m.predicted(first, estimate());
        assert!(m.predicting);
        assert!(m.estimate.is_none());
        m.predicted(second, None);
        assert!(!m.predicting);
        assert_eq!(m.error.as_deref(), Some("search failed"));
        let (third, _) = m.predict().unwrap();
        m.copy();
        m.predicted(third, estimate());
        assert!(m.estimate.is_none());
        m.evaluation = Some(Evaluation {
            prediction_available: true,
            ..Evaluation::default()
        });
        let (last, _) = m.predict().unwrap();
        m.dismiss();
        m.predicted(last, estimate());
        assert!(m.estimate.is_none());
    }
    fn fixture() -> Model {
        let data: Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/evaluate-reference.json"
        ))
        .unwrap();
        let mut model = Model::new(String::new());
        model.evaluated(
            0,
            serde_json::from_value(data["evaluation"].clone()).unwrap(),
        );
        model
    }

    #[test]
    fn profile_selection_survives_filter_edits_and_searches() {
        let mut model = fixture();
        assert_eq!(model.profile, Some(Profile::Quick));
        model
            .evaluation
            .as_mut()
            .unwrap()
            .item
            .profiles
            .push(Profile::Exact);
        model.apply_profile(Profile::Exact);
        model.invalidate();
        assert_eq!(model.profile, Some(Profile::Exact));
        assert!(model.filters.iter().all(|filter| filter.enabled));
        model.filters[0].enabled = false;
        model.filters[1].min = "30".into();
        model.invalidate();
        assert_eq!(model.profile, Some(Profile::Exact));
        model.apply_profile(Profile::Broad);
        model.invalidate();
        assert_eq!(model.profile, Some(Profile::Broad));
        assert!(!model.filters[0].enabled);
        assert!(model.filters[1].enabled);
        model.search().unwrap();
        assert_eq!(model.profile, Some(Profile::Broad));
        model.searched(model.generation, Results::default());
        assert_eq!(model.profile, Some(Profile::Broad));
        model.copy();
        assert_eq!(model.profile, None);
    }

    #[test]
    fn refresh_preserves_rows_until_search_returns_and_clears_them_on_filter_edit() {
        let mut m = fixture();
        let old = Results {
            search_id: "old".into(),
            listings: page(0, 3, None).listings,
            ..Results::default()
        };
        m.results = Some(old);
        let request = m.search().unwrap();
        assert_eq!(request["refresh"], true);
        assert!(m.loading);
        assert_eq!(m.results.as_ref().unwrap().listings.len(), 3);
        m.failed(m.generation, "upstream unavailable".into());
        assert!(m.error.is_none());
        assert_eq!(m.results.as_ref().unwrap().listings.len(), 3);
        m.search().unwrap();
        m.searched(
            m.generation,
            Results {
                search_id: "new".into(),
                next_offset: Some(0),
                ..Results::default()
            },
        );
        assert_eq!(m.results.as_ref().unwrap().search_id, "new");
        assert!(m.results.as_ref().unwrap().listings.is_empty());
        m.invalidate();
        assert!(m.results.is_none());
        assert_eq!(m.search().unwrap()["refresh"], true);
    }

    #[test]
    fn exact_profile_keeps_merged_modifiers_and_unranked_properties_off() {
        let mut model = fixture();
        let item = &mut model.evaluation.as_mut().unwrap().item;
        item.profiles.push(Profile::Exact);
        item.modifiers[0].section = "property".into();
        item.modifiers[0].quick_selected = true;
        item.modifiers[0].exact_selected = true;
        item.modifiers[1].exact_selected = false;
        item.modifiers[2].section = "property".into();
        item.modifiers[2].exact_selected = false;
        model.apply_profile(Profile::Exact);
        assert_eq!(
            model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
            vec![true, false, false]
        );
        model.apply_profile(Profile::Broad);
        assert_eq!(
            model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
            vec![true, false, false]
        );
    }

    #[test]
    fn profile_is_not_inferred_from_manually_selected_filters() {
        let mut model = fixture();
        for filter in &mut model.filters {
            filter.enabled = true;
        }
        model.invalidate();
        assert_eq!(model.profile, Some(Profile::Quick));
        model.apply_profile(Profile::Broad);
        for filter in &mut model.filters {
            filter.enabled = false;
        }
        model.invalidate();
        assert_eq!(model.profile, Some(Profile::Broad));
        let evaluation = model.evaluation.clone().unwrap();
        model.evaluated(model.generation.wrapping_sub(1), evaluation.clone());
        assert_eq!(model.profile, Some(Profile::Broad));
        model.evaluated(model.generation, evaluation);
        assert_eq!(model.profile, Some(Profile::Quick));
    }

    #[test]
    fn profile_bounds_are_supplied_by_daemon_for_every_row() {
        let cases: Vec<Value> = serde_json::from_slice(include_bytes!(
            "../../../retinue_cli/test/fixtures/poe2_trade_profiles.json"
        ))
        .unwrap();
        for case in cases {
            let mut model = fixture();
            let mut row = case["modifier"].clone();
            row["key"] = json!("profile-test");
            row["text"] = case["name"].clone();
            row["supported"] = json!(true);
            row["exact_selected"] = json!(true);
            row["ranges"] = case["expected"].clone();
            let mut evaluation = model.evaluation.take().unwrap();
            evaluation.item.profiles.push(Profile::Exact);
            evaluation.item.modifiers = vec![serde_json::from_value(row).unwrap()];
            model.evaluated(0, evaluation);
            let expected = |profile: &str, bound: &str| {
                case["expected"][profile][bound]
                    .as_f64()
                    .map(number)
                    .unwrap_or_default()
            };
            assert_eq!(model.filters[0].min, expected("allmods", "min"));
            assert_eq!(model.filters[0].max, expected("allmods", "max"));
            for enabled in [false, true] {
                model.filters[0] = Filter {
                    enabled,
                    min: "999".into(),
                    max: "999".into(),
                };
                for _ in 0..2 {
                    model.apply_profile(Profile::Broad);
                    assert_eq!(model.filters[0].enabled, enabled);
                    assert_eq!(model.filters[0].min, expected("fuzzy", "min"));
                    assert_eq!(model.filters[0].max, expected("fuzzy", "max"));
                }
            }
            model.apply_profile(Profile::Exact);
            assert!(model.filters[0].enabled);
            assert_eq!(model.filters[0].min, expected("allmods", "min"));
            assert_eq!(model.filters[0].max, expected("allmods", "max"));
            let query = model.search().unwrap();
            for bound in ["min", "max"] {
                assert_eq!(
                    query["filters"][0][bound].as_f64(),
                    case["expected"]["allmods"][bound].as_f64()
                );
            }
        }
    }

    #[test]
    fn category_defaults_and_profile_inclusion_come_from_daemon() {
        let mut model = fixture();
        assert_eq!(model.profile, Some(Profile::Quick));
        assert!(!model.base);
        assert!(!model.level.enabled);
        assert!(model.filters.iter().all(|f| !f.enabled));
        model.apply_profile(Profile::Exact);
        assert_eq!(model.profile, Some(Profile::Quick));

        let mut evaluation = model.evaluation.take().unwrap();
        evaluation.item.profiles = vec![Profile::Base, Profile::Quick, Profile::Broad];
        evaluation.item.default_profile = Some(Profile::Base);
        evaluation.item.modifiers[0].base_selected = true;
        evaluation.item.modifiers[1].quick_selected = true;
        model.evaluated(model.generation, evaluation);
        assert_eq!(model.profile, Some(Profile::Base));
        assert!(model.base && model.level.enabled);
        assert_eq!(model.level.min, "82");
        assert_eq!(
            model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
            [true, false, false]
        );
        model.apply_profile(Profile::Quick);
        assert!(!model.base && !model.level.enabled);
        assert_eq!(
            model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
            [false, true, false]
        );
        model.apply_profile(Profile::Broad);
        assert!(model.filters[1].enabled);
    }

    #[test]
    fn query_options_match_metadata_and_disabled_fields_stay_out() {
        let mut model = fixture();
        let query = model.search().unwrap();
        assert_eq!(query["status"], "securable");
        assert_eq!(query["rarity"], "rare");
        assert_eq!(query["sort"], "asc");
        assert_eq!(query["conditions"], json!(["modifiable"]));
        assert!(query.get("item_level").is_none());
        assert!(query["currency"].is_null());
        assert!(!query.as_object().unwrap().contains_key("online"));
        model.loading = false;
        model.status = "any".into();
        model.currency = Some("divine".into());
        model.indexed = Some("1day".into());
        model.rarity = None;
        model.descending = true;
        model.conditions[0] = false;
        model.level.enabled = true;
        model.level.min = "70".into();
        model.level.max = "85".into();
        let query = model.search().unwrap();
        assert_eq!(query["status"], "any");
        assert!(query["rarity"].is_null());
        assert_eq!(query["currency"], "divine");
        assert_eq!(query["indexed"], "1day");
        assert_eq!(query["conditions"], json!([]));
        assert_eq!(query["sort"], "desc");
        assert_eq!(query["item_level"], json!({"min":70,"max":85}));
    }

    #[test]
    fn invalid_level_does_not_start_or_discard_a_search() {
        let mut model = fixture();
        model.results = Some(Results::default());
        model.level.enabled = true;
        for value in ["NaN", "0", "101", "40.5"] {
            model.level.min = value.into();
            assert!(model.search().is_err());
            assert!(!model.loading);
            assert!(model.results.is_some());
            assert_eq!(model.generation, 0);
        }
    }

    #[test]
    fn automatic_search_uses_daemon_eligibility_and_budget() {
        for eligible in [false, true] {
            for allowed in [false, true] {
                let mut model = Model::default();
                model.evaluation = Some(Evaluation {
                    item: Item {
                        auto_search: eligible,
                        ..Item::default()
                    },
                    ..Evaluation::default()
                });
                assert_eq!(
                    model.restored(
                        0,
                        Restoration {
                            results: None,
                            auto_search_allowed: allowed
                        },
                        false
                    ),
                    eligible && allowed
                );
            }
        }
    }

    #[test]
    fn cache_lookup_can_restore_rare_items_during_cooldown_without_searching() {
        let mut model = fixture();
        model.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
        let request = model.restore().unwrap();
        assert_eq!(request["op"], "trade_restore");
        assert!(request.get("refresh").is_none());
        assert!(!model.loading);
        let restored = Restoration {
            results: Some(Results {
                search_id: "cached".into(),
                listings: page(0, 3, Some(10)).listings,
                next_offset: Some(10),
                ..Results::default()
            }),
            auto_search_allowed: false,
        };
        assert!(!model.restored(model.generation, restored, false));
        assert_eq!(model.results.as_ref().unwrap().listings.len(), 3);
        assert_eq!(model.results.as_ref().unwrap().next_offset, Some(10));
        assert!(!model.loading);
        assert!(model.search().is_err());
    }

    #[test]
    fn cache_miss_tracks_only_the_last_automatic_attempt() {
        let mut model = fixture();
        model.evaluation.as_mut().unwrap().item.auto_search = true;
        let miss = || Restoration {
            results: None,
            auto_search_allowed: true,
        };
        model.text = "first".into();
        assert!(model.restored(model.generation, miss(), false));
        model.search().unwrap();
        model.failed(model.generation, "upstream failed".into());
        model.restore().unwrap();
        assert!(!model.restored(model.generation, miss(), false));
        model.text = "second".into();
        model.restore().unwrap();
        assert!(model.restored(model.generation, miss(), false));
        model.text = "first".into();
        model.restore().unwrap();
        assert!(model.restored(model.generation, miss(), false));
        model.restore().unwrap();
        assert!(model.restored(model.generation, miss(), true));
    }

    #[test]
    fn cache_miss_on_a_rare_item_waits_for_search_or_a_result_selector() {
        let mut model = fixture();
        model.evaluation.as_mut().unwrap().item.auto_search = false;
        let miss = || Restoration {
            results: None,
            auto_search_allowed: true,
        };
        model.restore().unwrap();
        assert!(!model.restored(model.generation, miss(), false));
        assert!(model.results.is_none());
        assert!(!model.loading);
        assert!(model.restored(model.generation, miss(), true));
    }

    #[test]
    fn automatic_budget_denial_is_not_retried_but_empty_cached_results_are_hits() {
        let mut model = fixture();
        model.evaluation.as_mut().unwrap().item.auto_search = true;
        let reply = |allowed| Restoration {
            results: None,
            auto_search_allowed: allowed,
        };
        assert!(!model.restored(model.generation, reply(false), false));
        assert!(!model.restored(model.generation, reply(true), false));
        assert!(model.restored(model.generation, reply(true), true));
        let empty: Restoration = serde_json::from_value(json!({
            "auto_search_allowed": true,
            "results": {"search_id":"empty", "url":"", "total":0, "returned":0,
                "listings":[], "next_offset":null}
        }))
        .unwrap();
        assert!(!model.restored(model.generation, empty, true));
        assert_eq!(model.results.as_ref().unwrap().total, 0);
    }

    #[test]
    fn late_cache_lookup_cannot_replace_a_manual_search_or_edited_query() {
        let mut model = fixture();
        let hit = || Restoration {
            results: Some(Results::default()),
            auto_search_allowed: true,
        };
        model.restore().unwrap();
        let old = model.generation;
        assert_eq!(model.search().unwrap()["refresh"], true);
        assert!(!model.restored(old, hit(), false));
        assert!(model.results.is_none());
        assert!(model.loading);
        model.invalidate();
        model.restore().unwrap();
        let old = model.generation;
        model.filters[0].enabled = !model.filters[0].enabled;
        model.invalidate();
        model.restore().unwrap();
        assert!(!model.restored(old, hit(), false));
        assert!(model.results.is_none());
    }

    #[test]
    fn numeric_editor_accepts_transient_values_but_not_malformed_paste() {
        for value in ["", "-", "+", ".", "-.", "-12.5", "1234567890123456"] {
            assert!(numeric_edit(value), "{value}");
        }
        for value in ["1.2.3", "1-2", "++", "NaN", "12\n3", "12345678901234567"] {
            assert!(!numeric_edit(value), "{value}");
        }
    }
    #[test]
    fn rejects_unrelated_and_oversized_clipboard() {
        assert!(validate_item_text("personal clipboard".into()).is_err());
        assert!(
            validate_item_text(format!("Rarity: Rare\n--------\n{}", "x".repeat(65536))).is_err()
        );
        assert!(
            validate_item_text(
                "Item Class: Helmets\nRarity: Unique\nGoldrim\nFelt Cap\n--------".into()
            )
            .is_ok()
        );
    }

    #[test]
    fn stale_results_never_replace_new_item() {
        let mut m = Model::default();
        let old = m.copy();
        let current = m.copy();
        m.failed(old, "old".into());
        m.searched(old, Results::default());
        assert!(m.copying);
        assert!(m.error.is_none());
        assert!(m.results.is_none());
        assert!(!m.copied(old, "old".into()));
        assert!(m.copied(current, "new".into()));
        m.dismiss();
        m.failed(current, "late".into());
        assert!(!m.visible);
    }
    #[test]
    fn new_copy_cannot_retry_previous_items_text() {
        let mut model = Model::default();
        let first = model.copy();
        assert!(model.copied(first, "old item".into()));
        let second = model.copy();
        model.failed(second, "copy failed".into());
        assert!(model.text.is_empty());
        assert!(model.evaluation.is_none());
    }
    #[test]
    fn league_defaults_follow_metadata_without_overwriting_a_valid_choice() {
        let evaluation = Evaluation {
            leagues: vec![
                League {
                    id: "Current".into(),
                    text: "Current".into(),
                },
                League {
                    id: "Hardcore".into(),
                    text: "Hardcore".into(),
                },
            ],
            league: "Current".into(),
            ..Evaluation::default()
        };
        for saved in ["", "Expired"] {
            let mut model = Model::new(saved.into());
            model.evaluated(0, evaluation.clone());
            assert_eq!(model.league, "Current");
        }
        let mut model = Model::new("Hardcore".into());
        model.evaluated(0, evaluation);
        assert_eq!(model.league, "Hardcore");
    }

    fn observed_character(name: &str, league: Option<&str>) -> retinue_poe2::reader::Snapshot {
        use retinue_poe2::reader::{Identity, Player, Snapshot, Social};
        Snapshot {
            scene: "in_game".into(),
            player: Some(Player {
                name: name.into(),
                class: None,
                level: 10,
                experience: 0,
                gold: None,
                life: None,
            }),
            social: league.map(|league| Social {
                local: Some(Identity {
                    character: name.into(),
                    account: "Account".into(),
                    league: league.into(),
                }),
                party: vec![],
            }),
            ..Snapshot::default()
        }
    }

    fn trade_leagues() -> Evaluation {
        Evaluation {
            leagues: ["Standard", "Current", "Hardcore"]
                .into_iter()
                .map(|id| League {
                    id: id.into(),
                    text: id.into(),
                })
                .collect(),
            league: "Current".into(),
            ..Evaluation::default()
        }
    }

    #[test]
    fn native_league_applies_before_first_search_and_only_once_per_character() {
        let mut model = Model::new("Standard".into());
        let session = observed_character("Exile", Some("Current"));
        assert!(!model.observe_session(&session));
        model.evaluated(0, trade_leagues());
        assert_eq!(model.league, "Current");
        model.league = "Hardcore".into();
        assert!(!model.observe_session(&session));
        assert!(!model.observe_session(&observed_character("Exile", None)));
        let copy = model.copy();
        model.copied(copy, "Item".into());
        model.evaluated(copy, trade_leagues());
        assert_eq!(model.league, "Hardcore");
        assert!(model.observe_session(&observed_character("Other", Some("Standard"))));
        assert_eq!(model.league, "Standard");
    }

    #[test]
    fn late_identity_invalidates_old_prices_but_read_failures_do_not() {
        let mut model = Model::new("Standard".into());
        model.evaluated(0, trade_leagues());
        let generation = model.generation;
        let prediction = model.prediction_generation;
        assert!(model.observe_session(&observed_character("Exile", Some("Current"))));
        assert_ne!(model.generation, generation);
        model.searched(generation, Results::default());
        model.predicted(
            prediction,
            Some(Estimate {
                amount: 2.0,
                currency: "exalted".into(),
                uncertainty: None,
            }),
        );
        assert!(model.results.is_none());
        assert!(model.estimate.is_none());
        let generation = model.generation;
        assert!(!model.observe_session(&retinue_poe2::reader::Snapshot::default()));
        assert!(!model.observe_session(&observed_character("Exile", None)));
        assert_eq!(model.generation, generation);
        assert_eq!(model.league, "Current");
    }

    #[test]
    fn unavailable_leagues_do_not_guess_and_identity_waits_for_trade_metadata() {
        let mut model = Model::new("Hardcore".into());
        model.evaluated(0, trade_leagues());
        assert!(!model.observe_session(&observed_character("Solo", Some("SSF Current"))));
        assert_eq!(model.league, "Hardcore");
        assert!(!model.observe_session(&observed_character("New", Some("Future"))));
        let mut evaluation = trade_leagues();
        evaluation.leagues.push(League {
            id: "Future".into(),
            text: "Future".into(),
        });
        model.evaluated(0, evaluation);
        assert_eq!(model.league, "Future");
    }

    #[test]
    fn alt_tab_preserves_pending_results_and_restores_without_focus_theft() {
        let mut model = Model::new("Standard".into());
        let id = model.copy();
        model.copied(id, "item".into());
        let mut presentation = Presentation {
            game_active: false,
            keyboard: true,
        };
        assert!(presentation.visible(&model));
        presentation.blur();
        assert!(!presentation.visible(&model));
        model.searched(
            id,
            Results {
                total: 42,
                ..Results::default()
            },
        );
        assert!(!presentation.visible(&model));
        presentation.game_active = true;
        assert!(presentation.visible(&model));
        assert!(!presentation.keyboard);
        assert_eq!(model.results.as_ref().unwrap().total, 42);
        model.dismiss();
        assert!(!presentation.visible(&model));
        presentation.game_active = false;
        presentation.game_active = true;
        assert!(!presentation.visible(&model));
    }

    #[test]
    fn returning_to_game_does_not_reacquire_keyboard_after_layer_blur() {
        let model = Model {
            visible: true,
            ..Model::default()
        };
        let mut presentation = Presentation {
            game_active: true,
            keyboard: true,
        };
        presentation.blur();
        assert!(presentation.visible(&model));
        assert!(!presentation.keyboard);
        presentation.game_active = false;
        assert!(!presentation.visible(&model));
        presentation.game_active = true;
        assert!(presentation.visible(&model));
        assert!(!presentation.keyboard);
    }
    #[test]
    fn filters_are_explicit_and_reject_invalid_ranges() {
        let mut m = Model::new("Standard".into());
        m.evaluated(
            0,
            Evaluation {
                item: Item {
                    modifiers: vec![Modifier {
                        id: Some("explicit.life".into()),
                        min: Some(80.0),
                        supported: true,
                        ..Modifier::default()
                    }],
                    ..Item::default()
                },
                leagues: vec![League {
                    id: "Standard".into(),
                    text: "Standard".into(),
                }],
                league: "Standard".into(),
                ..Evaluation::default()
            },
        );
        assert_eq!(m.search().unwrap()["filters"], json!([]));
        m.loading = false;
        m.filters[0] = Filter {
            enabled: true,
            min: "90".into(),
            max: "80".into(),
        };
        assert!(m.search().is_err());
        m.filters[0].max = "NaN".into();
        assert!(m.search().is_err());
    }
}
