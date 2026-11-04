//! Work descriptions and execution pricing, independent of damage and cache policy.

use crate::paint::Pixels;

mod program;
pub(crate) use program::{leaf, summarize};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Work {
    pub solid: f64,
    pub blended: f64,
    pub sampled: f64,
    pub commands: f64,
    pub edges: f64,
    pub glyphs: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Density([f32; 6]);

impl Work {
    pub fn add(&mut self, other: Self) {
        self.solid += other.solid;
        self.blended += other.blended;
        self.sampled += other.sampled;
        self.commands += other.commands;
        self.edges += other.edges;
        self.glyphs += other.glyphs;
    }

    pub fn density(self, area: u64) -> Density {
        if area == 0 {
            return Density::default();
        }
        Density(
            [
                self.solid,
                self.blended,
                self.sampled,
                self.commands,
                self.edges,
                self.glyphs,
            ]
            .map(|v| (v / area as f64) as f32),
        )
    }

    pub fn image(area: u64) -> Self {
        Self {
            blended: area as f64,
            sampled: area as f64,
            commands: 1.0,
            ..Self::default()
        }
    }

    pub fn difference(self, other: Self) -> Self {
        Self {
            solid: self.solid - other.solid,
            blended: self.blended - other.blended,
            sampled: self.sampled - other.sampled,
            commands: self.commands - other.commands,
            edges: self.edges - other.edges,
            glyphs: self.glyphs - other.glyphs,
        }
    }
}

impl Density {
    pub fn image(area: u64) -> Self {
        Work::image(area).density(area)
    }

    pub fn work(self, area: u64) -> Work {
        let [solid, blended, sampled, commands, edges, glyphs] =
            self.0.map(|v| f64::from(v) * area as f64);
        Work {
            solid,
            blended,
            sampled,
            commands,
            edges,
            glyphs,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum CopyExecution {
    InPlace,
    Queued,
}

#[derive(Clone, Copy)]
pub(crate) struct Execution {
    pub threads: u32,
    pub copy: CopyExecution,
}

struct Coefficients {
    solid_pixel: f64,
    blended_pixel: f64,
    sampled_pixel: f64,
    command: f64,
    edge: f64,
    glyph: f64,
    pixel_workers: u32,
    queued_byte: f64,
    in_place_byte: f64,
    copy_region: f64,
}

// Relative warm-path scores, normalized to one solid pixel. Calibration and
// limitations: tools/benchmarks/history/2026-11-04-cost-calibration.md.
const WARM_NATIVE: Coefficients = Coefficients {
    solid_pixel: 1.0,
    blended_pixel: 2.0,
    sampled_pixel: 1.0,
    command: 4096.0,
    edge: 64.0,
    glyph: 4096.0,
    pixel_workers: 4,
    queued_byte: 0.125,
    in_place_byte: 0.25,
    copy_region: 8192.0,
};

impl Execution {
    pub const SERIAL: Self = Self {
        threads: 1,
        copy: CopyExecution::InPlace,
    };

    pub fn replay(self, work: Work) -> f64 {
        (work.solid * WARM_NATIVE.solid_pixel
            + work.blended * WARM_NATIVE.blended_pixel
            + work.sampled * WARM_NATIVE.sampled_pixel)
            / f64::from(self.threads.clamp(1, WARM_NATIVE.pixel_workers))
            + work.commands * WARM_NATIVE.command
            + work.edges * WARM_NATIVE.edge
            + work.glyphs * WARM_NATIVE.glyph
    }

    pub fn transfer(self, read_bytes: u64, write_bytes: u64, regions: usize) -> f64 {
        let byte = match self.copy {
            CopyExecution::InPlace => WARM_NATIVE.in_place_byte,
            CopyExecution::Queued => WARM_NATIVE.queued_byte,
        };
        (read_bytes as f64 + write_bytes as f64) * byte + regions as f64 * WARM_NATIVE.copy_region
    }
}

pub(crate) fn area(bounds: Pixels) -> u64 {
    u64::from(bounds.width) * u64::from(bounds.height)
}

#[cfg(test)]
mod tests;
