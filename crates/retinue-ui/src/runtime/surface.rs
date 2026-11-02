use super::ViewTree;
use crate::{
    Rect,
    view::{IntoElement, empty},
};
use retinue_render::{
    Key, LayerPolicy, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};

#[cfg(test)]
mod tests;

pub struct ViewSurface {
    pub tree: ViewTree,
    renderer: Renderer,
    #[cfg(feature = "benchmark")]
    full_layout: bool,
    #[cfg(feature = "benchmark")]
    pub profile: SurfaceProfile,
}

#[cfg(feature = "benchmark")]
#[derive(Default)]
pub struct SurfaceProfile {
    pub frames: u64,
    pub layout_updates: u64,
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
            tree: ViewTree::new(empty(), || {}),
            renderer: Renderer::new(ResidencyPolicy::overlay(0)),
            #[cfg(feature = "benchmark")]
            full_layout: std::env::var_os("RETINUE_BENCH_FULL_LAYOUT").is_some(),
            #[cfg(feature = "benchmark")]
            profile: SurfaceProfile::default(),
        }
    }
}

impl ViewSurface {
    pub fn begin_frame(&self, threads: std::num::NonZeroU32) -> retinue_render::RenderPass {
        self.renderer.host().begin_frame(threads)
    }
    pub fn set_render_host(&mut self, host: retinue_render::RenderHost) {
        self.renderer.set_host(host);
    }
    pub fn has_deferred(&self) -> bool {
        self.renderer.has_deferred()
    }
    pub fn maintenance_deadline(&self) -> Option<std::time::Instant> {
        self.renderer.host().maintenance_deadline()
    }
    pub fn maintain(&self, now: std::time::Instant) {
        self.renderer.host().maintain(now);
    }
    pub fn with_layers(residency: ResidencyPolicy, policy: LayerPolicy) -> Self {
        Self {
            renderer: Renderer::with_layers(residency, policy),
            ..Self::default()
        }
    }
    pub fn clear_cache(&mut self) {
        self.renderer.clear_cache();
    }
    pub fn residency_stats(&self) -> retinue_render::ResidencyStats {
        self.renderer.residency_stats()
    }

    pub fn draw(
        &mut self,
        view: impl IntoElement,
        painter: &mut Painter<'_>,
        repair: Rect,
        external: impl FnMut(Key, &mut Painter<'_>),
    ) -> Result<Damage, &'static str> {
        let _pass = self.renderer.host().begin_pass(painter.thread_count());
        #[cfg(feature = "benchmark")]
        let start = std::time::Instant::now();
        #[cfg(feature = "benchmark")]
        if self.full_layout {
            self.tree = ViewTree::new(empty(), || {});
        }
        self.tree.set_root(view);
        let damage = self.tree.update((painter.width(), painter.height()), 1.0)?;
        #[cfg(feature = "benchmark")]
        let updated = std::time::Instant::now();
        self.renderer.prepare(self.tree.frame(), RasterMode::Replay);
        #[cfg(feature = "benchmark")]
        let prepared = std::time::Instant::now();
        self.renderer
            .draw_with(self.tree.frame(), painter, repair, external);
        #[cfg(feature = "benchmark")]
        {
            self.profile.frames += 1;
            self.profile.layout_updates +=
                u64::from(self.tree.scene().stats.layout_computations > 0);
            self.profile.update_ns += (updated - start).as_nanos() as u64;
            self.profile.prepare_ns += (prepared - updated).as_nanos() as u64;
            self.profile.submit_ns += prepared.elapsed().as_nanos() as u64;
            self.profile.promoted_layers += self.renderer.stats.promoted_layers;
            self.profile.demoted_layers += self.renderer.stats.demoted_layers;
            self.profile.peak_layer_bytes = self
                .profile
                .peak_layer_bytes
                .max(self.renderer.stats.layer_bytes);
            self.profile.simplified_commands += self.renderer.stats.simplified_commands;
        }
        Ok(damage)
    }
}
