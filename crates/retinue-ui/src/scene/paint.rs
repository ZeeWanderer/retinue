use crate::layout::LayoutResult;
use retinue_render::{Bounds, CachePolicy, Command, Key, PaintKey, PaintProgram, ProgramStore};
use std::rc::Rc;
use taffy::Size;

#[derive(Clone)]
pub struct Paint {
    /// Content identity including resource generations; not the element's identity.
    pub key: PaintKey,
    pub cache: CachePolicy,
    pub uses_layout: bool,
    pub record: Rc<dyn Fn(Size<f32>, LayoutResult<'_>) -> std::sync::Arc<Vec<Command>>>,
}

impl PartialEq for Paint {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.cache == other.cache && self.uses_layout == other.uses_layout
    }
}

impl Paint {
    pub fn new(
        key: impl Into<PaintKey>,
        record: impl Fn(Size<f32>) -> Vec<Command> + 'static,
    ) -> Self {
        let mut paint = Self::with_layout(key, move |size, _| record(size));
        paint.uses_layout = false;
        paint
    }

    pub fn with_layout(
        key: impl Into<PaintKey>,
        record: impl Fn(Size<f32>, LayoutResult<'_>) -> Vec<Command> + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            cache: CachePolicy::Never,
            uses_layout: true,
            record: Rc::new(move |size, layout| std::sync::Arc::new(record(size, layout))),
        }
    }

    pub fn commands(key: impl Into<PaintKey>, commands: std::sync::Arc<Vec<Command>>) -> Self {
        Self {
            key: key.into(),
            cache: CachePolicy::Never,
            uses_layout: false,
            record: Rc::new(move |_, _| commands.clone()),
        }
    }

    /// The host reports changes with Scene::invalidate; live drawing is never cached.
    pub fn external(key: Key) -> Self {
        Self::new(PaintKey::new(key, 0), move |size| {
            vec![Command::External {
                key,
                bounds: Bounds::new(0.0, 0.0, size.width, size.height),
            }]
        })
    }

    pub(super) fn compile(
        &self,
        programs: &mut ProgramStore,
        size: Size<f32>,
        scale: f32,
        layout: LayoutResult<'_>,
    ) -> PaintProgram {
        #[cfg(feature = "profiling")]
        let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Programs);
        let commands = {
            #[cfg(feature = "profiling")]
            let _scope = crate::profiling::Scope::enter(crate::profiling::Phase::Record);
            (self.record)(size, layout)
        };
        programs.compile(self.key, (size.width, size.height), scale, commands)
    }
}
