use super::*;
use crate::{PaintProgram, ProgramId, damage::Region, painter::RasterImage};

#[cfg(test)]
mod benchmark;
mod job;
mod leaf;
use super::evidence;
pub use job::{PacketError, PacketJob, Wake};
pub(super) use leaf::LeafPackets;

enum Draw {
    #[cfg(feature = "benchmark")]
    Published(
        Arc<Vec<crate::PaintInstance>>,
        rustc_hash::FxHashMap<Key, Arc<RasterImage>>,
    ),
    Packet(Box<DrawPacket>),
    Shared(Arc<DrawPacket>, (i32, i32)),
    Image {
        item: Option<RasterId>,
        image: Arc<RasterImage>,
        origin: (i32, i32),
        clip: Rect,
    },
    Program {
        program: PaintProgram,
        origin: (i32, i32),
        clip: Rect,
        visible: Rect,
    },
}

/// Immutable native submission input. Programs and resources are leased, not copied.
pub struct DrawPacket {
    feedback: Option<Arc<evidence::Feedback>>,
    draws: Vec<Draw>,
    fonts: Arc<crate::text::preparation::PreparedFonts>,
    fragments: HashMap<ProgramId, Arc<RasterImage>>,
    children: BTreeMap<Key, DrawPacket>,
    optimized: bool,
    repair: Option<Region>,
}

impl DrawPacket {
    /// Replay an immutable child in its parent's clip without copying its commands or pixels.
    pub fn placed(child: Arc<Self>, offset: (i32, i32)) -> Self {
        Self {
            feedback: None,
            draws: vec![Draw::Shared(child, offset)],
            fonts: Default::default(),
            fragments: Default::default(),
            children: Default::default(),
            optimized: true,
            repair: None,
        }
    }

    pub fn stack(packets: impl IntoIterator<Item = Self>) -> Self {
        Self {
            feedback: None,
            draws: packets
                .into_iter()
                .map(|packet| Draw::Packet(Box::new(packet)))
                .collect(),
            fonts: Default::default(),
            fragments: Default::default(),
            children: Default::default(),
            optimized: true,
            repair: None,
        }
    }

    pub fn image(image: Arc<RasterImage>, origin: (i32, i32), clip: Rect) -> Self {
        Self {
            feedback: None,
            draws: vec![Draw::Image {
                item: None,
                image,
                origin,
                clip,
            }],
            fonts: Default::default(),
            fragments: Default::default(),
            children: Default::default(),
            optimized: true,
            repair: None,
        }
    }

    pub fn draw(&self, painter: &mut Painter<'_>) -> PaintStats {
        if let [Draw::Shared(child, offset)] = self.draws.as_slice() {
            if let Some(requested) = &self.repair {
                debug_assert_eq!(*offset, (0, 0));
                return child.draw_contents(painter, Some(requested));
            }
            return if *offset == (0, 0) {
                child.draw(painter)
            } else {
                painter.translated(*offset, |p| child.draw(p))
            };
        }
        self.draw_contents(painter, self.repair.as_ref())
    }

    fn draw_contents(&self, painter: &mut Painter<'_>, repair: Option<&Region>) -> PaintStats {
        if let Some(repair) = repair {
            let visible = painter.visible_regions();
            if visible != *repair && visible.intersection_with(repair).area() != visible.area() {
                painter.fail(crate::painter::PainterError::invalid(
                    "packet repair expanded after preparation",
                ));
                return PaintStats::default();
            }
            if visible.is_empty() {
                return PaintStats::default();
            }
            if let Some(feedback) = &self.feedback {
                let mut receipt = PaintReceipt::new(feedback, visible);
                let stats = match &mut receipt.recorded {
                    Some(uses) => self.replay::<true>(painter, uses),
                    None => self.replay::<false>(painter, &mut Vec::new()),
                };
                painter.observed(receipt);
                return stats;
            }
        }
        self.replay::<false>(painter, &mut Vec::new())
    }

    fn replay<const OBSERVE: bool>(
        &self,
        painter: &mut Painter<'_>,
        uses: &mut Vec<evidence::Use>,
    ) -> PaintStats {
        let mut stats = PaintStats::default();
        painter.with_text_fonts(&self.fonts, |painter| {
            for draw in &self.draws {
                match draw {
                    #[cfg(feature = "benchmark")]
                    Draw::Published(chunks, items) => publication::draw(
                        chunks,
                        painter,
                        &mut stats,
                        &mut |key, p| {
                            self.children[&key].draw(p);
                        },
                        &mut |lookup| match lookup {
                            publication::Lookup::Item(c) => items.get(&c.key).cloned(),
                            publication::Lookup::Fragment(program, ctx) => fragments::allowed(ctx)
                                .then(|| self.fragments.get(&program.id()).cloned())
                                .flatten(),
                        },
                    ),
                    Draw::Packet(packet) => {
                        packet.draw(painter);
                    }
                    Draw::Shared(packet, offset) => {
                        painter.translated(*offset, |p| packet.draw(p));
                    }
                    Draw::Image {
                        item,
                        image,
                        origin,
                        clip,
                    } => {
                        if !painter.intersects(*clip) {
                            continue;
                        }
                        painter.viewport(*clip, (0, 0), |painter| {
                            painter.translated(*origin, |painter| {
                                let (width, height) = image.dimensions();
                                painter.draw_image(image, 0, 0, width, height);
                            });
                        });
                        stats.blits += 1;
                        if OBSERVE && let Some(id) = item {
                            let (width, height) = image.dimensions();
                            let extent = crate::PaintExtent {
                                x: 0,
                                y: 0,
                                width,
                                height,
                            };
                            if painter.intersects(extent.visible(*origin, *clip)) {
                                uses.push(evidence::Use::ItemBlit(*id));
                            }
                        }
                    }
                    Draw::Program {
                        program,
                        origin,
                        clip,
                        visible,
                    } => {
                        if !painter.intersects(clip.intersection(*visible)) {
                            continue;
                        }
                        let cull = self.optimized
                            && (painter.has_multiple_clips()
                                || visible
                                    .intersection(painter.visible_bounds().intersection(*clip))
                                    != *visible);
                        if self.optimized {
                            stats.simplified_commands += program.summary().removed;
                        }
                        painter.viewport(*clip, (0, 0), |painter| {
                            program.replay(
                                painter,
                                *origin,
                                &mut |key, painter| {
                                    self.children[&key].draw(painter);
                                },
                                self.optimized,
                                cull,
                                &mut stats,
                                &mut |program, context| {
                                    if !fragments::allowed(context) {
                                        return None;
                                    }
                                    let id = program.id();
                                    let image = self.fragments.get(&id);
                                    if OBSERVE {
                                        uses.push(if image.is_some() {
                                            evidence::Use::FragmentBlit(id)
                                        } else {
                                            evidence::Use::Replay(id)
                                        });
                                    }
                                    image.cloned()
                                },
                            );
                        });
                    }
                }
            }
        });
        stats
    }
}

impl Renderer {
    pub fn freeze(
        &mut self,
        frame: &PaintFrame,
        repair: &Region,
        mut child: impl FnMut(Key, &Region) -> Result<DrawPacket, String>,
    ) -> Result<DrawPacket, String> {
        self.freeze_frame(&frame.freeze(), repair, |frame| {
            frame
                .external_repairs(repair)
                .into_iter()
                .map(|(key, area)| Ok((key, child(key, &area)?)))
                .collect()
        })
    }

    fn freeze_frame(
        &mut self,
        frame: &crate::frame::FrozenFrame,
        repair: &Region,
        children: impl FnOnce(&crate::frame::FrozenFrame) -> Result<BTreeMap<Key, DrawPacket>, String>,
    ) -> Result<DrawPacket, String> {
        if self.prepared_for != Some((frame.identity, frame.generation)) {
            return Err("prepare this frame generation before freezing paint".into());
        }
        let children = children(frame)?;
        #[cfg(feature = "benchmark")]
        if self.optimizations && publication::Trial::active() {
            let mut images = HashMap::new();
            let mut items = rustc_hash::FxHashMap::default();
            let mut draws = Vec::new();
            static COMPACT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *COMPACT.get_or_init(|| std::env::var_os("RETINUE_BENCH_COMPACT_PACKET").is_some()) {
                for c in frame.chunks.iter().filter(|c| repair.intersects(c.visible)) {
                    let id = self.publication.item(c);
                    freeze_instance(
                        c,
                        c.clip,
                        id,
                        &mut self.fragments,
                        &mut self.residency,
                        &mut draws,
                        &mut images,
                        true,
                    );
                }
            } else {
                if self.materializations && publication::Trial::fragments() {
                    for c in frame.chunks.iter().filter(|c| {
                        (c.cache == CachePolicy::Prefer || !c.program.calls().is_empty())
                            && repair.intersects(c.visible)
                    }) {
                        if let Some(id) = self.publication.item(c) {
                            self.residency.drawn(id);
                            items.insert(c.key, self.residency.image(id).clone());
                            continue;
                        }
                        collect(
                            &c.program,
                            &mut self.fragments,
                            &mut self.residency,
                            &mut images,
                            true,
                        );
                    }
                }
                draws.push(Draw::Published(frame.chunks.clone(), items));
            }
            return Ok(DrawPacket {
                feedback: None,
                draws,
                fonts: self.text_fonts.clone(),
                fragments: images,
                children,
                optimized: true,
                repair: Some(repair.clone()),
            });
        }
        let mut draws = Vec::new();
        let mut images = HashMap::new();
        let mut candidate = self.leaf.as_mut().filter(|_| {
            self.optimizations
                && children.is_empty()
                && !repair.is_empty()
                && !self.draw_plan.is_empty()
        });
        let choice = candidate.as_mut().map_or(leaf::Choice::Live, |state| {
            state.choose(repair, &self.text_fonts, || {
                frame.chunks.iter().any(|c| c.program.summary().external)
            })
        });
        let (mut leaf, alternate) = match choice {
            leaf::Choice::Live => (None, None),
            leaf::Choice::Hit(packet) => {
                let mut result = DrawPacket::placed(packet, (0, 0));
                result.repair = Some(repair.clone());
                return Ok(result);
            }
            leaf::Choice::Retain { alternate } => (candidate, alternate),
        };
        // Only widen command selection. Raster preparation, draw clipping, and
        // completed-use feedback must continue to see the actual demand.
        let expanded = leaf
            .as_ref()
            .and_then(|state| state.sweep.map(Region::from));
        let selection = expanded.as_ref().unwrap_or(repair);
        let immediate = leaf.is_none();
        if immediate {
            self.layers.drawn(repair, &mut self.residency);
        }
        for step in self.draw_plan.values() {
            if let DrawStep::Layer { group, .. } = step {
                let layer = &self.layers.prepared[group];
                for &clip in &layer.regions {
                    if selection.intersects(clip) {
                        draws.push(Draw::Image {
                            item: None,
                            image: self.residency.image(layer.raster).clone(),
                            origin: layer.origin,
                            clip,
                        });
                    }
                }
                continue;
            }
            let (index, raster, regions) = match step {
                DrawStep::Item { instance } => (*instance, true, None),
                DrawStep::Replay { instance } => (*instance, false, None),
                DrawStep::Clipped {
                    instance,
                    regions,
                    raster,
                } => (*instance, *raster, Some(regions)),
                DrawStep::Layer { .. } => unreachable!(),
            };
            let chunk = &frame.chunks[index];
            if !selection.intersects(chunk.visible) {
                continue;
            }
            let fallback = [chunk.clip];
            for clip in regions.map_or(fallback.as_slice(), Vec::as_slice) {
                let clip = clip.intersection(chunk.clip);
                if !selection.intersects(clip.intersection(chunk.visible)) {
                    continue;
                }
                freeze_instance(
                    chunk,
                    clip,
                    raster.then(|| self.prepared[index].unwrap()),
                    &mut self.fragments,
                    &mut self.residency,
                    &mut draws,
                    &mut images,
                    immediate,
                );
            }
        }
        let packet = DrawPacket {
            feedback: leaf
                .as_mut()
                .map(|state| state.feedback(&self.layers, repair, alternate)),
            draws,
            fonts: self.text_fonts.clone(),
            fragments: images,
            children,
            optimized: self.optimizations,
            repair: Some(repair.clone()),
        };
        if let Some(state) = leaf {
            let packet = Arc::new(packet);
            state.last = Some(packet.clone());
            Ok(DrawPacket::placed(packet, (0, 0)))
        } else {
            Ok(packet)
        }
    }
}
fn freeze_instance(
    chunk: &crate::PaintInstance,
    clip: Rect,
    raster: Option<RasterId>,
    fragments: &mut Fragments,
    residency: &mut Residency,
    draws: &mut Vec<Draw>,
    images: &mut HashMap<ProgramId, Arc<RasterImage>>,
    immediate: bool,
) {
    if let Some(id) = raster {
        if immediate {
            residency.drawn(id);
        }
        let bounds = chunk.program.bounds();
        draws.push(Draw::Image {
            item: Some(id),
            image: residency.image(id).clone(),
            origin: (chunk.origin.0 + bounds.x, chunk.origin.1 + bounds.y),
            clip,
        });
    } else {
        collect(&chunk.program, fragments, residency, images, immediate);
        draws.push(Draw::Program {
            program: chunk.program.clone(),
            origin: chunk.origin,
            clip,
            visible: chunk.visible,
        });
    }
}

fn collect(
    program: &PaintProgram,
    fragments: &mut Fragments,
    residency: &mut Residency,
    images: &mut HashMap<ProgramId, Arc<RasterImage>>,
    immediate: bool,
) {
    for call in program.calls() {
        if !fragments::allowed(&call.context) {
            continue;
        }
        if let Some(&id) = fragments.selected.get(&call.program.id()) {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                images.entry(call.program.id())
            {
                if immediate {
                    residency.drawn(id);
                }
                entry.insert(residency.image(id).clone());
            }
        } else {
            if immediate {
                fragments.replayed(&call.program);
            }
            collect(&call.program, fragments, residency, images, immediate);
        }
    }
}

#[cfg(test)]
mod tests;
