//! Acquired-buffer rendering shared by native window and overlay hosts.

pub(crate) mod history;
mod queued;
mod snapshot;
pub use snapshot::Snapshot;
#[cfg(any(test, feature = "benchmark"))]
pub mod experiment;
#[cfg(test)]
mod tests;

use crate::painter::Painter;
use crate::{
    damage::{AgeRepaintTracker, Damage, Region, RepaintTracker},
    relocate::Plan,
};
use history::{History, SlotHistory};
use std::num::NonZeroU32;
use std::rc::Rc;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Relocation {
    #[default]
    Auto,
    Disabled,
}

#[cfg(any(test, feature = "benchmark"))]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum CopyBackend {
    #[default]
    Queued,
    Cpu,
    CpuParallel,
}

#[derive(Default)]
pub struct Buffered {
    damage: RepaintTracker,
    history: SlotHistory,
    epoch: u64,
    relocation: Relocation,
    #[cfg(any(test, feature = "benchmark"))]
    copy_backend: CopyBackend,
    #[cfg(any(test, feature = "benchmark"))]
    copy_workers: queued::CopyWorkers,
}

#[derive(Default)]
pub struct Aged {
    damage: AgeRepaintTracker,
    history: History,
    unsubmitted: bool,
    epoch: u64,
    relocation: Relocation,
    #[cfg(any(test, feature = "benchmark"))]
    copy_backend: CopyBackend,
    #[cfg(any(test, feature = "benchmark"))]
    copy_workers: queued::CopyWorkers,
}

/// Commit only after native presentation succeeds. Dropping it leaves the target invalid.
pub struct Submission {
    pub damage: Region,
    target: Target,
    epoch: u64,
    snapshot: Option<Rc<Snapshot>>,
    #[cfg(any(test, feature = "benchmark"))]
    pub copied_pixels: u64,
    #[cfg(any(test, feature = "benchmark"))]
    pub repaired_pixels: u64,
    #[cfg(any(test, feature = "benchmark"))]
    pub planning_ns: u64,
    #[cfg(any(test, feature = "benchmark"))]
    pub copy_ns: u64,
    #[cfg(any(test, feature = "benchmark"))]
    pub paint_submit_ns: u64,
    #[cfg(any(test, feature = "benchmark"))]
    pub finish_ns: u64,
}

enum Target {
    Slot(usize),
    Age,
}

impl Buffered {
    #[allow(clippy::too_many_arguments)]
    pub fn render_queued<'a>(
        &mut self,
        index: usize,
        size: (u32, u32),
        snapshot: Option<Snapshot>,
        pixels: &'a mut [u8],
        source: Option<(usize, &'a [u8])>,
        threads: NonZeroU32,
        paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
    ) -> Result<Option<Submission>, String> {
        let source = source
            .filter(|(from, _)| *from != index)
            .and_then(|(from, pixels)| self.history.at(from).map(|scene| (scene, pixels)));
        let snapshot = snapshot.map(Rc::new);
        validate(size, pixels, snapshot.as_deref())?;
        if let Some(snapshot) = &snapshot {
            self.damage
                .invalidate(snapshot.damage_since(self.history.latest()));
        }
        let Some(repaint) = self.damage.plan(index, size) else {
            return Ok(None);
        };
        let previous = self.history.take(index);
        self.damage.failed(index);
        queued::execute(
            Target::Slot(index),
            self.epoch,
            size,
            snapshot,
            previous.as_deref(),
            source,
            #[cfg(any(test, feature = "benchmark"))]
            self.copy_backend,
            #[cfg(any(test, feature = "benchmark"))]
            &mut self.copy_workers,
            self.relocation,
            repaint,
            pixels,
            threads,
            paint,
        )
        .map(Some)
    }
    pub fn set_relocation(&mut self, relocation: Relocation) {
        self.relocation = relocation;
    }
    pub fn invalidate(&mut self, damage: Damage) {
        self.damage.invalidate(damage);
    }
    pub fn is_dirty(&self) -> bool {
        self.damage.is_dirty()
    }
    pub fn add_buffer(&mut self) {
        self.damage.add_buffer();
        self.history.add_buffer();
    }
    pub fn reset(&mut self) {
        self.damage.reset();
        self.history.clear();
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub fn render(
        &mut self,
        index: usize,
        size: (u32, u32),
        snapshot: Option<Snapshot>,
        pixels: &mut [u8],
        paint: impl FnOnce(&mut [u8], &Region) -> Result<(), String>,
    ) -> Result<Option<Submission>, String> {
        let snapshot = snapshot.map(Rc::new);
        validate(size, pixels, snapshot.as_deref())?;
        if let Some(snapshot) = &snapshot {
            self.damage
                .invalidate(snapshot.damage_since(self.history.latest()));
        }
        let Some(repaint) = self.damage.plan(index, size) else {
            return Ok(None);
        };
        let previous = self.history.take(index);
        self.damage.failed(index);
        execute(
            Target::Slot(index),
            self.epoch,
            snapshot,
            previous.as_deref(),
            self.relocation,
            repaint,
            pixels,
            paint,
        )
        .map(Some)
    }

    pub fn submitted(&mut self, frame: Submission) {
        assert_eq!(self.epoch, frame.epoch, "surface changed before submission");
        let Target::Slot(index) = frame.target else {
            panic!("submission belongs to an aged surface");
        };
        self.damage.submitted(index);
        self.history.submitted(index, frame.snapshot);
    }
}

impl Aged {
    #[allow(clippy::too_many_arguments)]
    pub fn render_queued<'a>(
        &mut self,
        age: u8,
        size: (u32, u32),
        snapshot: Option<Snapshot>,
        pixels: &'a mut [u8],
        source: Option<&'a [u8]>,
        threads: NonZeroU32,
        paint: impl FnOnce(&mut Painter<'a>, &Region) -> Result<(), String>,
    ) -> Result<Option<Submission>, String> {
        if self.unsubmitted {
            self.reset();
        }
        let source =
            source.and_then(|pixels| self.history.latest_owned().map(|scene| (scene, pixels)));
        let snapshot = snapshot.map(Rc::new);
        validate(size, pixels, snapshot.as_deref())?;
        if let Some(snapshot) = &snapshot {
            self.damage
                .invalidate(snapshot.damage_since(self.history.latest()));
        }
        let Some(repaint) = self.damage.plan(age, size) else {
            return Ok(None);
        };
        self.unsubmitted = true;
        queued::execute(
            Target::Age,
            self.epoch,
            size,
            snapshot,
            self.history.at_age(age),
            source,
            #[cfg(any(test, feature = "benchmark"))]
            self.copy_backend,
            #[cfg(any(test, feature = "benchmark"))]
            &mut self.copy_workers,
            self.relocation,
            repaint,
            pixels,
            threads,
            paint,
        )
        .map(Some)
    }
    pub fn set_relocation(&mut self, relocation: Relocation) {
        self.relocation = relocation;
    }
    pub fn invalidate(&mut self, damage: Damage) {
        self.damage.invalidate(damage);
    }
    pub fn is_dirty(&self) -> bool {
        self.damage.is_dirty()
    }
    pub fn reset(&mut self) {
        self.damage.reset();
        self.history.clear();
        self.unsubmitted = false;
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub fn render(
        &mut self,
        age: u8,
        size: (u32, u32),
        snapshot: Option<Snapshot>,
        pixels: &mut [u8],
        paint: impl FnOnce(&mut [u8], &Region) -> Result<(), String>,
    ) -> Result<Option<Submission>, String> {
        if self.unsubmitted {
            self.reset();
        }
        let snapshot = snapshot.map(Rc::new);
        validate(size, pixels, snapshot.as_deref())?;
        if let Some(snapshot) = &snapshot {
            self.damage
                .invalidate(snapshot.damage_since(self.history.latest()));
        }
        let Some(repaint) = self.damage.plan(age, size) else {
            return Ok(None);
        };
        self.unsubmitted = true;
        execute(
            Target::Age,
            self.epoch,
            snapshot,
            self.history.at_age(age),
            self.relocation,
            repaint,
            pixels,
            paint,
        )
        .map(Some)
    }

    pub fn submitted(&mut self, frame: Submission) {
        assert_eq!(self.epoch, frame.epoch, "surface changed before submission");
        assert!(
            matches!(frame.target, Target::Age),
            "submission belongs to a buffered surface"
        );
        self.damage.submitted();
        if let Some(snapshot) = frame.snapshot {
            self.history.submitted(snapshot);
        } else {
            self.history.clear();
        }
        self.unsubmitted = false;
    }
}

fn validate(size: (u32, u32), pixels: &[u8], snapshot: Option<&Snapshot>) -> Result<(), String> {
    let length = (size.0 as usize)
        .checked_mul(size.1 as usize)
        .and_then(|n| n.checked_mul(4));
    if length != Some(pixels.len()) || snapshot.is_some_and(|s| s.size != size) {
        return Err("prepared surface does not match acquired buffer".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute(
    target: Target,
    epoch: u64,
    snapshot: Option<Rc<Snapshot>>,
    previous: Option<&Snapshot>,
    relocation: Relocation,
    repaint: crate::damage::Repaint,
    pixels: &mut [u8],
    paint: impl FnOnce(&mut [u8], &Region) -> Result<(), String>,
) -> Result<Submission, String> {
    #[cfg(any(test, feature = "benchmark"))]
    let started = std::time::Instant::now();
    let plan = snapshot
        .as_ref()
        .filter(|_| relocation != Relocation::Disabled)
        .map(|snapshot| Plan::new(snapshot, previous, &repaint.repair));
    #[cfg(any(test, feature = "benchmark"))]
    let planned = std::time::Instant::now();
    if let Some(plan) = &plan {
        plan.apply(pixels).map_err(str::to_owned)?;
    }
    #[cfg(any(test, feature = "benchmark"))]
    let copied = std::time::Instant::now();
    let repair = plan.as_ref().map_or(&repaint.repair, |p| &p.repair);
    paint(pixels, repair)?;
    Ok(Submission {
        target,
        epoch,
        snapshot,
        damage: repaint.damage,
        #[cfg(any(test, feature = "benchmark"))]
        copied_pixels: plan.as_ref().map_or(0, Plan::copied_pixels),
        #[cfg(any(test, feature = "benchmark"))]
        repaired_pixels: repair.area(),
        #[cfg(any(test, feature = "benchmark"))]
        planning_ns: (planned - started).as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        copy_ns: (copied - planned).as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        paint_submit_ns: copied.elapsed().as_nanos() as u64,
        #[cfg(any(test, feature = "benchmark"))]
        finish_ns: 0,
    })
}
