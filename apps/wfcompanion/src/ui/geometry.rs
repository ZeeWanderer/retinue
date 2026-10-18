pub(crate) use retinue_overlay::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitTarget {
    Content,
    Close,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HitRegion {
    pub(crate) target: HitTarget,
    pub(crate) bounds: Rect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScreenOutput {
    pub(crate) animation_bounds: Option<Rect>,
    pub(crate) hit_regions: Vec<HitRegion>,
}

impl ScreenOutput {
    pub(crate) fn contains(&self, target: HitTarget, position: (f64, f64)) -> bool {
        self.hit_regions
            .iter()
            .any(|region| region.target == target && region.bounds.contains(position.0, position.1))
    }
}
