use super::evaluate::{
    evaluate_model::EvaluateModel,
    panel::{EvaluatePanel, Frame, Interaction, Menu, Target},
};
use crate::domain::{budget, currency};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum PreviewState {
    Ready,
    Predicting,
    Estimate,
    EstimateError,
    Searching,
    Fetching,
    Results,
    ListingPreview,
    Empty,
    SearchError,
    FetchError,
    Cooldown,
}

pub fn preview(
    output: &std::path::Path,
    input: Option<&std::path::Path>,
    scale: f32,
    (width, height): (u32, u32),
    full_frame: bool,
    state: PreviewState,
    currency_assets: Option<&std::path::Path>,
) -> Result<(), String> {
    if !(640..=7680).contains(&width)
        || !(640..=4320).contains(&height)
        || !scale.is_finite()
        || !(0.5..=3.0).contains(&scale)
    {
        return Err("invalid preview dimensions or scale".into());
    }
    let source = if let Some(path) = input {
        std::fs::read(path).map_err(|e| e.to_string())?
    } else {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/evaluate-preview.json"
        ))
        .to_vec()
    };
    let data: serde_json::Value = serde_json::from_slice(&source).map_err(|e| e.to_string())?;
    let evaluation: crate::data::evaluation::Evaluation =
        serde_json::from_value(data["evaluation"].clone()).map_err(|e| e.to_string())?;
    let mut model = EvaluateModel::new(evaluation.league.clone());
    model.evaluated(0, evaluation);
    if let Some(profile) = data.get("profile") {
        model.apply_profile(serde_json::from_value(profile.clone()).map_err(|e| e.to_string())?);
    }
    model.visible = true;
    match state {
        PreviewState::Ready => {}
        PreviewState::Predicting => model.predicting = true,
        PreviewState::Estimate | PreviewState::EstimateError => {
            model.estimate = Some(
                serde_json::from_value(data.get("estimate").cloned().unwrap_or_else(|| {
                    serde_json::json!({"amount":40,"currency":"exalted",
                    "uncertainty":{"low":30,"high":50,"confidence":75}})
                }))
                .map_err(|e| e.to_string())?,
            );
            if matches!(state, PreviewState::EstimateError) {
                model.error = Some("Trade service unavailable".into());
            }
        }
        PreviewState::Searching => model.loading = true,
        PreviewState::Results
        | PreviewState::ListingPreview
        | PreviewState::Fetching
        | PreviewState::FetchError => {
            let results = data.get("results").ok_or("fixture has no results")?;
            model.results =
                Some(serde_json::from_value(results.clone()).map_err(|e| e.to_string())?);
            if matches!(state, PreviewState::Fetching | PreviewState::FetchError) {
                model.results.as_mut().unwrap().listings.clear();
                model.results.as_mut().unwrap().next_offset = Some(0);
                model.fetching = matches!(state, PreviewState::Fetching);
                if matches!(state, PreviewState::FetchError) {
                    model.fetch_error = Some("Trade service unavailable".into());
                }
            }
        }
        PreviewState::Empty => model.results = Some(Default::default()),
        PreviewState::SearchError => model.error = Some("Trade service unavailable".into()),
        PreviewState::Cooldown => {
            model.budget.observe(
                budget::Snapshot {
                    observed_at_ms: 1,
                    retry_after_ms: 10_000,
                    windows: vec![],
                },
                Instant::now() - Duration::from_millis(300),
            );
            model.retry_at = model.budget.retry_at();
        }
    }
    let mut view = EvaluatePanel::new()?;
    if let Some(path) = currency_assets {
        view.currencies = currency::preview(path, (20.0 * scale).round() as u32)?;
    }
    view.preview_time = Some(1791885600);
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut interaction = Interaction::default();
    if matches!(state, PreviewState::ListingPreview) {
        if model
            .results
            .as_ref()
            .is_none_or(|results| results.listings.len() < 2 || results.listings[1].item.is_none())
        {
            return Err("listing preview needs a second listing with item details".into());
        }
        interaction.hover = Some(Target::Listing(1));
        interaction.tooltip = true;
    }
    interaction.menu = match data["menu"].as_str() {
        Some("settings") => Some(Menu::Settings),
        Some("rarity") => Some(Menu::Rarity),
        Some("status") => Some(Menu::Status),
        Some("currency") => Some(Menu::Currency),
        Some("indexed") => Some(Menu::Indexed),
        _ => None,
    };
    view.paint(
        &mut pixels,
        Frame {
            size: (width, height),
            scale,
            zoom: scale,
            phase: 0.4,
        },
        &model,
        &interaction,
    )?;
    if let Some(edit) = data.get("editor") {
        let index = edit["modifier"]
            .as_u64()
            .ok_or("editor.modifier must be an index")? as usize;
        let field = model
            .filters
            .get_mut(index)
            .ok_or("editor.modifier is out of range")?;
        field.enabled = true;
        let (target, value) = match edit["bound"].as_str() {
            Some("minimum") => (Target::Minimum(index), &field.min),
            Some("maximum") => (Target::Maximum(index), &field.max),
            _ => return Err("editor.bound must be minimum or maximum".into()),
        };
        interaction.focus = Some(target);
        view.begin_edit(target, value, edit["select_all"].as_bool().unwrap_or(false));
        view.paint(
            &mut pixels,
            Frame {
                size: (width, height),
                scale,
                zoom: scale,
                phase: 0.4,
            },
            &model,
            &interaction,
        )?;
    }
    let image = retinue_render::painter::straight_rgba(&pixels, width, height)?;
    let rect = view.bounds;
    if full_frame {
        image.save(output).map_err(|e| e.to_string())?;
        let metadata = serde_json::json!({
            "screen": {"width": width, "height": height}, "scale": scale,
            "state": format!("{state:?}"), "fixture": input,
            "bounds": {"x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height},
        });
        std::fs::write(
            output.with_extension("json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .map_err(|e| e.to_string())
    } else {
        retinue_render::image::imageops::crop_imm(&image, rect.x, rect.y, rect.width, rect.height)
            .to_image()
            .save(output)
            .map_err(|e| e.to_string())
    }
}
