#[cfg(test)]
mod benchmark;
mod build_planner;
mod fixtures;
mod listing;
mod market;
mod right_rail;
use right_rail as rail;
#[cfg(test)]
mod tests;

use crate::{
    data::foundry::Asset, data::foundry::Descriptor, domain::media::Media,
    domain::session::Request, domain::session::Update, presentation::controls::ControlHost,
    presentation::controls::Signal, presentation::shell::Page,
};
use listing::{Listing, Quote};
use retinue_render::{
    damage::Damage, painter::Painter, painter::RasterImage, resources::Resource, thumbnails::Key,
    thumbnails::Request as ImageRequest,
};
use retinue_ui::{Rect, scroll::Scroll};
use serde_json::{Value, json};
use std::{
    collections::HashMap, collections::HashSet, collections::VecDeque, time::Duration,
    time::Instant, time::SystemTime, time::UNIX_EPOCH,
};

mod page_model;
pub use page_model::Pages;

fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn number(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or(0.0)
}
fn entries(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn asset(v: &Value) -> Option<Asset> {
    serde_json::from_value(v.clone()).ok()
}

pub(crate) mod controls;
pub(crate) mod foundry;
pub(crate) mod settings;
pub(crate) mod shell;
pub(crate) mod style;
pub(crate) mod widgets;
