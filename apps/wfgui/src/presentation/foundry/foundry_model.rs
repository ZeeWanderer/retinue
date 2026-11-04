use super::{card, filters};
use retinue_render::Bounds;
use retinue_ui::{drawing::DrawList, layout::LayoutSize, layout::grid::UniformGrid, prelude::*};

#[derive(Clone, PartialEq)]
pub(super) struct Model {
    pub area: State<Bounds>,
    pub toolbar: State<Bounds>,
    pub viewport: State<Bounds>,
    pub extent: State<(f32, f32)>,
    pub offset: State<(f32, f32)>,
    pub controls: Items<Bounds>,
    pub overlay_controls: Items<Bounds>,
    pub modal: State<bool>,
    pub component_bounds: State<Option<Bounds>>,
    pub cards: Items<card::Content>,
    pub grid: State<retinue_ui::layout::grid::GridLayout>,
    pub filters: State<Option<filters::declaration::Geometry>>,
    pub filter_offset: State<(f32, f32)>,
    pub toolbar_paint: State<DrawList>,
    pub background_paint: State<DrawList>,
    pub overlays_paint: State<DrawList>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            area: State::default(),
            toolbar: State::default(),
            viewport: State::default(),
            extent: State::new((0.0, 0.0)),
            offset: State::new((0.0, 0.0)),
            controls: Items::default(),
            overlay_controls: Items::default(),
            modal: State::new(false),
            component_bounds: State::new(None),
            cards: Items::default(),
            grid: State::new(
                UniformGrid::new(
                    1,
                    LayoutSize::new(1.0, 1.0),
                    LayoutSize::ZERO,
                    Insets::default(),
                )
                .unwrap()
                .arrange(KeyedOrder::default()),
            ),
            filters: State::new(None),
            filter_offset: State::new((0.0, 0.0)),
            toolbar_paint: State::default(),
            background_paint: State::default(),
            overlays_paint: State::default(),
        }
    }
}
