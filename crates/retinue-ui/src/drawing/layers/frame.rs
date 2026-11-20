use super::{Layer, LayeredDrawing};
use crate::Rect;
use retinue_render::{
    CacheHint, CachePolicy, OptimizationContext, PaintFrame, PaintGroup, PaintInstance, PaintKey,
    PaintProgram, PaintUpdate, PlacementUpdate, ProgramStore, damage::Damage,
};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(crate) struct PixelFrame {
    pub frame: PaintFrame,
    previous: LayeredDrawing,
    programs: ProgramStore,
}

fn same_content(a: &Layer, b: &Layer) -> bool {
    a.bounds.width == b.bounds.width && a.bounds.height == b.bounds.height && a.drawing == b.drawing
}

impl PixelFrame {
    pub fn update(
        &mut self,
        next: LayeredDrawing,
        size: (u32, u32),
    ) -> Result<Damage, &'static str> {
        if next.size != size {
            return Err("pixel recording size differs from surface");
        }
        let clip = Rect {
            width: size.0,
            height: size.1,
            ..Rect::default()
        };
        let same_topology = self.frame.size() == size
            && next.layers.len() == self.previous.layers.len()
            && next
                .layers
                .iter()
                .zip(&self.previous.layers)
                .all(|(a, b)| a.key == b.key && a.hint == b.hint);
        let generation = self.frame.generation() + 1;
        let mut compile = |layer: &Layer| {
            self.programs.compile(
                PaintKey::new(layer.key, generation),
                (layer.bounds.width as f32, layer.bounds.height as f32),
                1.0,
                layer.drawing.0.clone(),
            )
        };
        let origin = |layer: &Layer| -> Result<_, &'static str> {
            Ok((
                i32::try_from(layer.bounds.x).map_err(|_| "layer x exceeds paint coordinates")?,
                i32::try_from(layer.bounds.y).map_err(|_| "layer y exceeds paint coordinates")?,
            ))
        };
        let damage = if same_topology {
            let mut paints = Vec::new();
            let mut moves = Vec::new();
            for (index, (old, layer)) in self.previous.layers.iter().zip(&next.layers).enumerate() {
                if !same_content(old, layer) {
                    paints.push(PaintUpdate {
                        index,
                        program: compile(layer),
                        cache: CachePolicy::Never,
                    });
                }
                if (old.bounds.x, old.bounds.y) != (layer.bounds.x, layer.bounds.y) {
                    moves.push(PlacementUpdate {
                        index,
                        origin: origin(layer)?,
                        clip,
                    });
                }
            }
            if paints.is_empty() && moves.is_empty() {
                return Ok(Damage::None);
            }
            self.frame.patch_resolved(paints, moves, [])?
        } else {
            let prior: FxHashMap<_, _> = self
                .previous
                .layers
                .iter()
                .zip(self.frame.instances())
                .map(|(l, i)| (l.key, (l, &i.program)))
                .collect();
            let mut instances = Vec::with_capacity(next.layers.len());
            let mut groups = Vec::new();
            for (index, layer) in next.layers.iter().enumerate() {
                let program = prior
                    .get(&layer.key)
                    .filter(|(old, _)| same_content(old, layer))
                    .map_or_else(|| compile(layer), |(_, p)| PaintProgram::clone(p));
                instances.push(PaintInstance::new(layer.key, program, origin(layer)?, clip));
                if layer.hint != CacheHint::Dynamic {
                    groups.push(PaintGroup {
                        key: layer.key,
                        instances: index..index + 1,
                        context: OptimizationContext::default().with(layer.hint),
                    });
                }
            }
            self.frame.replace(size, 1.0, instances, groups)?
        };
        self.previous = next;
        Ok(damage)
    }
}
