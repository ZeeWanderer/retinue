use super::Work;

struct Coefficients {
    solid_pixel: f64,
    blended_pixel: f64,
    sampled_pixel: f64,
    command: f64,
    edge: f64,
    mask_glyph: f64,
    native_glyph: f64,
    serial_geometry: f64,
    serial_glyph: f64,
    workers: u32,
    target_setup: f64,
    target_pixel: f64,
    payback_margin: f64,
}

// Cold caller-only producers versus warm consumers. Relative scores, not timings.
// Calibration and limits: tools/benchmarks/history/2026-11-10-text.md.
const NATIVE: Coefficients = Coefficients {
    solid_pixel: 0.5,
    blended_pixel: 1.0,
    sampled_pixel: 0.1,
    command: 256.0,
    edge: 10_000.0,
    mask_glyph: 512.0,
    native_glyph: 10_000.0,
    serial_geometry: 0.215,
    serial_glyph: 0.275,
    workers: 4,
    target_setup: 2048.0,
    target_pixel: 3.5,
    payback_margin: 1.25,
};

pub(crate) struct CacheCost {
    pub construction: f64,
    pub saving: f64,
}

impl CacheCost {
    pub fn new(work: Work, area: u64, consumer_threads: u32) -> Self {
        Self {
            construction: replay(work, 1) + NATIVE.target_setup + area as f64 * NATIVE.target_pixel,
            saving: replay(work, consumer_threads) - replay(Work::image(area), consumer_threads),
        }
    }

    pub fn required_uses(&self) -> Option<f64> {
        (self.saving > 0.0)
            .then(|| NATIVE.payback_margin * self.construction / self.saving)
            .filter(|uses| uses.is_finite())
    }
}

fn replay(work: Work, threads: u32) -> f64 {
    let workers = f64::from(threads.clamp(1, NATIVE.workers));
    let geometry = NATIVE.serial_geometry + (1.0 - NATIVE.serial_geometry) / workers;
    let glyphs = NATIVE.serial_glyph + (1.0 - NATIVE.serial_glyph) / workers;
    (work.solid * NATIVE.solid_pixel
        + work.blended * NATIVE.blended_pixel
        + work.sampled * NATIVE.sampled_pixel)
        / workers
        + work.commands * NATIVE.command
        + work.edges * NATIVE.edge * geometry
        + work.glyphs * NATIVE.mask_glyph
        + work.native_glyphs * NATIVE.native_glyph * glyphs
}
