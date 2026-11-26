use crate::relocate::{area, shifted};
use crate::{
    Color, Command, Key, PaintFrame, PaintProgram, RasterMode, Rect, Renderer,
    damage::{Damage, Region},
    program::Signature,
    renderer::cost::{Estimate, Work},
};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PaintKey {
    frame: crate::frame::Identity,
    occurrence: u32,
    instance: Key,
    segment: u32,
}

pub(crate) struct Paint {
    pub key: PaintKey,
    signature: Option<Signature>,
    pub origin: (i32, i32),
    pub visible: Rect,
    pub uniform: [Rect; 2],
    work: crate::cost::Density,
}

impl Paint {
    pub fn same_content(&self, other: &Self) -> bool {
        self.signature.is_some() && self.signature == other.signature
    }
}

/// Matching ordinals are common even during motion. Index only structural changes.
pub(crate) struct PaintLookup<'a> {
    paint: &'a [Paint],
    index: Option<FxHashMap<PaintKey, usize>>,
}

impl<'a> PaintLookup<'a> {
    pub(crate) fn find(&mut self, ordinal: usize, key: PaintKey) -> Option<(usize, &'a Paint)> {
        if let Some(paint) = self.paint.get(ordinal)
            && paint.key == key
        {
            return Some((ordinal, paint));
        }
        let index = self.index.get_or_insert_with(|| {
            self.paint
                .iter()
                .enumerate()
                .map(|(i, p)| (p.key, i))
                .collect()
        });
        let &i = index.get(&key)?;
        Some((i, &self.paint[i]))
    }
}

/// Ordered surface evidence. No paint programs, images or fonts are retained.
pub struct Snapshot {
    pub(crate) size: (u32, u32),
    background: Color,
    pub(crate) paint: Vec<Paint>,
    work: Vec<Work>,
    occurrences: FxHashMap<crate::frame::Identity, u32>,
    origin: (i32, i32),
    clip: Rect,
    valid: bool,
}

impl Snapshot {
    pub fn new(size: (u32, u32), background: Color) -> Self {
        Self {
            size,
            background,
            paint: Vec::new(),
            work: Vec::new(),
            occurrences: FxHashMap::default(),
            origin: (0, 0),
            clip: Rect {
                width: size.0,
                height: size.1,
                ..Rect::default()
            },
            valid: true,
        }
    }

    pub fn record(&mut self, frame: &PaintFrame, external: impl FnMut(Key, &mut Self) -> bool) {
        self.record_with(frame, None, external);
    }

    /// Record a placed child in the same local coordinates as its packet.
    /// The enclosing external slot's surface clip does not move with the child.
    pub fn translated(&mut self, offset: (i32, i32), record: impl FnOnce(&mut Self)) {
        let saved = self.origin;
        let Some(origin) = add(saved, offset) else {
            self.valid = false;
            return;
        };
        self.origin = origin;
        record(self);
        self.origin = saved;
    }

    /// Include resident replay costs without preparing or retaining any rasters.
    pub fn record_rendered(
        &mut self,
        frame: &PaintFrame,
        renderer: &Renderer,
        mode: RasterMode,
        external: impl FnMut(Key, &mut Self) -> bool,
    ) {
        #[cfg(feature = "benchmark")]
        if std::env::var_os("RETINUE_BENCH_RAW_REPAIR_COST").is_some() {
            self.record(frame, external);
            return;
        }
        self.record_with(frame, Some(&Estimate::new(renderer, frame, mode)), external);
    }

    fn record_with(
        &mut self,
        frame: &PaintFrame,
        estimate: Option<&Estimate<'_>>,
        mut external: impl FnMut(Key, &mut Self) -> bool,
    ) {
        let source = frame.identity;
        let count = self.occurrences.entry(source).or_default();
        let occurrence = *count;
        *count += 1;
        for (index, instance) in frame.instances().iter().enumerate() {
            let Some(origin) = add(self.origin, instance.origin()) else {
                self.valid = false;
                continue;
            };
            let clip = shifted(
                instance.clip(),
                (i64::from(self.origin.0), i64::from(self.origin.1)),
                self.clip,
            );
            let visible = instance.program.bounds().visible(origin, clip);
            if visible.is_empty() {
                continue;
            }
            let key = PaintKey {
                frame: source,
                occurrence,
                instance: instance.key,
                segment: 0,
            };
            if !instance.program.summary().external {
                let work = if let Some(estimate) = estimate {
                    estimate.instance(index, instance, &mut |work| self.local_work(work))
                } else {
                    instance.program.cost(false)
                };
                self.paint.push(Paint {
                    key,
                    signature: Some(instance.program.signature()),
                    origin,
                    visible,
                    uniform: uniform_program(&instance.program, origin, clip),
                    work,
                });
            } else {
                let mut segment = 0;
                self.commands(
                    key,
                    &instance.program,
                    instance.program.commands(),
                    origin,
                    clip,
                    &mut segment,
                    &mut external,
                );
            }
        }
        if let Some(estimate) = estimate {
            for &bounds in &estimate.layers {
                self.local_work(Work {
                    bounds,
                    density: crate::cost::Density::image(area(bounds)),
                });
            }
        }
    }

    fn local_work(&mut self, work: Work) {
        let bounds = shifted(
            work.bounds,
            (i64::from(self.origin.0), i64::from(self.origin.1)),
            self.clip,
        );
        if !bounds.is_empty() {
            self.work.push(Work { bounds, ..work });
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect {
            width: self.size.0,
            height: self.size.1,
            ..Rect::default()
        }
    }

    pub(crate) fn lookup(&self) -> PaintLookup<'_> {
        PaintLookup {
            paint: &self.paint,
            index: None,
        }
    }

    pub fn damage_since(&self, previous: Option<&Self>) -> Damage {
        let Some(old) = previous.filter(|old| self.compatible(old)) else {
            return Damage::Full;
        };
        let mut before = old.lookup();
        let mut after = self.lookup();
        let mut damage = Vec::new();
        let mut last = None;
        for (i, paint) in self.paint.iter().enumerate() {
            let Some((order, prior)) = before.find(i, paint.key) else {
                damage.push(paint.visible);
                continue;
            };
            if last.is_some_and(|last| order <= last) {
                return Damage::Full;
            }
            last = Some(order);
            if !paint.same_content(prior)
                || paint.origin != prior.origin
                || paint.visible != prior.visible
            {
                damage.extend([paint.visible, prior.visible]);
            }
        }
        damage.extend(
            old.paint
                .iter()
                .enumerate()
                .filter_map(|(i, p)| after.find(i, p.key).is_none().then_some(p.visible)),
        );
        Damage::region(Region::new(damage))
    }

    pub(crate) fn compatible(&self, other: &Self) -> bool {
        self.valid && other.valid && self.size == other.size && self.background == other.background
    }

    #[cfg(test)]
    pub(crate) fn repair_cost(&self, region: &Region) -> u64 {
        crate::cost::Execution::SERIAL
            .replay(self.repair_work(region))
            .round() as u64
    }

    pub(crate) fn repair_work(&self, region: &Region) -> crate::cost::Work {
        self.paint
            .iter()
            .map(|paint| Work {
                bounds: paint.visible,
                density: paint.work,
            })
            .chain(self.work.iter().copied())
            .fold(
                crate::cost::Work {
                    solid: region.area() as f64,
                    commands: region.rects().len() as f64,
                    ..Default::default()
                },
                |mut cost, work| {
                    let pixels = region
                        .rects()
                        .iter()
                        .map(|r| area(r.intersection(work.bounds)))
                        .sum::<u64>();
                    cost.add(work.density.work(pixels));
                    cost
                },
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn commands(
        &mut self,
        key: PaintKey,
        program: &PaintProgram,
        commands: &[Command],
        origin: (i32, i32),
        clip: Rect,
        segment: &mut u32,
        external: &mut dyn FnMut(Key, &mut Self) -> bool,
    ) {
        for command in commands {
            let key = PaintKey {
                segment: *segment,
                ..key
            };
            *segment += 1;
            match command {
                Command::Translate { offset, commands } => {
                    let delta = (
                        (offset.0 * program.scale()).round() as i32,
                        (offset.1 * program.scale()).round() as i32,
                    );
                    if let Some(origin) = add(origin, delta) {
                        self.commands(key, program, commands, origin, clip, segment, external);
                    } else {
                        self.valid = false;
                    }
                }
                Command::Clip { bounds, commands } => {
                    self.commands(
                        key,
                        program,
                        commands,
                        origin,
                        bounds.pixels(program.scale()).visible(origin, clip),
                        segment,
                        external,
                    );
                }
                Command::Fragment { program, .. } => {
                    self.commands(
                        key,
                        program,
                        program.commands(),
                        origin,
                        clip,
                        segment,
                        external,
                    );
                }
                Command::External {
                    bounds,
                    key: external_key,
                } => {
                    let bounds = bounds.pixels(program.scale());
                    let visible = bounds.visible(origin, clip);
                    if visible.is_empty() {
                        continue;
                    }
                    let saved = (self.origin, self.clip);
                    let Some(position) = add(origin, (bounds.x, bounds.y)) else {
                        self.valid = false;
                        continue;
                    };
                    self.origin = position;
                    self.clip = visible;
                    let known = external(*external_key, self);
                    (self.origin, self.clip) = saved;
                    if !known {
                        self.paint.push(Paint {
                            key,
                            signature: None,
                            origin,
                            visible,
                            uniform: [Rect::default(); 2],
                            work: crate::cost::Work::image(area(visible)).density(area(visible)),
                        });
                    }
                }
                _ => {
                    let visible = command
                        .bounds()
                        .pixels(program.scale())
                        .visible(origin, clip);
                    if !visible.is_empty() {
                        self.paint.push(Paint {
                            key,
                            signature: Some(program.signature()),
                            origin,
                            visible,
                            uniform: uniform(command, program.scale(), origin, clip),
                            work: crate::cost::leaf(command, program.scale()).density(
                                crate::cost::area(command.bounds().pixels(program.scale())),
                            ),
                        });
                    }
                }
            }
        }
    }
}

fn add(a: (i32, i32), b: (i32, i32)) -> Option<(i32, i32)> {
    Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?))
}

fn uniform(command: &Command, scale: f32, origin: (i32, i32), clip: Rect) -> [Rect; 2] {
    let Command::Fill { bounds, radius, .. } = command else {
        return [Rect::default(); 2];
    };
    let bounds = bounds.pixels(scale);
    let inset = (radius * scale).ceil().max(0.0) as u32;
    [
        crate::PaintExtent {
            x: bounds.x.saturating_add(inset.min(i32::MAX as u32) as i32),
            width: bounds.width.saturating_sub(inset.saturating_mul(2)),
            ..bounds
        },
        crate::PaintExtent {
            y: bounds.y.saturating_add(inset.min(i32::MAX as u32) as i32),
            height: bounds.height.saturating_sub(inset.saturating_mul(2)),
            ..bounds
        },
    ]
    .map(|r| r.visible(origin, clip))
}

fn uniform_program(program: &PaintProgram, origin: (i32, i32), clip: Rect) -> [Rect; 2] {
    fn visit(
        commands: &[Command],
        scale: f32,
        origin: (i32, i32),
        clip: Rect,
        safe: &mut [Rect; 2],
    ) -> Option<()> {
        for command in commands {
            match command {
                Command::Fill { .. } => {
                    let interior = uniform(command, scale, origin, clip);
                    let mut intersections = [
                        safe[0].intersection(interior[0]),
                        safe[0].intersection(interior[1]),
                        safe[1].intersection(interior[0]),
                        safe[1].intersection(interior[1]),
                    ];
                    intersections.sort_unstable_by_key(|r| std::cmp::Reverse(area(*r)));
                    *safe = [
                        intersections[0],
                        intersections
                            .iter()
                            .copied()
                            .find(|r| *r != intersections[0])
                            .unwrap_or_default(),
                    ];
                    if safe.iter().all(|r| r.is_empty()) {
                        return None;
                    }
                }
                Command::Clip { bounds, commands } => visit(
                    commands,
                    scale,
                    origin,
                    bounds.pixels(scale).visible(origin, clip),
                    safe,
                )?,
                Command::Translate { offset, commands } => {
                    let origin = add(
                        origin,
                        (
                            (offset.0 * scale).round() as i32,
                            (offset.1 * scale).round() as i32,
                        ),
                    )?;
                    visit(commands, scale, origin, clip, safe)?;
                }
                Command::Fragment { program, .. } => {
                    visit(program.commands(), program.scale(), origin, clip, safe)?
                }
                _ => return None,
            }
        }
        Some(())
    }
    let mut safe = [clip; 2];
    if visit(program.commands(), program.scale(), origin, clip, &mut safe).is_none() {
        return [Rect::default(); 2];
    }
    safe
}
