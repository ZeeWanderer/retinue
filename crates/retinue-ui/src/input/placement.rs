use crate::{Bounds, Key, Rect, scene::Geometry};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Geometry of named controls and landmarks in a retained subtree, in its own
/// coordinates. Contains no event router, capture, focus or action ordering.
#[derive(Clone, Default)]
pub struct LocalInputGeometry(Arc<FxHashMap<Key, LocalTarget>>);

struct LocalTarget {
    layout: Bounds,
    clip: Bounds,
    hit: Rect,
    interactive: bool,
}

impl FromIterator<(Key, Geometry)> for LocalInputGeometry {
    fn from_iter<I: IntoIterator<Item = (Key, Geometry)>>(items: I) -> Self {
        Self(Arc::new(
            items
                .into_iter()
                .map(|(key, node)| {
                    (
                        key,
                        LocalTarget {
                            layout: node.layout,
                            clip: node.clip,
                            hit: node.hit,
                            interactive: node.interactive,
                        },
                    )
                })
                .collect(),
        ))
    }
}

/// A local geometry publication viewed through a parent clip and translation.
#[derive(Clone)]
pub struct PlacedInputGeometry {
    local: LocalInputGeometry,
    offset: (i32, i32),
    logical_offset: (f32, f32),
    clip: Rect,
    logical_clip: Bounds,
}

impl LocalInputGeometry {
    /// Place without copying any entries. The logical clip controls hit testing;
    /// the corresponding device clip controls reported pixel bounds.
    pub fn placed(
        &self,
        offset: (i32, i32),
        scale: f32,
        clip: Rect,
        logical_clip: Bounds,
    ) -> PlacedInputGeometry {
        assert!(scale.is_finite() && scale > 0.0);
        PlacedInputGeometry {
            local: self.clone(),
            offset,
            logical_offset: (offset.0 as f32 / scale, offset.1 as f32 / scale),
            clip,
            logical_clip,
        }
    }
}

impl PlacedInputGeometry {
    /// Visible device-pixel rectangle for a focus ring or accessibility node.
    /// A known but fully clipped key has empty bounds; an unknown key is absent.
    pub fn hit_bounds(&self, key: Key) -> Option<Rect> {
        let rect = self.local.0.get(&key)?.hit;
        let axis = |start: u32, length: u32, offset: i32, clip: u32, extent: u32| {
            let a = i64::from(start) + i64::from(offset);
            let limit = i64::from(clip.saturating_add(extent));
            let left = a.clamp(i64::from(clip), limit);
            let right = (a + i64::from(length)).clamp(left, limit);
            (left as u32, (right - left) as u32)
        };
        let (x, width) = axis(
            rect.x,
            rect.width,
            self.offset.0,
            self.clip.x,
            self.clip.width,
        );
        let (y, height) = axis(
            rect.y,
            rect.height,
            self.offset.1,
            self.clip.y,
            self.clip.height,
        );
        Some(Rect {
            x,
            y,
            width,
            height,
        })
    }

    /// Bind an action using the unrounded layout and clip for pointer tests.
    pub fn region<K>(&self, key: Key, action: K) -> Option<super::ControlRegion<K>> {
        let node = self.local.0.get(&key)?;
        let shift = |r: Bounds| Bounds {
            x: r.x + self.logical_offset.0,
            y: r.y + self.logical_offset.1,
            ..r
        };
        Some(super::ControlRegion {
            clip: shift(node.clip).intersection(self.logical_clip),
            enabled: node.interactive,
            ..super::ControlRegion::new(action, shift(node.layout))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{ControlRegion, Controls};

    #[test]
    fn moved_child_keeps_identity_and_capture_but_yields_to_later_siblings() {
        let key = Key::new("tile", 3);
        let r = Rect {
            x: 5,
            y: 10,
            width: 25,
            height: 25,
        };
        let logical = Bounds::new(4., 8., 20., 20.);
        let local: LocalInputGeometry = [(
            key,
            Geometry {
                key,
                parent: None,
                layout: logical,
                clip: logical,
                hit: r,
                paint: r,
                interactive: true,
                role: Default::default(),
                window_action: None,
            },
        )]
        .into_iter()
        .collect();
        let clip = Rect {
            x: 0,
            y: 0,
            width: 25,
            height: 25,
        };
        let place = |delta| local.placed(delta, 1.25, clip, Bounds::new(0., 0., 20., 20.));
        let a = place((0, 0));
        let b = place((-10, -5));
        assert!(Arc::ptr_eq(&a.local.0, &b.local.0));
        assert_eq!(
            b.hit_bounds(key),
            Some(Rect {
                x: 0,
                y: 5,
                width: 20,
                height: 20
            })
        );
        assert!(
            place((i32::MIN, i32::MAX))
                .hit_bounds(key)
                .unwrap()
                .is_empty()
        );
        let mut page = Controls::default();
        page.update([a.region(key, 1).unwrap()]);
        page.button(true, (12., 12.));
        page.update([
            b.region(key, 1).unwrap(),
            ControlRegion::new(2, Bounds::new(0., 4., 8., 8.)),
        ]);
        assert_eq!(page.hit((4., 8.)), Some(&2));
        assert_eq!(page.hit((12., 8.)), Some(&1));
        assert!(page.hit((16., 8.)).is_none());
        assert_eq!(page.captured(), Some(&1));
        assert_eq!(page.button(false, (12., 8.)).activated, Some(1));
    }
}
