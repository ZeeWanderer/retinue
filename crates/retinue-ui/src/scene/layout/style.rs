use std::ops::Deref;
use taffy::{
    Style,
    style::{CompactLength, GridTemplateComponent, GridTemplateRepetition, TrackSizingFunction},
};

pub(in crate::scene) struct NumericStyle(Style);

impl NumericStyle {
    pub(super) fn new(style: Style) -> Self {
        assert!(
            Self::supports(&style),
            "layout calc handles require owner-only evaluation"
        );
        Self(style)
    }

    pub(crate) fn supports(style: &Style) -> bool {
        // Exhaustive: a new upstream field must be classified before worker use.
        let Style {
            dummy,
            display,
            item_is_table,
            item_is_replaced,
            box_sizing,
            direction,
            overflow,
            scrollbar_width,
            position,
            inset,
            size,
            min_size,
            max_size,
            aspect_ratio,
            margin,
            padding,
            border,
            align_items,
            align_self,
            justify_items,
            justify_self,
            align_content,
            justify_content,
            gap,
            flex_direction,
            flex_wrap,
            flex_basis,
            flex_grow,
            flex_shrink,
            grid_template_rows,
            grid_template_columns,
            grid_auto_rows,
            grid_auto_columns,
            grid_auto_flow,
            grid_template_areas,
            grid_template_column_names,
            grid_template_row_names,
            grid_row,
            grid_column,
        } = style;
        fn shared<T: Send + Sync>(_: T) {}
        shared((
            dummy,
            display,
            item_is_table,
            item_is_replaced,
            box_sizing,
            direction,
            overflow,
            scrollbar_width,
            position,
            aspect_ratio,
            align_items,
            align_self,
            justify_items,
            justify_self,
            align_content,
            justify_content,
            flex_direction,
            flex_wrap,
            flex_grow,
            flex_shrink,
            grid_auto_flow,
            grid_template_areas,
            grid_template_column_names,
            grid_template_row_names,
            grid_row,
            grid_column,
        ));
        [
            inset.left.into_raw(),
            inset.right.into_raw(),
            inset.top.into_raw(),
            inset.bottom.into_raw(),
            size.width.into_raw(),
            size.height.into_raw(),
            min_size.width.into_raw(),
            min_size.height.into_raw(),
            max_size.width.into_raw(),
            max_size.height.into_raw(),
            margin.left.into_raw(),
            margin.right.into_raw(),
            margin.top.into_raw(),
            margin.bottom.into_raw(),
            padding.left.into_raw(),
            padding.right.into_raw(),
            padding.top.into_raw(),
            padding.bottom.into_raw(),
            border.left.into_raw(),
            border.right.into_raw(),
            border.top.into_raw(),
            border.bottom.into_raw(),
            gap.width.into_raw(),
            gap.height.into_raw(),
            flex_basis.into_raw(),
        ]
        .into_iter()
        .all(numeric)
            && grid_auto_rows.iter().chain(grid_auto_columns).all(track)
            && grid_template_rows
                .iter()
                .chain(grid_template_columns)
                .all(|component| match component {
                    GridTemplateComponent::Single(value) => track(value),
                    GridTemplateComponent::Repeat(GridTemplateRepetition {
                        count,
                        tracks,
                        line_names,
                    }) => {
                        shared((count, line_names));
                        tracks.iter().all(track)
                    }
                })
    }
}

fn track(value: &TrackSizingFunction) -> bool {
    numeric(value.min.into_raw()) && numeric(value.max.into_raw())
}

fn numeric(value: CompactLength) -> bool {
    matches!(
        value.tag(),
        CompactLength::LENGTH_TAG
            | CompactLength::PERCENT_TAG
            | CompactLength::AUTO_TAG
            | CompactLength::FR_TAG
            | CompactLength::MIN_CONTENT_TAG
            | CompactLength::MAX_CONTENT_TAG
            | CompactLength::FIT_CONTENT_PX_TAG
            | CompactLength::FIT_CONTENT_PERCENT_TAG
    )
}

impl Deref for NumericStyle {
    type Target = Style;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// SAFETY: the only !Send/!Sync fields in this concrete Style are CompactLength's
// tagged pointers. Construction rejects every pointer-handle variant, including
// nested repeated tracks. Other fields are checked above, and no mutable access
// can invalidate this invariant. Numeric tags never dereference their stored bits.
unsafe impl Send for NumericStyle {}
unsafe impl Sync for NumericStyle {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_dimensions_and_tracks_are_supported() {
        let mut style = Style::default();
        style.size.width = taffy::Dimension::percent(0.75);
        style.grid_template_columns = vec![taffy::style_helpers::fr(1.0_f32)];
        assert!(NumericStyle::supports(&style));
        let value = NumericStyle::new(style.clone());
        assert_eq!(*value, style);
    }

    #[test]
    fn pointer_handles_are_rejected_in_dimensions_and_repeated_grid_tracks() {
        let handle = std::ptr::without_provenance(8);
        assert!(!numeric(CompactLength::calc(handle)));
        let mut style = Style::default();
        style.size.width = taffy::Dimension::calc(handle);
        assert!(!NumericStyle::supports(&style));
        style.size.width = taffy::Dimension::auto();
        style.grid_template_columns = vec![GridTemplateComponent::Repeat(GridTemplateRepetition {
            count: taffy::RepetitionCount::Count(2),
            line_names: Vec::new(),
            tracks: vec![TrackSizingFunction {
                min: taffy::MinTrackSizingFunction::length(5.0),
                max: taffy::MaxTrackSizingFunction::calc(handle),
            }],
        })];
        assert!(!NumericStyle::supports(&style));
        assert!(std::panic::catch_unwind(|| NumericStyle::new(style)).is_err());
    }
}
