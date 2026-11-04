use crate::data::evaluation::*;
use crate::domain::budget::Budget;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub enabled: bool,
    pub min: String,
    pub max: String,
}

#[derive(Default)]
pub struct EvaluateModel {
    pub budget: Budget,
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
    pub results_stale: bool,
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
    pub fn visible(&self, model: &EvaluateModel) -> bool {
        model.visible && (self.game_active || self.keyboard)
    }

    pub fn blur(&mut self) {
        self.keyboard = false;
    }
}

impl EvaluateModel {
    pub fn currency_request(&self, scale: f32) -> crate::domain::currency::Request {
        let ids = self
            .results
            .iter()
            .flat_map(|r| r.listings.iter().map(|l| l.currency.clone()))
            .chain(self.estimate.iter().map(|e| e.currency.clone()));
        crate::domain::currency::Request::new(ids, (20.0 * scale).round() as u32, self.generation)
    }

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
        self.results_stale = false;
        self.fetching = false;
        self.fetch_error = None;
    }
    pub fn refresh_results(&mut self) {
        let results = self.results.take();
        self.invalidate();
        self.results_stale = results.is_some();
        self.results = results;
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
            self.results_stale = false;
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
        if self.loading
            || self.fetching
            || self.results_stale
            || self.retry_after() > 0
            || self.fetch_error.is_some()
        {
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
        if key == "league" {
            return self
                .evaluation
                .as_ref()
                .and_then(|e| e.leagues.iter().find(|l| l.id == self.league))
                .map(|l| l.text.as_str())
                .unwrap_or(&self.league);
        }
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

pub fn number(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
