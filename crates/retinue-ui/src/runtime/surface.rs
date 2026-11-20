use crate::{
    Rect,
    drawing::{LayeredDrawing, PixelFrame},
};
use retinue_render::{
    Key, LayerPolicy, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
mod tests;

pub struct ViewSurface {
    layers: PixelFrame,
    renderer: Rc<RefCell<Renderer>>,
    #[cfg(feature = "benchmark")]
    rebuild_layers: bool,
    #[cfg(feature = "benchmark")]
    pub profile: SurfaceProfile,
}

#[cfg(feature = "benchmark")]
#[derive(Default)]
pub struct SurfaceProfile {
    pub frames: u64,
    pub layer_updates: u64,
    pub update_ns: u64,
    pub prepare_ns: u64,
    pub submit_ns: u64,
    pub promoted_layers: usize,
    pub demoted_layers: usize,
    pub peak_layer_bytes: usize,
    pub simplified_commands: usize,
}

impl Default for ViewSurface {
    fn default() -> Self {
        Self {
            layers: PixelFrame::default(),
            renderer: Rc::new(RefCell::new(Renderer::new(ResidencyPolicy::overlay(0)))),
            #[cfg(feature = "benchmark")]
            rebuild_layers: std::env::var_os("RETINUE_BENCH_REBUILD_LAYERS").is_some(),
            #[cfg(feature = "benchmark")]
            profile: SurfaceProfile::default(),
        }
    }
}

impl ViewSurface {
    pub fn frame(&self) -> &retinue_render::PaintFrame {
        &self.layers.frame
    }

    pub fn snapshot(&self, background: retinue_render::Color) -> retinue_render::surface::Snapshot {
        let mut snapshot = retinue_render::surface::Snapshot::new(self.frame().size(), background);
        self.record_surface(&mut snapshot);
        snapshot
    }

    pub fn record_surface(&self, snapshot: &mut retinue_render::surface::Snapshot) {
        snapshot.record_rendered(
            self.frame(),
            &self.renderer.borrow(),
            RasterMode::Replay,
            |_, _| false,
        );
    }

    pub fn begin_frame(&self, threads: std::num::NonZeroU32) -> retinue_render::RenderPass {
        self.renderer.borrow().host().begin_frame(threads)
    }
    pub fn render_host(&self) -> retinue_render::RenderHost {
        self.renderer.borrow().host().clone()
    }
    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.renderer.borrow_mut().set_host(host);
    }
    pub fn has_deferred(&self) -> bool {
        self.renderer.borrow().has_deferred()
    }
    pub fn maintenance_deadline(&self) -> Option<std::time::Instant> {
        self.renderer.borrow().host().maintenance_deadline()
    }
    pub fn maintain(&self, now: std::time::Instant) {
        self.renderer.borrow().host().maintain(now);
    }
    pub fn with_layers(residency: ResidencyPolicy, policy: LayerPolicy) -> Self {
        Self {
            renderer: Rc::new(RefCell::new(Renderer::with_layers(residency, policy))),
            ..Self::default()
        }
    }
    pub fn clear_cache(&mut self) {
        self.renderer.borrow_mut().clear_cache();
    }
    pub fn residency_stats(&self) -> retinue_render::ResidencyStats {
        self.renderer.borrow().residency_stats()
    }

    pub fn update(
        &mut self,
        recording: LayeredDrawing,
        size: (u32, u32),
    ) -> Result<Damage, &'static str> {
        #[cfg(feature = "benchmark")]
        let start = std::time::Instant::now();
        #[cfg(feature = "benchmark")]
        if self.rebuild_layers {
            self.layers = PixelFrame::default();
        }
        #[cfg(feature = "benchmark")]
        let previous = self.frame().generation();
        let damage = self.layers.update(recording, size)?;
        #[cfg(feature = "benchmark")]
        {
            self.profile.layer_updates += u64::from(self.frame().generation() != previous);
            self.profile.update_ns += start.elapsed().as_nanos() as u64;
        }
        Ok(damage)
    }

    pub fn prepare_rasters(&mut self, repair: &retinue_render::damage::Region) {
        #[cfg(feature = "benchmark")]
        let updated = std::time::Instant::now();
        self.renderer
            .borrow_mut()
            .prepare_for(self.frame(), RasterMode::Replay, repair);
        #[cfg(feature = "benchmark")]
        {
            self.profile.prepare_ns += updated.elapsed().as_nanos() as u64;
        }
    }

    pub fn draw_prepared(
        &mut self,
        painter: &mut Painter<'_>,
        repair: Rect,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) {
        #[cfg(feature = "benchmark")]
        let prepared = std::time::Instant::now();
        let mut renderer = self.renderer.borrow_mut();
        renderer.draw_with(self.frame(), painter, repair, external);
        #[cfg(feature = "benchmark")]
        {
            self.profile.frames += 1;
            self.profile.submit_ns += prepared.elapsed().as_nanos() as u64;
            self.profile.promoted_layers += renderer.stats.promoted_layers;
            self.profile.demoted_layers += renderer.stats.demoted_layers;
            self.profile.peak_layer_bytes = self
                .profile
                .peak_layer_bytes
                .max(renderer.stats.layer_bytes);
            self.profile.simplified_commands += renderer.stats.simplified_commands;
        }
    }

    pub fn freeze(
        &mut self,
        repair: &retinue_render::damage::Region,
    ) -> Result<retinue_render::DrawPacket, String> {
        self.renderer
            .borrow_mut()
            .freeze(self.frame(), repair, |_, _| {
                Err("unresolved external paint".into())
            })
    }

    pub fn prepare_packet(
        &self,
        repair: &retinue_render::damage::Region,
        wake: retinue_render::Wake,
    ) -> Result<retinue_render::PacketJob, String> {
        retinue_render::PacketJob::prepare(
            self.renderer.clone(),
            self.frame(),
            RasterMode::Replay,
            repair,
            wake,
            |_, _, _| Err("unresolved external paint".into()),
        )
    }

    pub fn draw(
        &mut self,
        recording: LayeredDrawing,
        painter: &mut Painter<'_>,
        repair: Rect,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) -> Result<Damage, &'static str> {
        let damage = self.update(recording, (painter.width(), painter.height()))?;
        self.prepare_rasters(&painter.visible_regions().intersection(repair));
        self.draw_prepared(painter, repair, external);
        Ok(damage)
    }
}
