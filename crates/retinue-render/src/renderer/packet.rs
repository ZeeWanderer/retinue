use super::*;
use crate::{PaintProgram, ProgramId, damage::Region, painter::RasterImage};

mod job;
pub use job::{PacketError, PacketJob, Wake};

enum Draw {
    Packet(Box<DrawPacket>),
    Image {
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
    draws: Vec<Draw>,
    fonts: Arc<crate::text::preparation::PreparedFonts>,
    fragments: HashMap<ProgramId, Arc<RasterImage>>,
    children: BTreeMap<Key, DrawPacket>,
    optimized: bool,
    repair: Option<Region>,
}

impl DrawPacket {
    pub fn stack(packets: impl IntoIterator<Item = Self>) -> Self {
        Self {
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
            draws: vec![Draw::Image {
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
        let mut stats = PaintStats::default();
        if let Some(repair) = &self.repair {
            let visible = painter.visible_regions();
            if visible.intersection_with(repair).area() != visible.area() {
                painter.fail(crate::painter::PainterError::invalid(
                    "packet repair expanded after preparation",
                ));
                return stats;
            }
        }
        painter.with_text_fonts(&self.fonts, |painter| {
            for draw in &self.draws {
                match draw {
                    Draw::Packet(packet) => {
                        packet.draw(painter);
                    }
                    Draw::Image {
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
                            && (painter.visible_regions().rects().len() > 1
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
                                    fragments::allowed(context)
                                        .then(|| self.fragments.get(&program.id()).cloned())
                                        .flatten()
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
        self.freeze_frame(&frame.freeze(), repair, &mut child)
    }

    fn freeze_frame(
        &mut self,
        frame: &crate::frame::FrozenFrame,
        repair: &Region,
        child: &mut impl FnMut(Key, &Region) -> Result<DrawPacket, String>,
    ) -> Result<DrawPacket, String> {
        if self.prepared_for != Some((frame.identity, frame.generation)) {
            return Err("prepare this frame generation before freezing paint".into());
        }
        let mut children = BTreeMap::new();
        for (key, repair) in frame.external_repairs(repair) {
            children.insert(key, child(key, &repair)?);
        }
        let mut draws = Vec::new();
        let mut images = HashMap::new();
        self.layers.drawn(repair, &mut self.residency);
        for step in self.draw_plan.values() {
            if let DrawStep::Layer { group, .. } = step {
                let layer = &self.layers.prepared[group];
                for &clip in &layer.regions {
                    if repair.intersects(clip) {
                        draws.push(Draw::Image {
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
            if !repair.intersects(chunk.visible) {
                continue;
            }
            let fallback = [chunk.clip];
            for clip in regions.map_or(fallback.as_slice(), Vec::as_slice) {
                let clip = clip.intersection(chunk.clip);
                if !repair.intersects(clip.intersection(chunk.visible)) {
                    continue;
                }
                if raster {
                    let id = self.prepared[index].unwrap();
                    self.residency.drawn(id);
                    let bounds = chunk.program.bounds();
                    draws.push(Draw::Image {
                        image: self.residency.image(id).clone(),
                        origin: (chunk.origin.0 + bounds.x, chunk.origin.1 + bounds.y),
                        clip,
                    });
                } else {
                    collect(
                        &chunk.program,
                        &mut self.fragments,
                        &mut self.residency,
                        &mut images,
                    );
                    draws.push(Draw::Program {
                        program: chunk.program.clone(),
                        origin: chunk.origin,
                        clip,
                        visible: chunk.visible,
                    });
                }
            }
        }
        Ok(DrawPacket {
            draws,
            fonts: self.text_fonts.clone(),
            fragments: images,
            children,
            optimized: self.optimizations,
            repair: Some(repair.clone()),
        })
    }
}

fn collect(
    program: &PaintProgram,
    fragments: &mut Fragments,
    residency: &mut Residency,
    images: &mut HashMap<ProgramId, Arc<RasterImage>>,
) {
    for call in program.calls() {
        if !fragments::allowed(&call.context) {
            continue;
        }
        if let Some(&id) = fragments.selected.get(&call.program.id()) {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                images.entry(call.program.id())
            {
                residency.drawn(id);
                entry.insert(residency.image(id).clone());
            }
        } else {
            fragments.replayed(&call.program);
            collect(&call.program, fragments, residency, images);
        }
    }
}

#[cfg(test)]
mod tests;
