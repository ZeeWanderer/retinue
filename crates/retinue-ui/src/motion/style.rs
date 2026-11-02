use crate::transition::Interpolate;
use taffy::{CompactLength, Dimension, LengthPercentage, LengthPercentageAuto, Rect, Size, Style};

fn parameter(from: CompactLength, to: CompactLength, progress: f32) -> Option<(usize, f32)> {
    (from.tag() == to.tag() && to.is_length_or_percentage()).then(|| {
        (
            to.tag(),
            f32::interpolate(&from.value(), &to.value(), progress),
        )
    })
}

fn dimension(from: Dimension, to: Dimension, t: f32) -> Dimension {
    match parameter(from.into_raw(), to.into_raw(), t) {
        Some((CompactLength::LENGTH_TAG, value)) => Dimension::length(value.max(0.0)),
        Some((CompactLength::PERCENT_TAG, value)) => Dimension::percent(value.max(0.0)),
        _ => to,
    }
}

fn length(from: LengthPercentage, to: LengthPercentage, t: f32) -> LengthPercentage {
    match parameter(from.into_raw(), to.into_raw(), t) {
        Some((CompactLength::LENGTH_TAG, value)) => LengthPercentage::length(value.max(0.0)),
        Some((CompactLength::PERCENT_TAG, value)) => LengthPercentage::percent(value.max(0.0)),
        _ => to,
    }
}

fn length_auto(
    from: LengthPercentageAuto,
    to: LengthPercentageAuto,
    t: f32,
) -> LengthPercentageAuto {
    match parameter(from.into_raw(), to.into_raw(), t) {
        Some((CompactLength::LENGTH_TAG, value)) => LengthPercentageAuto::length(value),
        Some((CompactLength::PERCENT_TAG, value)) => LengthPercentageAuto::percent(value),
        _ => to,
    }
}

fn size<T: Copy>(from: Size<T>, to: Size<T>, t: f32, map: fn(T, T, f32) -> T) -> Size<T> {
    Size {
        width: map(from.width, to.width, t),
        height: map(from.height, to.height, t),
    }
}

fn rect<T: Copy>(from: Rect<T>, to: Rect<T>, t: f32, map: fn(T, T, f32) -> T) -> Rect<T> {
    Rect {
        left: map(from.left, to.left, t),
        right: map(from.right, to.right, t),
        top: map(from.top, to.top, t),
        bottom: map(from.bottom, to.bottom, t),
    }
}

pub(super) fn interpolate(from: &Style, to: &Style, t: f32) -> Style {
    Style {
        size: size(from.size, to.size, t, dimension),
        min_size: size(from.min_size, to.min_size, t, dimension),
        max_size: size(from.max_size, to.max_size, t, dimension),
        gap: size(from.gap, to.gap, t, length),
        padding: rect(from.padding, to.padding, t, length),
        border: rect(from.border, to.border, t, length),
        margin: rect(from.margin, to.margin, t, length_auto),
        inset: rect(from.inset, to.inset, t, length_auto),
        flex_basis: dimension(from.flex_basis, to.flex_basis, t),
        flex_grow: f32::interpolate(&from.flex_grow, &to.flex_grow, t).max(0.0),
        flex_shrink: f32::interpolate(&from.flex_shrink, &to.flex_shrink, t).max(0.0),
        ..to.clone()
    }
}
