use std::num::NonZeroU32;

pub fn threads() -> NonZeroU32 {
    std::env::var("GUI_BENCH_THREADS")
        .ok()
        .map(|value| {
            let count: u32 = value.parse().expect("GUI_BENCH_THREADS must be an integer");
            assert!((1..=8).contains(&count));
            NonZeroU32::new(count).unwrap()
        })
        .unwrap_or_else(retinue_ui::desktop::render_threads)
}
