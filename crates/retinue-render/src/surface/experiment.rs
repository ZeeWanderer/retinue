//! Headless acquired-buffer comparisons. Does not select a shipping backend.

use super::Snapshot;
use crate::{
    Rect,
    damage::{Damage, Region, RepaintTracker},
    painter::{Painter, PixelSource},
    relocate::Plan,
};
use std::{num::NonZeroU32, rc::Rc, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Repair,
    NoRelocate,
    InPlace,
    Previous,
    PreviousRegions,
    PreviousCpu,
    PreviousCpuParallel,
    SnapshotFull,
    SnapshotRegions,
}

pub struct Trial {
    transfer: Transfer,
    size: (u32, u32),
    pixels: Vec<Vec<u8>>,
    scenes: Vec<Option<Rc<Snapshot>>>,
    latest: Option<usize>,
    damage: RepaintTracker,
    scratch: Vec<u8>,
    queued: super::Buffered,
}

#[derive(Default)]
pub struct Sample {
    pub damage: Region,
    pub copied_pixels: u64,
    pub snapshot_pixels: u64,
    pub repaired_pixels: u64,
    pub planning_ns: u64,
    pub snapshot_ns: u64,
    pub copy_submit_ns: u64,
    pub paint_submit_ns: u64,
    pub finish_ns: u64,
}

impl Trial {
    pub fn new(size: (u32, u32), count: usize, transfer: Transfer) -> Self {
        assert!(count > 0);
        let mut damage = RepaintTracker::default();
        let mut queued = super::Buffered::default();
        queued.copy_backend = match transfer {
            Transfer::PreviousCpu => super::CopyBackend::Cpu,
            Transfer::PreviousCpuParallel => super::CopyBackend::CpuParallel,
            _ => super::CopyBackend::Queued,
        };
        if transfer == Transfer::NoRelocate {
            queued.set_relocation(super::Relocation::Disabled);
        }
        for _ in 0..count {
            damage.add_buffer();
            queued.add_buffer();
        }
        Self {
            transfer,
            size,
            pixels: vec![vec![0; size.0 as usize * size.1 as usize * 4]; count],
            scenes: (0..count).map(|_| None).collect(),
            latest: None,
            damage,
            scratch: Vec::new(),
            queued,
        }
    }

    pub fn pixels(&self, index: usize) -> &[u8] {
        &self.pixels[index]
    }

    pub fn bytes(&self) -> usize {
        self.pixels.iter().map(Vec::capacity).sum::<usize>() + self.scratch.capacity()
    }

    pub fn invalidate(&mut self, damage: Damage) {
        if matches!(
            self.transfer,
            Transfer::NoRelocate
                | Transfer::Previous
                | Transfer::PreviousRegions
                | Transfer::PreviousCpu
                | Transfer::PreviousCpuParallel
        ) {
            self.queued.invalidate(damage);
        } else {
            self.damage.invalidate(damage);
        }
    }

    pub fn paint(
        &mut self,
        index: usize,
        scene: Snapshot,
        threads: NonZeroU32,
        mut draw: impl FnMut(&mut Painter<'_>, &Region) -> Result<(), String>,
    ) -> Result<Option<Sample>, String> {
        if scene.size != self.size {
            return Err("trial scene size mismatch".into());
        }
        if matches!(
            self.transfer,
            Transfer::NoRelocate
                | Transfer::Previous
                | Transfer::PreviousRegions
                | Transfer::PreviousCpu
                | Transfer::PreviousCpuParallel
        ) {
            return self.paint_queued(index, scene, threads, draw);
        }
        let start = Instant::now();
        let latest = self.latest.and_then(|n| self.scenes[n].as_deref());
        self.damage.invalidate(scene.damage_since(latest));
        let Some(repaint) = self.damage.plan(index, self.size) else {
            return Ok(None);
        };
        let previous = self.scenes[index].take();
        self.damage.failed(index);
        let plan = (self.transfer != Transfer::Repair)
            .then(|| Plan::new(&scene, previous.as_deref(), &repaint.repair));
        let repair = plan.as_ref().map_or(&repaint.repair, |p| &p.repair);
        let mut sample = Sample {
            damage: repaint.damage,
            repaired_pixels: repair.area(),
            copied_pixels: plan.as_ref().map_or(0, Plan::copied_pixels),
            planning_ns: start.elapsed().as_nanos() as u64,
            ..Sample::default()
        };
        let copy_start = Instant::now();
        let snapshot = matches!(
            self.transfer,
            Transfer::SnapshotFull | Transfer::SnapshotRegions
        ) && sample.copied_pixels > 0;
        if snapshot {
            self.scratch.resize(self.pixels[index].len(), 0);
            let source = PixelSource::new(&self.pixels[index], self.size.0, self.size.1)
                .map_err(|e| e.to_string())?;
            let regions = if self.transfer == Transfer::SnapshotFull {
                Region::from(Rect {
                    width: self.size.0,
                    height: self.size.1,
                    ..Rect::default()
                })
            } else {
                plan.as_ref().unwrap().sources()
            };
            let mut painter =
                Painter::with_threads(&mut self.scratch, self.size.0, self.size.1, threads)
                    .map_err(|e| e.to_string())?;
            for r in regions.rects() {
                painter.copy_source(&source, *r, (r.x, r.y));
            }
            painter.finish().map_err(|e| e.to_string())?;
            sample.snapshot_pixels = regions.area();
            sample.snapshot_ns = copy_start.elapsed().as_nanos() as u64;
        } else if let Some(plan) = &plan {
            plan.apply(&mut self.pixels[index]).map_err(str::to_owned)?;
            sample.copy_submit_ns = copy_start.elapsed().as_nanos() as u64;
        }

        let source = snapshot
            .then_some(self.scratch.as_slice())
            .map(|s| PixelSource::new(s, self.size.0, self.size.1))
            .transpose()
            .map_err(|e| e.to_string())?;
        let mut painter =
            Painter::with_threads(&mut self.pixels[index], self.size.0, self.size.1, threads)
                .map_err(|e| e.to_string())?;
        let submit = Instant::now();
        if let Some(source) = &source {
            plan.as_ref().unwrap().enqueue(source, &mut painter);
        }
        sample.copy_submit_ns += submit.elapsed().as_nanos() as u64;
        painter.repair(repair);
        let submit = Instant::now();
        draw(&mut painter, repair)?;
        sample.paint_submit_ns = submit.elapsed().as_nanos() as u64;
        let finish = Instant::now();
        painter.finish().map_err(|e| e.to_string())?;
        sample.finish_ns = finish.elapsed().as_nanos() as u64;
        self.scenes[index] = Some(Rc::new(scene));
        self.latest = Some(index);
        self.damage.submitted(index);
        Ok(Some(sample))
    }

    fn paint_queued(
        &mut self,
        index: usize,
        scene: Snapshot,
        threads: NonZeroU32,
        mut draw: impl FnMut(&mut Painter<'_>, &Region) -> Result<(), String>,
    ) -> Result<Option<Sample>, String> {
        let (target, source) = if let Some(from) = self.latest.filter(|n| *n != index) {
            let (target, source) = if index < from {
                let (left, right) = self.pixels.split_at_mut(from);
                (&mut left[index], &right[0])
            } else {
                let (left, right) = self.pixels.split_at_mut(index);
                (&mut right[0], &left[from])
            };
            (target, Some((from, source.as_slice())))
        } else {
            (&mut self.pixels[index], None)
        };
        let region_first = self.transfer == Transfer::PreviousRegions;
        let Some(plan) = self.queued.render_queued(
            index,
            self.size,
            Some(scene),
            target,
            source,
            threads,
            |painter, repair| {
                if region_first {
                    for rect in repair.rects() {
                        let region = Region::from(*rect);
                        painter.viewport(*rect, (0, 0), |painter| draw(painter, &region))?;
                    }
                    Ok(())
                } else {
                    draw(painter, repair)
                }
            },
        )?
        else {
            return Ok(None);
        };
        let sample = Sample {
            damage: plan.damage.clone(),
            copied_pixels: plan.copied_pixels,
            repaired_pixels: plan.repaired_pixels,
            planning_ns: plan.planning_ns,
            copy_submit_ns: plan.copy_ns,
            paint_submit_ns: plan.paint_submit_ns,
            finish_ns: plan.finish_ns,
            ..Sample::default()
        };
        self.queued.submitted(plan);
        self.latest = Some(index);
        Ok(Some(sample))
    }
}
