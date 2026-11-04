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

pub fn scales(default: &[f64]) -> Vec<f64> {
    match std::env::var("GUI_BENCH_SCALES") {
        Ok(value) => parse_scales(&value).expect("invalid GUI_BENCH_SCALES"),
        Err(std::env::VarError::NotPresent) => default.to_vec(),
        Err(error) => panic!("invalid GUI_BENCH_SCALES: {error}"),
    }
}

fn parse_scales(value: &str) -> Result<Vec<f64>, String> {
    let mut scales = Vec::new();
    for token in value.split(',').map(str::trim) {
        let scale = token
            .parse::<f64>()
            .map_err(|_| format!("expected a scale ratio, got {token:?}"))?;
        if !(0.75..=2.0).contains(&scale) {
            return Err(format!("scale must be between 0.75 and 2.0: {token}"));
        }
        let scale = (scale * 100.0).round() / 100.0;
        if scales.contains(&scale) {
            return Err(format!("duplicate rendered scale: {token}"));
        }
        scales.push(scale);
    }
    Ok(scales)
}

#[test]
fn scale_list_preserves_order_and_report_precision() {
    assert_eq!(
        parse_scales("1.2, 0.9, 1, 1.5").unwrap(),
        [1.2, 0.9, 1.0, 1.5]
    );
    assert_eq!(parse_scales("1.234,1.236").unwrap(), [1.23, 1.24]);
    assert_eq!(parse_scales("0.75,2").unwrap(), [0.75, 2.0]);
}

#[test]
fn scale_list_rejects_invalid_or_duplicate_scales() {
    for value in [
        "",
        "1,",
        "90%",
        "0",
        "-1",
        "NaN",
        "inf",
        "1e100",
        "1e-100",
        "90",
        "0.74",
        "2.01",
        "1,1.0",
        "1,1.00000001",
        "1,1.004",
    ] {
        assert!(parse_scales(value).is_err(), "{value}");
    }
}
