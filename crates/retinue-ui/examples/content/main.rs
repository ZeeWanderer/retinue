mod card;
mod drawn_card;
mod model;

use retinue_render::{
    Color, ImageSource, RasterMode, Renderer, ResidencyPolicy,
    damage::Damage,
    painter::{Painter, RasterImage, straight_rgba},
};
use retinue_ui::{image::ImageRendition, prelude::*, runtime::ViewTree};
use serde_json::json;
use std::{fs, path::PathBuf, rc::Rc};

fn artwork(color: Color) -> ImageSource {
    Rc::new(
        RasterImage::render(160, 72, |painter| {
            painter.fill_rounded_rect(0, 0, 160, 72, 4, color.bgra());
            painter.fill_rounded_rect(12, 12, 48, 48, 8, Color::rgb(0x375a68).bgra());
            painter.fill_rounded_rect(76, 22, 66, 8, 4, Color::rgb(0xe0e5eb).bgra());
            painter.fill_rounded_rect(76, 40, 46, 8, 4, Color::rgb(0xabb6c1).bgra());
        })
        .unwrap(),
    )
    .into()
}

fn render(
    tree: &ViewTree,
    renderer: &mut Renderer,
    pixels: &mut [u8],
    damage: Damage,
) -> Result<(), Box<dyn std::error::Error>> {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(pixels, size.0, size.1, 2.try_into().unwrap())?;
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x171a1d));
    }
    painter.finish()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "research/retinue/ui-sdk/content/example".into());
    fs::create_dir_all(&output)?;
    let mut report = Vec::new();
    for scale in [1.0, 1.25, 2.0] {
        let image = ImageResource::default();
        let request = image.begin();
        let model = model::Model::new("Retained Card".into(), image.clone());
        let mut composed = ViewTree::new(card::Card(model.clone()).padding(20.0), || {});
        let mut drawn = ViewTree::new(drawn_card::DrawnCard(model.clone()).padding(20.0), || {});
        let size = ((240.0 * scale) as u32, (190.0 * scale) as u32);
        let mut buffers = [
            vec![0; size.0 as usize * size.1 as usize * 4],
            vec![0; size.0 as usize * size.1 as usize * 4],
        ];
        let mut renderers = [
            Renderer::new(ResidencyPolicy::desktop(0)),
            Renderer::new(ResidencyPolicy::desktop(0)),
        ];
        for (index, name) in ["missing", "preview", "refined", "renamed", "failed"]
            .into_iter()
            .enumerate()
        {
            match index {
                1 | 2 => {
                    image
                        .publish(
                            request,
                            ImageRendition {
                                revision: 1,
                                quality: index as u32,
                                pixels: artwork(Color::rgb(if index == 1 {
                                    0x666c72
                                } else {
                                    0x458777
                                })),
                                fresh: index == 2,
                            },
                        )
                        .unwrap();
                }
                3 => {
                    model.title.set("Updated title".into());
                }
                4 => {
                    image.fail(request, "refresh unavailable").unwrap();
                }
                _ => {}
            }
            for (which, tree) in [&mut composed, &mut drawn].into_iter().enumerate() {
                let damage = tree.update(size, scale)?;
                render(
                    tree,
                    &mut renderers[which],
                    &mut buffers[which],
                    if index == 0 { Damage::Full } else { damage },
                )?;
                let mut fresh = vec![0; buffers[which].len()];
                render(
                    tree,
                    &mut Renderer::new(ResidencyPolicy::desktop(0)),
                    &mut fresh,
                    Damage::Full,
                )?;
                assert!(
                    buffers[which] == fresh,
                    "partial repair differs: {name}/{scale}/{which}"
                );
                report.push(json!({ "case": name, "scale": scale, "mode": which,
                    "bodies": tree.stats.bodies, "projected": tree.stats.projected,
                    "layout": tree.scene().stats.layout_computations, "recordings": tree.scene().stats.recordings }));
                straight_rgba(&buffers[which], size.0, size.1)?.save(output.join(format!(
                    "card-{}-{name}-{which}.png",
                    (100.0 * scale) as u32
                )))?;
            }
            assert!(
                buffers[0] == buffers[1],
                "composed and drawn differ: {name}/{scale}"
            );
        }
    }
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("Content captures: {}", output.display());
    Ok(())
}
