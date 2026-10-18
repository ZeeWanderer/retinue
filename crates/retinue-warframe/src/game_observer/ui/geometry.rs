pub(crate) mod layout;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::Serialize;

use super::{Movie, display, registry};
use crate::game_observer::{
    ProcessIdentity,
    adapter::ScaleformLayout,
    executable,
    memory::{self, ProcessMemory},
};
use display::{has_vtables, pointers, read, read_prefix, u64_at};

const MAX_NODES: usize = 10_000;
const MAX_TEXT: usize = 4096;
const MAX_DEPTH: usize = 32;

#[derive(Debug, Serialize)]
pub struct DisplayTree {
    pub movie: Movie,
    pub nodes: Vec<DisplayNode>,
    pub unrecognized_objects: usize,
    pub consistent: bool,
    pub total_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct DisplayNode {
    pub address: u64,
    pub kind: &'static str,
    pub name: String,
    pub parent: u64,
    pub children: Vec<u64>,
    pub text: Option<String>,
    pub visible: bool,
    pub alpha: Option<f32>,
    pub screen_bounds: Option<[f32; 4]>,
    pub unavailable: Option<String>,
}

pub struct DisplayReader {
    pid: u32,
    display: ScaleformLayout,
    geometry: layout::Layout,
}

impl DisplayReader {
    pub fn open_for_identity(identity: &ProcessIdentity) -> Result<Self, String> {
        let (display, geometry) = bindings(identity)?;
        Ok(Self {
            pid: identity.pid,
            display,
            geometry,
        })
    }

    pub fn read(&self, movie: &str) -> Result<DisplayTree, String> {
        let memory = memory::open(self.pid)?;
        let scan = registry::scan(&memory, self.display)
            .map_err(|(stage, error)| format!("{stage}: {error}"))?;
        let mut matches = scan
            .snapshot
            .movies
            .iter()
            .filter(|m| m.path.contains(movie));
        let found = matches.next().ok_or("movie not open")?;
        if matches.next().is_some() {
            return Err("movie selection is ambiguous".into());
        }
        read_tree(&memory, self.display, &self.geometry, found, MAX_NODES)
    }
}

pub(crate) fn bindings(
    identity: &ProcessIdentity,
) -> Result<(ScaleformLayout, layout::Layout), String> {
    let (hash, bytes) = executable::read(&identity.executable.path)?;
    if hash != identity.executable.sha256 {
        return Err("executable changed since process identification".into());
    }
    let display = super::layout::discover(&bytes)?;
    Ok((display, layout::discover(&bytes, display)?))
}

struct Node {
    header: Vec<u8>,
    result: DisplayNode,
    matrix: [f32; 16],
    local_rect: Option<[f32; 4]>,
    alpha: [f32; 2],
    visible: bool,
    parent_alive: bool,
    text_pointer: u64,
}

pub(crate) fn read_tree(
    memory: &ProcessMemory,
    display: ScaleformLayout,
    geometry: &layout::Layout,
    movie: &Movie,
    limit: usize,
) -> Result<DisplayTree, String> {
    read_nodes(memory, display, geometry, movie, limit, None)
}

pub(super) fn read_selected(
    memory: &ProcessMemory,
    display: ScaleformLayout,
    geometry: &layout::Layout,
    movie: &Movie,
    selected: &[u64],
) -> Result<DisplayTree, String> {
    read_nodes(memory, display, geometry, movie, 256, Some(selected))
}

fn read_nodes(
    memory: &ProcessMemory,
    display: ScaleformLayout,
    geometry: &layout::Layout,
    movie: &Movie,
    limit: usize,
    selected: Option<&[u64]>,
) -> Result<DisplayTree, String> {
    if !(1..=MAX_NODES).contains(&limit) {
        return Err("node limit must be 1..10000".into());
    }
    if !movie.scale_x.is_finite()
        || !movie.scale_y.is_finite()
        || movie.scale_x <= 0.0
        || movie.scale_y <= 0.0
    {
        return Err("invalid movie scale".into());
    }
    let started = Instant::now();
    let image = memory.image_base().ok_or("game image absent")?;
    let flash = movie
        .record_address
        .checked_sub(0xc0)
        .ok_or("invalid movie record")?;
    let actual_type = read(memory, flash, 8, "Flash type")?;
    if u64_at(&actual_type, 0) != Some(image + display.flash_instance_vtable_rva) {
        return Err("Flash instance type changed".into());
    }
    let (registered, _) = display::registered_objects(memory, display, image, flash)?;
    let registered: BTreeSet<_> = pointers(&registered).filter(|&a| a != 0).collect();
    let mut pending: Vec<_> = selected
        .map(|addresses| addresses.to_vec())
        .unwrap_or_else(|| registered.iter().copied().collect());
    let mut nodes = BTreeMap::new();
    let mut seen = BTreeSet::new();
    while let Some(address) = pending.pop() {
        if !seen.insert(address) {
            continue;
        }
        if seen.len() > limit {
            return Err("display tree node limit exceeded".into());
        }
        let types = read(memory, address, 0x18, "display object type")?;
        let vtable = u64_at(&types, 0).unwrap();
        let is_container = has_vtables(
            &types,
            image + display.container_vtable_rva,
            image + display.container_secondary_vtable_rva,
        );
        let is_text = has_vtables(
            &types,
            image + display.text_vtable_rva,
            image + display.text_secondary_vtable_rva,
        );
        if (vtable == image + display.container_vtable_rva && !is_container)
            || (vtable == image + display.text_vtable_rva && !is_text)
        {
            return Err("display secondary type changed".into());
        }
        let definition = vtable
            .checked_sub(image)
            .and_then(|rva| geometry.leaf_definitions.get(&rva))
            .copied();
        if !is_container && definition.is_none() {
            continue;
        }
        let length = (geometry.flags + 1)
            .max(if is_container {
                0x140
            } else {
                definition.unwrap() + 8
            })
            .max(if is_text { 0x198 } else { 0 });
        let header = read(memory, address, length, "display object")?;
        let name = String::from_utf8(
            display::instance_name_at(memory, &header, geometry.name)?.into_owned(),
        )
        .map_err(|_| "invalid instance name encoding")?;
        let matrix = floats::<16>(&header, geometry.matrix)?;
        let alpha = floats::<2>(&header, geometry.alpha)?;
        let visible = header[geometry.flags] & 1 != 0;
        let parent = u64_at(&header, geometry.parent).ok_or("missing display parent")?;
        let control = u64_at(&header, geometry.parent_control).ok_or("missing parent lifetime")?;
        let parent_alive =
            parent == 0 || (control != 0 && read(memory, control, 9, "parent lifetime")?[8] != 0);
        let mut children = Vec::new();
        if is_container && header[0x138..0x13c] != [0; 4] {
            let vector = display::optional_vector_at(memory, &header, 0x130, MAX_NODES)
                .ok_or("invalid display child vector")?;
            children = pointers(&read(
                memory,
                vector.address,
                vector.bytes,
                "display children",
            )?)
            .filter(|&a| a != 0)
            .collect();
            if pending.len() + children.len() > MAX_NODES * 4 {
                return Err("display tree edge limit exceeded".into());
            }
            if selected.is_none() {
                pending.extend(children.iter().copied());
            }
        }
        if selected.is_some() {
            if parent != 0 {
                pending.push(parent);
            } else if !registered.contains(&address) {
                return Err("selected display root is no longer registered".into());
            }
        }
        let rect = definition
            .map(|offset| definition_rect(memory, geometry, image, &header, offset))
            .transpose();
        let (local_rect, unavailable) = match rect {
            Ok(value) => (value, None),
            Err(error) => (None, Some(error)),
        };
        let text_pointer = if is_text {
            u64_at(&header, 0x190).unwrap_or(0)
        } else {
            0
        };
        let text = text_value(memory, text_pointer)?;
        nodes.insert(
            address,
            Node {
                result: DisplayNode {
                    address,
                    kind: if is_text {
                        "text"
                    } else if is_container {
                        "container"
                    } else {
                        "leaf"
                    },
                    name,
                    parent,
                    children,
                    text,
                    visible: false,
                    alpha: None,
                    screen_bounds: None,
                    unavailable,
                },
                header,
                matrix,
                local_rect,
                alpha,
                visible,
                parent_alive,
                text_pointer,
            },
        );
    }
    let mut consistent = true;
    let mut results = Vec::with_capacity(nodes.len());
    for (&address, node) in &nodes {
        let state = project(address, &nodes, movie);
        let current = read(memory, address, node.header.len(), "display consistency")?;
        if current != node.header
            || text_value(memory, node.text_pointer)? != node.result.text
            || (node.result.kind == "container"
                && !children_unchanged(memory, &current, &node.result.children)?)
        {
            consistent = false;
        }
        results.push((address, state));
    }
    for (address, state) in results {
        let node = nodes.get_mut(&address).unwrap();
        match state {
            Ok((visible, alpha, bounds)) => {
                node.result.visible = visible;
                node.result.alpha = Some(alpha);
                node.result.screen_bounds = bounds;
            }
            Err(error) => node.result.unavailable = Some(error),
        }
    }
    Ok(DisplayTree {
        movie: movie.clone(),
        unrecognized_objects: seen.len() - nodes.len(),
        nodes: nodes.into_values().map(|n| n.result).collect(),
        consistent,
        total_ms: started.elapsed().as_millis(),
    })
}

fn children_unchanged(
    memory: &ProcessMemory,
    header: &[u8],
    expected: &[u64],
) -> Result<bool, String> {
    if header[0x138..0x13c] == [0; 4] {
        return Ok(expected.is_empty());
    }
    let vector = display::optional_vector_at(memory, header, 0x130, MAX_NODES)
        .ok_or("invalid display child vector")?;
    let bytes = read(
        memory,
        vector.address,
        vector.bytes,
        "display child consistency",
    )?;
    Ok(pointers(&bytes)
        .filter(|&a| a != 0)
        .eq(expected.iter().copied()))
}

fn definition_rect(
    memory: &ProcessMemory,
    layout: &layout::Layout,
    image: u64,
    header: &[u8],
    offset: usize,
) -> Result<[f32; 4], String> {
    let pointer = u64_at(header, offset).ok_or("missing definition pointer")?;
    let definition = read(memory, pointer, 8, "display definition")?;
    let vtable = u64_at(&definition, 0)
        .and_then(|v| v.checked_sub(image))
        .ok_or("invalid definition type")?;
    let offset = *layout
        .definition_rects
        .get(&vtable)
        .ok_or("unknown definition bounds method")?;
    let rect = floats::<4>(
        &read(memory, pointer + offset as u64, 16, "definition bounds")?,
        0,
    )?;
    if rect[0] > rect[1] || rect[2] > rect[3] {
        return Err("invalid definition bounds".into());
    }
    Ok(rect)
}

fn text_value(memory: &ProcessMemory, pointer: u64) -> Result<Option<String>, String> {
    if pointer == 0 {
        return Ok(None);
    }
    let bytes = read_prefix(memory, pointer, MAX_TEXT).ok_or("unreadable display text")?;
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or("unterminated display text")?;
    String::from_utf8(bytes[..end].to_vec())
        .map(Some)
        .map_err(|_| "invalid display text encoding".into())
}

fn floats<const N: usize>(bytes: &[u8], offset: usize) -> Result<[f32; N], String> {
    let slice = bytes
        .get(offset..offset + N * 4)
        .ok_or("missing display floats")?;
    let result =
        std::array::from_fn(|i| f32::from_le_bytes(slice[i * 4..i * 4 + 4].try_into().unwrap()));
    if result.iter().any(|v| !v.is_finite()) {
        return Err("non-finite display geometry".into());
    }
    Ok(result)
}

fn transform(matrix: &[f32; 16], point: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|row| (0..4).map(|col| matrix[col * 4 + row] * point[col]).sum())
}

type Projection = (bool, f32, Option<[f32; 4]>);

fn project(address: u64, nodes: &BTreeMap<u64, Node>, movie: &Movie) -> Result<Projection, String> {
    let first = nodes.get(&address).ok_or("unknown display node")?;
    let mut corners = first.local_rect.map(|r| {
        [
            [r[0], r[2], 0.0, 1.0],
            [r[1], r[2], 0.0, 1.0],
            [r[0], r[3], 0.0, 1.0],
            [r[1], r[3], 0.0, 1.0],
        ]
    });
    let mut current = address;
    let mut seen = BTreeSet::new();
    let mut visible = true;
    let mut alpha = 1.0;
    while current != 0 {
        if !seen.insert(current) || seen.len() > MAX_DEPTH {
            return Err("display parent cycle/depth limit".into());
        }
        let node = nodes.get(&current).ok_or("unknown display ancestor")?;
        if !node.parent_alive {
            return Err("expired display parent".into());
        }
        visible &= node.visible;
        alpha = alpha * node.alpha[0] + node.alpha[1];
        if !alpha.is_finite() {
            return Err("non-finite display alpha".into());
        }
        if let Some(points) = &mut corners {
            *points = points.map(|point| transform(&node.matrix, point));
        }
        if node.result.parent != 0
            && !nodes
                .get(&node.result.parent)
                .is_some_and(|parent| parent.result.children.contains(&current))
        {
            return Err("display parent/child links disagree".into());
        }
        current = node.result.parent;
    }
    let bounds = corners
        .map(|points| {
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for point in points {
                if point.iter().any(|v| !v.is_finite()) || point[3].abs() < 1e-6 {
                    return Err("invalid display projection".to_owned());
                }
                let x = point[0] / point[3] / 20.0 / movie.scale_x;
                let y = point[1] / point[3] / 20.0 / movie.scale_y;
                if !x.is_finite() || !y.is_finite() {
                    return Err("non-finite screen bounds".into());
                }
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
            Ok(bounds)
        })
        .transpose()?;
    Ok((visible && alpha > 0.01, alpha, bounds))
}

#[cfg(test)]
mod tests;
