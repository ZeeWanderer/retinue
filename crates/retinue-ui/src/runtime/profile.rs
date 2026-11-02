#[cfg(feature = "profiling")]
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateProfile {
    pub evaluation: std::time::Duration,
    pub lowering: std::time::Duration,
    pub scene: std::time::Duration,
    pub input: std::time::Duration,
}

macro_rules! timed {
    ($tree:ident, $field:ident, $expression:expr) => {{
        #[cfg(feature = "profiling")]
        let started = std::time::Instant::now();
        let result = $expression;
        #[cfg(feature = "profiling")]
        {
            $tree.profile.$field += started.elapsed();
        }
        result
    }};
}

pub(super) use timed;
