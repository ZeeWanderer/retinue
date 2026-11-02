use super::*;

pub(super) fn candidates(frame: &PaintFrame, group: &PaintGroup) -> Option<Vec<Coverage>> {
    let hint = group.context.get::<TileHint>()?;
    let context = group.collection()?;
    if context.motion == CollectionMotion::Reordering {
        return Some(Vec::new());
    }
    let (width, height) = hint.size;
    if width == 0 || height == 0 {
        return None;
    }
    let chunks = &frame.chunks[group.instances.clone()];
    if chunks.iter().any(|c| c.program.analysis().effects.barrier) {
        return Some(Vec::new());
    }
    let visible = chunks
        .iter()
        .map(|c| c.visible)
        .filter(|r| !r.is_empty())
        .reduce(Rect::union)?;
    let local = |v: u32, origin: i32| i64::from(v) - i64::from(origin);
    let left = local(visible.x, context.origin.0).div_euclid(i64::from(width));
    let top = local(visible.y, context.origin.1).div_euclid(i64::from(height));
    let right = (local(visible.x, context.origin.0) + i64::from(visible.width) - 1)
        .div_euclid(i64::from(width));
    let bottom = (local(visible.y, context.origin.1) + i64::from(visible.height) - 1)
        .div_euclid(i64::from(height));
    // Bad hints must not create an unbounded planner workload.
    let count = (right - left + 1).checked_mul(bottom - top + 1)?;
    if count > 256 {
        return None;
    }
    let mut regions = Vec::with_capacity(count as usize);
    for row in top..=bottom {
        for column in left..=right {
            let x = i32::try_from(column * i64::from(width)).ok()?;
            let y = i32::try_from(row * i64::from(height)).ok()?;
            let content = PaintExtent {
                x,
                y,
                width,
                height,
            };
            let bounds = content.visible(context.origin, visible);
            if !chunks
                .iter()
                .any(|c| !c.visible.intersection(bounds).is_empty())
            {
                continue;
            }
            let origin = (
                context.origin.0.checked_add(x)?,
                context.origin.1.checked_add(y)?,
            );
            regions.push(Coverage {
                id: ScopeId {
                    group: group.key,
                    partition: Partition::Tile {
                        x,
                        y,
                        width,
                        height,
                    },
                },
                instances: group.instances.clone(),
                content,
                origin,
                visible: bounds,
                contextual: true,
            });
        }
    }
    Some(regions)
}
