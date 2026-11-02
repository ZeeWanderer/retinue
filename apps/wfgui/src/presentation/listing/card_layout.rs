pub(super) use retinue_ui::layout::positioned::Positioned;
use retinue_ui::prelude::*;
use retinue_ui::{Bounds, layout::positioned::Placement};

pub(super) fn place(view: impl IntoElement, bounds: Bounds) -> Element {
    view.layout_value::<Placement>(Some(bounds))
}
