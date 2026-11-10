use std::collections::BTreeMap;
use std::time::Instant;

use serde::Serialize;

use super::{DisplayNode, DisplayTree, Movie, geometry, registry};
use crate::game_observer::{
    ProcessIdentity, adapter::ScaleformLayout, memory, memory::ProcessMemory,
};

const SQUAD: &str = "/Lotus/Interface/ThemedSquadOverlay.swf";
const INVENTORY: &str = "/Lotus/Interface/InventoryTest.swf";
const POPUP: &str = "/Lotus/Interface/ItemInfoPopup.swf";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DucatHover {
    pub title: String,
    pub ducats: u64,
    pub viewport: [u32; 2],
    pub panel: [f32; 4],
    pub price_label: [f32; 4],
    pub scale: f32,
}

#[derive(Debug, Serialize)]
pub struct DucatState {
    pub kiosk_open: bool,
    pub hover: Option<DucatHover>,
    pub total_us: u128,
}

pub struct DucatReader {
    pid: u32,
    display: ScaleformLayout,
    geometry: geometry::layout::Layout,
    cache: Cache,
}

impl DucatReader {
    pub fn open_for_identity(identity: &ProcessIdentity) -> Result<Self, String> {
        let (display, geometry) = geometry::bindings(identity)?;
        Ok(Self {
            pid: identity.pid,
            display,
            geometry,
            cache: Cache::default(),
        })
    }

    pub fn read(&mut self) -> Result<DucatState, String> {
        self.cache
            .read(&memory::open(self.pid)?, self.display, &self.geometry)
    }
}

#[derive(Default)]
pub(crate) struct Cache {
    gate: Option<Selection>,
    popup: Option<Selection>,
}

impl Cache {
    pub(crate) fn read(
        &mut self,
        memory: &ProcessMemory,
        display: ScaleformLayout,
        geometry: &geometry::layout::Layout,
    ) -> Result<DucatState, String> {
        let started = Instant::now();
        let snapshot = registry::scan(memory, display)
            .map_err(|(stage, reason)| format!("{stage}: {reason}"))?
            .snapshot;
        let mut state = DucatState {
            kiosk_open: false,
            hover: None,
            total_us: 0,
        };
        let Some(_) = unique_movie(&snapshot.movies, INVENTORY)? else {
            self.popup = None;
            return Ok(state);
        };
        if let Some(movie) = unique_movie(&snapshot.movies, SQUAD)? {
            let tree = selected_tree(&mut self.gate, memory, display, geometry, movie, false)?;
            state.kiosk_open = kiosk_open(&tree)?;
        }
        if state.kiosk_open
            && let Some(movie) = unique_movie(&snapshot.movies, POPUP)?
        {
            let tree = selected_tree(&mut self.popup, memory, display, geometry, movie, true)?;
            state.hover = hover(&tree)?;
        }
        state.total_us = started.elapsed().as_micros();
        Ok(state)
    }
}

fn unique_movie<'a>(movies: &'a [Movie], path: &str) -> Result<Option<&'a Movie>, String> {
    let mut matches = movies.iter().filter(|movie| movie.path == path);
    let result = matches.next();
    if matches.next().is_some() {
        return Err(format!("ambiguous movie: {path}"));
    }
    Ok(result)
}

struct Selection {
    movie: u64,
    targets: Vec<u64>,
    identities: Vec<(u64, String, u64, Vec<u64>)>,
}

impl Selection {
    fn new(tree: &DisplayTree, popup: bool) -> Result<Self, String> {
        let mut targets = Vec::new();
        for node in &tree.nodes {
            let path = path(tree, node)?;
            let selected = if popup {
                (node.name == "Title" && has_roles(&path, &["Popup", "Title"]))
                    || has_roles(&path, &["Popup", "TitleBg"])
                    || has_roles(&path, &["Popup", "TagContainer"])
            } else {
                node.name == "Label" && has_roles(&path, &["TitleBar", "Title", "Label"])
            };
            if selected {
                targets.push(node.address);
            }
        }
        if targets.is_empty() {
            return Err("tooltip display paths unavailable".into());
        }
        let by_address: BTreeMap<_, _> = tree.nodes.iter().map(|n| (n.address, n)).collect();
        let mut identities = BTreeMap::new();
        for target in &targets {
            let mut address = *target;
            while address != 0 && !identities.contains_key(&address) {
                let node = by_address.get(&address).ok_or("unknown tooltip ancestor")?;
                identities.insert(
                    address,
                    (
                        address,
                        node.name.clone(),
                        node.parent,
                        node.children.clone(),
                    ),
                );
                address = node.parent;
            }
        }
        Ok(Self {
            movie: tree.movie.record_address,
            targets,
            identities: identities.into_values().collect(),
        })
    }

    fn matches(&self, tree: &DisplayTree) -> bool {
        self.identities
            .iter()
            .all(|(address, name, parent, children)| {
                tree.nodes.iter().any(|n| {
                    n.address == *address
                        && n.name == *name
                        && n.parent == *parent
                        && n.children == *children
                        && n.unavailable.is_none()
                })
            })
    }
}

fn selected_tree(
    cache: &mut Option<Selection>,
    memory: &ProcessMemory,
    display: ScaleformLayout,
    geometry: &geometry::layout::Layout,
    movie: &Movie,
    popup: bool,
) -> Result<DisplayTree, String> {
    if let Some(selection) = cache.as_ref().filter(|s| s.movie == movie.record_address) {
        if let Ok(tree) =
            geometry::read_selected(memory, display, geometry, movie, &selection.targets)
            && selection.matches(&tree)
        {
            return Ok(tree);
        }
        *cache = None;
    }
    let tree = geometry::read_tree(memory, display, geometry, movie, 2048)?;
    if !tree.consistent {
        return Err("tooltip changed during discovery".into());
    }
    *cache = Some(Selection::new(&tree, popup)?);
    Ok(tree)
}

fn path(tree: &DisplayTree, node: &DisplayNode) -> Result<String, String> {
    let mut parts = Vec::new();
    let mut current = node;
    for _ in 0..32 {
        if !current.name.is_empty() {
            parts.push(current.name.as_str());
        }
        if current.parent == 0 {
            parts.reverse();
            return Ok(parts.join("."));
        }
        current = tree
            .nodes
            .iter()
            .find(|n| n.address == current.parent)
            .ok_or("unknown tooltip ancestor")?;
    }
    Err("tooltip parent cycle/depth limit".into())
}

fn kiosk_open(tree: &DisplayTree) -> Result<bool, String> {
    if !tree.consistent {
        return Err("kiosk title changed during read".into());
    }
    let titles: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| {
            n.name == "Label"
                && path(tree, n).is_ok_and(|p| has_roles(&p, &["TitleBar", "Title", "Label"]))
        })
        .collect();
    let [title] = titles.as_slice() else {
        return Err("kiosk title path missing or ambiguous".into());
    };
    Ok(title.visible
        && title.unavailable.is_none()
        && title
            .text
            .as_deref()
            .and_then(plain_text)
            .is_some_and(|text| text.trim() == "INVENTORY/DUCAT KIOSK"))
}

fn hover(tree: &DisplayTree) -> Result<Option<DucatHover>, String> {
    if !tree.consistent {
        return Err("tooltip changed during read".into());
    }
    let nodes: Vec<_> = tree
        .nodes
        .iter()
        .map(|n| path(tree, n).map(|p| (p, n)))
        .collect::<Result<_, _>>()?;
    let titles: Vec<_> = nodes
        .iter()
        .filter(|(p, n)| n.name == "Title" && has_roles(p, &["Popup", "Title"]))
        .collect();
    if titles.is_empty() {
        return Err("tooltip title missing".into());
    }
    let visible: Vec<_> = titles.into_iter().filter(|(_, n)| n.visible).collect();
    if visible.is_empty() {
        return Ok(None);
    }
    let [(_, title)] = visible.as_slice() else {
        return Err("tooltip title missing or ambiguous".into());
    };
    if title.unavailable.is_some() {
        return Err("tooltip title geometry unavailable".into());
    }
    let title = title
        .text
        .as_deref()
        .and_then(plain_text)
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or("tooltip item name unavailable")?;
    let labels: Vec<_> = nodes
        .iter()
        .filter_map(|(p, n)| {
            if n.name != "Label" || !has_roles(p, &["Popup", "TagContainer", "Label"]) || !n.visible
            {
                return None;
            }
            let text = n.text.as_deref().and_then(plain_text)?;
            currency_amount(&text).map(|amount| (*n, amount))
        })
        .collect();
    let [(label, ducats)] = labels.as_slice() else {
        return Err(format!(
            "visible kiosk tooltip has {} currency price labels",
            labels.len()
        ));
    };
    if !(1..=1000).contains(ducats) {
        return Err("ducat price outside supported range".into());
    }
    let panels: Vec<_> = nodes
        .iter()
        .filter(|(p, n)| {
            has_roles(p, &["Popup", "TitleBg"]) && n.visible && n.screen_bounds.is_some()
        })
        .collect();
    let [(_, panel)] = panels.as_slice() else {
        return Err("tooltip background missing or ambiguous".into());
    };
    let panel = bounds(panel)?;
    let price_label = bounds(label)?;
    if !contains(panel, price_label)
        || !contains(
            [0.0, 0.0, tree.movie.width as f32, tree.movie.height as f32],
            panel,
        )
    {
        return Err("tooltip bounds outside panel or viewport".into());
    }
    Ok(Some(DucatHover {
        title,
        ducats: *ducats,
        viewport: [tree.movie.width, tree.movie.height],
        panel,
        price_label,
        scale: 1.0 / tree.movie.scale_y,
    }))
}

fn has_roles(path: &str, roles: &[&str]) -> bool {
    let mut names = path.split('.');
    roles.iter().all(|role| names.any(|name| name == *role))
}

fn currency_amount(text: &str) -> Option<u64> {
    // Game updates can renumber private-use font icons. Kiosk scope and the
    // inventory's exact name/value match identify the currency, not its glyph ID.
    let mut chars = text.trim().chars();
    if !matches!(chars.next()?, '\u{e000}'..='\u{f8ff}') {
        return None;
    }
    let amount = chars.as_str();
    if !amount.starts_with(char::is_whitespace) {
        return None;
    }
    let amount = amount.trim();
    if amount.is_empty() || !amount.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    amount.parse().ok()
}

fn bounds(node: &DisplayNode) -> Result<[f32; 4], String> {
    let rect = node
        .screen_bounds
        .filter(|r| r.iter().all(|v| v.is_finite()) && r[0] < r[2] && r[1] < r[3])
        .ok_or("tooltip bounds unavailable")?;
    if node.unavailable.is_some() {
        return Err("tooltip geometry unavailable".into());
    }
    Ok(rect)
}

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

fn plain_text(html: &str) -> Option<String> {
    let mut text = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let end = rest[start..].find('>')? + start;
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    let text = text
        .replace("&nbsp;", " ")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests;
