use serde::{Deserialize, Serialize};
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
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
