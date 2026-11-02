mod dashboard;
mod meter;
mod model;

use retinue_render::{
    Color, RasterMode, Renderer, ResidencyPolicy,
    damage::Damage,
    painter::{Painter, straight_rgba},
};
use retinue_ui::runtime::ViewTree;
use serde_json::json;
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "research/retinue/ui-sdk/view-runtime/example".into());
    fs::create_dir_all(&output)?;
    let mut report = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let model = model::Model::default();
        let mut tree = ViewTree::new(
            dashboard::Dashboard {
                initial: model.clone(),
            },
            || {},
        );
        let size = ((300.0 * scale) as u32, (140.0 * scale) as u32);
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        for (name, filled, details) in [
            ("initial", 2, true),
            ("value", 4, true),
            ("branch", 4, false),
        ] {
            model.filled.set(filled);
            model.details.set(details);
            tree.update(size, scale)?;
            renderer.prepare(tree.frame(), RasterMode::Replay);
            let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
            let mut painter =
                Painter::with_threads(&mut pixels, size.0, size.1, 2.try_into().unwrap())?;
            renderer.paint(
                tree.frame(),
                &mut painter,
                Damage::Full.bounds(size).unwrap(),
                Color::rgb(0x171717),
            );
            painter.finish()?;
            straight_rgba(&pixels, size.0, size.1)?
                .save(output.join(format!("retained-{}-{name}.png", (scale * 100.0) as u32)))?;
            report.push(
                json!({ "case": name, "scale": scale, "bodies": tree.stats.bodies,
                    "mounted": tree.stats.mounted, "unmounted": tree.stats.unmounted,
                    "projected": tree.stats.projected, "nodes": tree.node_count(),
                    "layout_computations": tree.scene().stats.layout_computations,
                    "recordings": tree.scene().stats.recordings,
                }),
            );
        }
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("Retained View captures: {}", output.display());
    Ok(())
}
