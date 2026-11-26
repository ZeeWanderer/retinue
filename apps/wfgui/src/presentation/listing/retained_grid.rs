use super::*;

pub(super) const GRID: Key = Key::new("listing.grid.child", 0);

#[derive(Clone, PartialEq)]
struct Slot(State<Metrics>);

#[view]
impl View for Slot {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        place(
            Canvas(retinue_ui::drawing::ExternalDrawing(GRID)),
            self.0.get().viewport(),
        )
        .named(VIEWPORT)
    }
}

impl ListingController {
    pub(super) fn mount_grid(&mut self, ui: &mut ControlHost) {
        let root = self.view_with_grid(ui, Slot(self.model.metrics.clone()));
        ui.install_compiled(root);
        let model = Model {
            metrics: State::new(self.model.metrics.get()),
            offset: State::new(0.),
            ..self.model.clone()
        };
        let metrics = self.model.metrics.clone();
        let offset = self.model.offset.clone();
        let staged_model = model.clone();
        let band = std::rc::Rc::new(std::cell::Cell::new(0));
        let staged_band = band.clone();
        let mount = ui.mount_fixed_view(
            GRID,
            grid_view::GridView {
                model: model.clone(),
                fonts: Fonts::new(ui),
            },
            move |page| {
                let metrics = metrics.read();
                let offset = (offset.get() * metrics.scale).round() as u32;
                proposal(&staged_model, &metrics, page, offset, &staged_band).0
            },
        );
        let mut inputs = retinue_ui::input::LocalInputGeometry::default();
        let mut revision = None;
        self.grid_inputs = Some(Box::new(move |metrics, listing, ui, _media| {
            let viewport = metrics.viewport();
            let (size, clip, translation) =
                proposal(&model, metrics, ui.size, listing.offset, &band);
            let mounted = mount.upgrade().expect("bind only after mount publication");
            let mut child = mounted.borrow_mut();
            #[cfg(all(test, feature = "geometry-census"))]
            let layout_span = crate::app::benchmark::entry_cost::start(
                crate::app::benchmark::entry_cost::Stage::Child,
            );
            child.place(size, metrics.scale, clip, translation);
            #[cfg(all(test, feature = "geometry-census"))]
            drop(layout_span);
            #[cfg(all(test, feature = "geometry-census"))]
            let _targets = crate::app::benchmark::entry_cost::start(
                crate::app::benchmark::entry_cost::Stage::Targets,
            );
            let current = (
                child.input_revision(),
                listing.order_revision,
                model.cells.order(),
            );
            if revision.as_ref() != Some(&current) {
                let mut keys = Vec::new();
                for &id in current.2.keys() {
                    let cell = model.cells.item(id).unwrap();
                    let cell = cell.read();
                    match metrics.page {
                        Page::Relics => keys.extend(relics::input_keys(id, &cell)),
                        Page::Mastery => keys.extend(mastery::input_keys(id, &cell)),
                        _ => unreachable!("unsupported listing grid"),
                    }
                }
                inputs = child.inputs(keys);
                revision = Some(current);
            }
            inputs.placed(translation, metrics.scale, clip, viewport)
        }));
    }
}

fn proposal(
    model: &Model,
    metrics: &Metrics,
    page: (u32, u32),
    offset: u32,
    band: &std::cell::Cell<u32>,
) -> ((u32, u32), Rect, (i32, i32)) {
    let clip = metrics.pixel_rect(metrics.viewport());
    if offset < band.get() || offset - band.get() > clip.height {
        band.set(offset.saturating_sub(clip.height / 2));
    }
    let size = (page.0, page.1.saturating_add(clip.height));
    let mut local = metrics.clone();
    local.height = size.1 as f32 / metrics.scale;
    model.offset.set(band.get() as f32 / metrics.scale);
    model.metrics.set(local);
    (size, clip, (0, band.get() as i32 - offset as i32))
}
