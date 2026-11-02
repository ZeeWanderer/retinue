use crate::Rect;
use std::{collections::BTreeMap, sync::Arc};

const MAX_RECTS: usize = 256;
const TILE_AXIS: u32 = 32;

/// Disjoint physical rectangles, with conservative tiling for excessive fragmentation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region(Arc<[Rect]>);

impl Region {
    pub fn new(rects: impl IntoIterator<Item = Rect>) -> Self {
        let mut rects: Vec<_> = rects
            .into_iter()
            .map(|r| Rect {
                width: r.width.min(u32::MAX - r.x),
                height: r.height.min(u32::MAX - r.y),
                ..r
            })
            .filter(|r| !r.is_empty())
            .collect();
        if rects.len() <= 1 {
            return Self(rects.into());
        }
        if rects.len() > MAX_RECTS {
            remove_contained(&mut rects);
        }
        Self(normalize(&rects).unwrap_or_else(|| tiled(&rects)).into())
    }

    pub fn rects(&self) -> &[Rect] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn bounds(&self) -> Rect {
        self.0.iter().copied().fold(Rect::default(), Rect::union)
    }

    pub fn area(&self) -> u64 {
        self.0.iter().map(|r| area(*r)).sum()
    }

    pub fn intersects(&self, rect: Rect) -> bool {
        self.0.iter().any(|r| !r.intersection(rect).is_empty())
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return other.clone();
        }
        if other.is_empty()
            || Arc::ptr_eq(&self.0, &other.0)
            || self == other
            || other.0.iter().all(|rect| self.contains_rect(*rect))
        {
            return self.clone();
        }
        Self::new(self.0.iter().chain(other.0.iter()).copied())
    }

    pub(super) fn union_rect(&self, rect: Rect) -> Self {
        if rect.is_empty() || self.contains_rect(rect) {
            return self.clone();
        }
        Self::new(self.0.iter().copied().chain([rect]))
    }

    fn contains_rect(&self, rect: Rect) -> bool {
        self.0
            .iter()
            .any(|existing| existing.intersection(rect) == rect)
    }

    pub fn intersection(&self, rect: Rect) -> Self {
        if self.bounds().intersection(rect) == self.bounds() {
            return self.clone();
        }
        Self::disjoint_intersections(self.0.iter().map(|r| r.intersection(rect)))
    }

    pub fn intersection_with(&self, other: &Self) -> Self {
        if let [rect] = other.rects() {
            return self.intersection(*rect);
        }
        if let [rect] = self.rects() {
            return other.intersection(*rect);
        }
        Self::disjoint_intersections(
            self.0
                .iter()
                .flat_map(|a| other.0.iter().map(|b| a.intersection(*b))),
        )
    }

    fn disjoint_intersections(rects: impl Iterator<Item = Rect>) -> Self {
        let rects: Vec<_> = rects.filter(|r| !r.is_empty()).collect();
        // A clipping intersection must not fill holes through conservative tiling.
        Self(normalize(&rects).unwrap_or(rects).into())
    }
}

impl From<Rect> for Region {
    fn from(rect: Rect) -> Self {
        Self::new([rect])
    }
}

impl From<&Region> for Region {
    fn from(region: &Region) -> Self {
        region.clone()
    }
}

pub(super) fn area(rect: Rect) -> u64 {
    u64::from(rect.width) * u64::from(rect.height)
}

fn remove_contained(rects: &mut Vec<Rect>) {
    rects.sort_unstable_by_key(|r| std::cmp::Reverse((area(*r), r.x, r.y, r.width, r.height)));
    rects.dedup();
    let mut kept = 0;
    for index in 0..rects.len() {
        let rect = rects[index];
        if rects[..kept].iter().any(|r| r.intersection(rect) == rect) {
            continue;
        }
        rects.swap(kept, index);
        kept += 1;
        if kept > MAX_RECTS {
            return;
        }
    }
    rects.truncate(kept);
}

fn normalize(rects: &[Rect]) -> Option<Vec<Rect>> {
    if rects.len() > MAX_RECTS {
        return None;
    }
    let mut ys: Vec<_> = rects.iter().flat_map(|r| [r.y, r.y + r.height]).collect();
    ys.sort_unstable();
    ys.dedup();
    let mut result: Vec<Rect> = Vec::new();
    let mut previous = BTreeMap::<(u32, u32), usize>::new();
    let mut spans = Vec::new();
    for y in ys.windows(2) {
        spans.clear();
        spans.extend(
            rects
                .iter()
                .filter(|r| r.y < y[1] && r.y + r.height > y[0])
                .map(|r| (r.x, r.x + r.width)),
        );
        spans.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for &(left, right) in &spans {
            if let Some(last) = merged.last_mut()
                && left <= last.1
            {
                last.1 = last.1.max(right);
            } else {
                merged.push((left, right));
            }
        }
        let mut current = BTreeMap::new();
        for (left, right) in merged {
            let index = if let Some(&index) = previous.get(&(left, right)) {
                result[index].height += y[1] - y[0];
                index
            } else {
                result.push(Rect {
                    x: left,
                    y: y[0],
                    width: right - left,
                    height: y[1] - y[0],
                });
                if result.len() > MAX_RECTS {
                    return None;
                }
                result.len() - 1
            };
            current.insert((left, right), index);
        }
        previous = current;
    }
    Some(result)
}

fn tiled(rects: &[Rect]) -> Vec<Rect> {
    let bounds = rects.iter().copied().fold(Rect::default(), Rect::union);
    let width = bounds.width.div_ceil(TILE_AXIS).max(1);
    let height = bounds.height.div_ceil(TILE_AXIS).max(1);
    let mut rows = [0u32; TILE_AXIS as usize];
    for rect in rects {
        let left = (rect.x - bounds.x) / width;
        let right = (rect.x + rect.width - bounds.x).div_ceil(width);
        let top = (rect.y - bounds.y) / height;
        let bottom = (rect.y + rect.height - bounds.y).div_ceil(height);
        let mask = (u32::MAX >> (TILE_AXIS - (right - left))) << left;
        for row in &mut rows[top as usize..bottom as usize] {
            *row |= mask;
        }
    }
    let mut result: Vec<Rect> = Vec::new();
    let mut previous = BTreeMap::<(u32, u32), usize>::new();
    for (row, mut bits) in rows.into_iter().enumerate() {
        let top = row as u32 * height;
        let h = bounds.height.saturating_sub(top).min(height);
        let mut current = BTreeMap::new();
        while bits != 0 {
            let start = bits.trailing_zeros();
            let count = (bits >> start).trailing_ones();
            let x = start * width;
            let w = bounds
                .width
                .saturating_sub(x)
                .min(count.saturating_mul(width));
            let key = (x, w);
            let index = if let Some(&index) = previous.get(&key) {
                result[index].height += h;
                index
            } else {
                result.push(Rect {
                    x: bounds.x + x,
                    y: bounds.y + top,
                    width: w,
                    height: h,
                });
                result.len() - 1
            };
            current.insert(key, index);
            let mask = u32::MAX >> (TILE_AXIS - count);
            bits &= !(mask << start);
        }
        previous = current;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn covered_damage_reuses_storage_without_filling_gaps() {
        let region = Region::new([rect(0, 0, 10, 20), rect(20, 0, 10, 20)]);
        let covered = Region::new([rect(1, 1, 3, 5), rect(21, 6, 3, 7)]);
        assert!(Arc::ptr_eq(&region.0, &region.union(&covered).0));
        assert!(Arc::ptr_eq(
            &region.0,
            &region.union_rect(rect(2, 2, 3, 5)).0
        ));
        let crossing = rect(5, 5, 20, 5);
        let expected = Region::new(region.rects().iter().copied().chain([crossing]));
        assert_eq!(region.union_rect(crossing), expected);
        assert_eq!(region.union(&crossing.into()), expected);
        assert_eq!(expected.area(), region.area() + 50);
    }

    #[test]
    fn many_child_rectangles_do_not_force_tiles_over_the_card_gaps() {
        let cards = (0..48)
            .map(|i| rect(i % 4 * 110, i / 4 * 90, 100, 80))
            .collect::<Vec<_>>();
        let children = cards.iter().flat_map(|card| {
            (0..24).map(move |i| rect(card.x + i % 6 * 15, card.y + i / 6 * 15, 9, 9))
        });
        let mut input = cards.iter().copied().chain(children).collect::<Vec<_>>();
        let expected = Region::new(cards);
        assert_eq!(Region::new(input.iter().copied()), expected);
        input.reverse();
        assert_eq!(Region::new(input), expected);
    }

    #[test]
    fn normalization_preserves_exact_coverage_without_overlap() {
        let input = [
            rect(1, 2, 9, 5),
            rect(4, 1, 3, 12),
            rect(20, 20, 3, 3),
            rect(7, 1, 3, 3),
        ];
        let region = Region::new(input);
        let reversed = Region::new(input.into_iter().rev());
        assert_eq!(region, reversed);
        for y in 0..30 {
            for x in 0..30 {
                let actual = region
                    .rects()
                    .iter()
                    .filter(|r| r.contains(x as f64, y as f64))
                    .count();
                let expected = input.iter().any(|r| r.contains(x as f64, y as f64));
                assert_eq!(actual, usize::from(expected), "{x},{y}");
            }
        }
    }

    #[test]
    fn fragmented_regions_are_bounded_and_conservative() {
        let input: Vec<_> = (0..4096)
            .map(|i| rect((i % 64) * 5, (i / 64) * 5, 1, 1))
            .collect();
        let region = Region::new(input.iter().copied());
        assert!(region.rects().len() <= 512);
        assert!(input.iter().all(|r| region.intersects(*r)));
        for (i, a) in region.rects().iter().enumerate() {
            assert!(
                region.rects()[i + 1..]
                    .iter()
                    .all(|b| a.intersection(*b).is_empty())
            );
        }
    }

    #[test]
    fn empty_adjacent_nested_and_saturated_bounds() {
        assert!(Region::new([Rect::default()]).is_empty());
        assert_eq!(
            Region::new([rect(0, 0, 10, 10), rect(10, 0, 10, 10), rect(1, 1, 2, 2)]),
            Region::from(rect(0, 0, 20, 10))
        );
        let max = u32::MAX;
        let region = Region::new([rect(max - 3, max - 3, 9, 9), rect(0, 0, 1, 1)]);
        assert_eq!(region.area(), 10);
        assert_eq!(region.intersection(rect(max - 3, max - 3, 3, 3)).area(), 9);
    }
}
