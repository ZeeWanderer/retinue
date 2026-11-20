use retinue_ui::authoring::*;
pub(super) use retinue_ui::layout::positioned::Positioned;
use retinue_ui::{Bounds, layout::positioned::Placement};

pub(super) fn place(view: impl Body, bounds: Bounds) -> impl Body {
    view.layout_value::<Placement>(Some(bounds))
}
