use retinue_render::{Bounds, CachePolicy, Command, Key, PaintKey, PaintProgram, ProgramStore};
use std::rc::Rc;
use taffy::Size;

#[derive(Clone)]
pub struct Paint {
    /// Content identity including resource generations; not the element's identity.
    pub key: PaintKey,
    pub cache: CachePolicy,
    pub record: Rc<dyn Fn(Size<f32>) -> Rc<Vec<Command>>>,
}

impl PartialEq for Paint {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.cache == other.cache
    }
}

impl Paint {
    pub fn new(
        key: impl Into<PaintKey>,
        record: impl Fn(Size<f32>) -> Vec<Command> + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            cache: CachePolicy::Never,
            record: Rc::new(move |size| Rc::new(record(size))),
        }
    }

    pub fn commands(key: impl Into<PaintKey>, commands: Rc<Vec<Command>>) -> Self {
        Self {
            key: key.into(),
            cache: CachePolicy::Never,
            record: Rc::new(move |_| commands.clone()),
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
    ) -> PaintProgram {
        programs.compile(
            self.key,
            (size.width, size.height),
            scale,
            (self.record)(size),
        )
    }
}
