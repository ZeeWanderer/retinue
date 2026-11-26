use super::view_host::Host;
use crate::domain::media::image_store::ImageStore;
use retinue_render::{
    Key, LayerPolicy, PacketJob, RasterMode, Rect, RenderHost, Renderer, ResidencyPolicy, Wake,
    damage::{Damage, Region},
    painter::Painter,
    surface::Snapshot,
};
use retinue_ui::{authoring::View, input::LocalInputGeometry, timeline::FrameTime};
use std::{cell::RefCell, rc::Rc};

pub(crate) type Mount = Rc<RefCell<MountedView>>;

/// A declarative subtree with its own paint frame, not a native surface or input root.
pub(crate) struct MountedView {
    host: Host,
    pub(super) renderer: Rc<RefCell<Renderer>>,
    size: (u32, u32),
    scale: f32,
    clip: Rect,
    offset: (i32, i32),
    #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
    first_layout_polled: bool,
}

impl MountedView {
    pub(super) fn new(view: impl View, images: ImageStore, host: RenderHost) -> Mount {
        let mut renderer = Renderer::with_layers(
            ResidencyPolicy::desktop(16 * 1024 * 1024),
            LayerPolicy::desktop(),
        );
        renderer.set_host(host);
        Rc::new(RefCell::new(Self {
            host: Host::compiled_with_images(view, images),
            renderer: Rc::new(RefCell::new(renderer)),
            size: (0, 0),
            scale: 1.,
            clip: Rect::default(),
            offset: (0, 0),
            #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
            first_layout_polled: false,
        }))
    }

    pub(crate) fn place(&mut self, size: (u32, u32), scale: f32, clip: Rect, offset: (i32, i32)) {
        #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
        // The last poll of a prewarmed child is not a complete first layout.
        let before =
            (self.size == (0, 0) && !self.first_layout_polled).then(|| self.host.scene().profile);
        #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
        let recurring = (crate::app::benchmark::entry_cost::recurring::sampling()
            && !self.host.is_prepared(size, scale))
        .then(|| self.host.scene().profile);
        self.size = size;
        self.scale = scale;
        self.clip = clip;
        self.offset = offset;
        #[cfg(all(test, feature = "geometry-census", feature = "layout-profiling"))]
        let text_capture =
            recurring.map(|_| retinue_ui::profiling::Capture::start_with_text_layout());
        self.host.prepare(size, scale);
        #[cfg(all(test, feature = "geometry-census", feature = "layout-profiling"))]
        if let Some(capture) = text_capture {
            crate::app::benchmark::entry_cost::recurring::record_text(capture.finish().layout);
        }
        #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
        if let Some(previous) = recurring {
            crate::app::benchmark::entry_cost::recurring::record(
                self.host.stats(),
                self.host.profile(),
                self.host.scene().stats.layout_computations,
                previous,
                self.host.scene().profile,
            );
        }
        #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
        if let Some(before) = before {
            crate::app::benchmark::entry_cost::first_child(
                self.host.profile(),
                before,
                self.host.scene().profile,
            );
        }
    }

    pub(crate) fn input_revision(&self) -> u64 {
        self.host.scene().input_revision()
    }

    pub(crate) fn poll_first_layout(
        &mut self,
        size: (u32, u32),
        scale: f32,
        execution: &retinue_ui::runtime::LayoutExecution,
    ) -> bool {
        debug_assert_eq!(self.size, (0, 0));
        if self.host.is_prepared(size, scale) {
            return true;
        }
        #[cfg(all(test, feature = "geometry-census", feature = "profiling"))]
        {
            self.first_layout_polled = true;
        }
        #[cfg(all(test, feature = "geometry-census"))]
        let _span = crate::app::benchmark::entry_cost::start(
            crate::app::benchmark::entry_cost::Stage::Child,
        );
        self.host.execution = Some(execution.clone());
        let ready = self.host.update(size, scale, FrameTime::now()).is_ready();
        #[cfg(all(test, feature = "geometry-census", feature = "layout-profiling"))]
        if ready {
            crate::app::benchmark::entry_cost::CHILD_SOLVE
                .set(self.host.scene().profile.last_worker_window);
        }
        self.host.execution = None;
        ready
    }

    fn page_geometry(&self, mut node: retinue_ui::scene::Geometry) -> retinue_ui::scene::Geometry {
        let shift = |mut r: retinue_ui::Bounds| {
            r.x += self.offset.0 as f32 / self.scale;
            r.y += self.offset.1 as f32 / self.scale;
            r
        };
        node.layout = shift(node.layout);
        node.clip =
            shift(node.clip).intersection(super::render_host::logical(self.clip, self.scale));
        node.hit = self.visible(node.hit);
        node.paint = self.visible(node.paint);
        node
    }

    fn visible(&self, r: Rect) -> Rect {
        retinue_render::PaintExtent {
            x: r.x as i32,
            y: r.y as i32,
            width: r.width,
            height: r.height,
        }
        .visible(self.offset, self.clip)
    }

    pub(super) fn declared_elements(&self, elements: &mut Vec<serde_json::Value>) {
        for instance in self.host.scene().frame().instances() {
            let (x, y) = instance.origin();
            super::report::commands(
                elements,
                instance.program.commands(),
                (
                    x.saturating_add(self.offset.0),
                    y.saturating_add(self.offset.1),
                ),
                self.visible(instance.clip()),
                self.scale,
            );
        }
    }

    pub(super) fn bounds(&self, key: Key) -> Option<retinue_ui::scene::Geometry> {
        self.host.bounds(key).map(|node| self.page_geometry(node))
    }

    pub(super) fn named_geometry(&self) -> impl Iterator<Item = retinue_ui::scene::Geometry> + '_ {
        self.host
            .named_geometry()
            .map(|node| self.page_geometry(node))
    }

    pub(crate) fn inputs(&self, keys: impl IntoIterator<Item = Key>) -> LocalInputGeometry {
        keys.into_iter()
            .filter_map(|key| Some((key, self.host.input_bounds(key)?)))
            .collect()
    }

    fn slot_offset(&self) -> (i32, i32) {
        (
            self.offset.0 - self.clip.x as i32,
            self.offset.1 - self.clip.y as i32,
        )
    }

    pub(super) fn update(&mut self) -> Damage {
        let time = FrameTime::now();
        if !self.host.needs_update() && !self.deadline().is_some_and(|d| d <= time.instant) {
            return Damage::None;
        }
        let std::task::Poll::Ready(damage) = self.host.update(self.size, self.scale, time) else {
            unreachable!("mounted view layout is local")
        };
        Damage::region(
            damage
                .regions(self.size)
                .local((-i64::from(self.offset.0), -i64::from(self.offset.1)))
                .intersection(self.clip),
        )
    }

    pub(super) fn deadline(&self) -> Option<std::time::Instant> {
        self.host.next_deadline()
    }

    pub(super) fn record(&self, scene: &mut Snapshot) {
        scene.translated(self.slot_offset(), |scene| {
            scene.record_rendered(
                self.host.scene().frame(),
                &self.renderer.borrow(),
                RasterMode::Explicit,
                |_, _| false,
            )
        });
    }

    pub(super) fn packet(&self, slot_repair: &Region, wake: Wake) -> Result<PacketJob, String> {
        let offset = self.slot_offset();
        let local = slot_repair.local((offset.0.into(), offset.1.into()));
        PacketJob::prepare(
            self.renderer.clone(),
            self.host.scene().frame(),
            RasterMode::Explicit,
            &local,
            wake,
            |_, _, _| Err("unresolved nested mount".into()),
        )
        .map(|job| job.placed(offset))
    }

    pub(super) fn prepare_rasters(&self, page_repair: &Region) {
        let local = page_repair
            .intersection(self.clip)
            .local((self.offset.0.into(), self.offset.1.into()));
        self.renderer.borrow_mut().prepare_for(
            self.host.scene().frame(),
            RasterMode::Explicit,
            &local,
        );
    }

    pub(super) fn draw(&self, p: &mut Painter<'_>) {
        p.translated(self.slot_offset(), |p| {
            self.renderer.borrow_mut().draw_with(
                self.host.scene().frame(),
                p,
                p.visible_bounds(),
                |_, _| unreachable!("unresolved nested mount"),
            );
        });
    }
}
