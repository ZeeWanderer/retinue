use super::*;
use crate::{
    data::foundry::Descriptor, domain::media::Media, presentation::controls::ControlHost,
    presentation::widgets::place,
};
use retinue_render::{
    image, painter::Painter, painter::RasterImage, resources::ResourceCache, thumbnails::Request,
    thumbnails::Thumbnails,
};

fn render(ui: &ControlHost, media: &Media) -> Vec<u8> {
    let mut pixels = vec![0; ui.size.0 as usize * ui.size.1 as usize * 4];
    let mut painter = Painter::new(&mut pixels, ui.size.0, ui.size.1).unwrap();
    ui.paint(&mut painter, media);
    painter.finish().unwrap();
    pixels
}

#[test]
fn retained_components_match_existing_artwork_at_each_scale() {
    let mut thumbnails = Thumbnails::new(0, 1024 * 1024);
    let mut artwork = ResourceCache::new(1024 * 1024);
    for scale in [1.0_f32, 1.25, 2.0] {
        for owned in [false, true] {
            for builtin in [false, true] {
                let asset = Asset {
                    id: "test".into(),
                    source: if builtin { "builtin" } else { "wfcd" }.into(),
                    image_name: if builtin { "forma.png" } else { "test.png" }.into(),
                };
                let mut legacy = ControlHost::with_faces(&Default::default()).unwrap();
                let mut retained = ControlHost::with_faces(&Default::default()).unwrap();
                let mut media = Media::default();
                let rect = retinue_ui::Rect {
                    x: 20,
                    y: 24,
                    width: (37.0 * scale).round() as u32,
                    height: (37.0 * scale).round() as u32,
                };
                for ui in [&mut legacy, &mut retained] {
                    ui.begin((160, 160), scale);
                }
                legacy.component(rect, Some(&asset), owned);
                legacy.finish();
                retained.install_view(place(
                    ComponentImage {
                        asset: Some(asset.clone()),
                        owned,
                        edge: rect.width,
                        scale,
                    },
                    retinue_ui::Bounds::new(
                        rect.x as f32 / scale,
                        rect.y as f32 / scale,
                        rect.width as f32 / scale,
                        rect.height as f32 / scale,
                    ),
                ));
                retained.update_view(FrameTime::now());
                retained.finish();
                media.assets(vec![Descriptor {
                    id: asset.id,
                    path: "unused.png".into(),
                    digest: "ab".repeat(32),
                }]);
                let requests: Vec<Request> = legacy
                    .requests(&media)
                    .into_iter()
                    .chain(retained.requests(&media))
                    .collect();
                let ready = requests
                    .iter()
                    .map(|request| {
                        let resource = if builtin {
                            thumbnails.prepare(request).unwrap()
                        } else {
                            artwork.get_or_insert_with(request.key, || {
                                RasterImage::from_rgba(image::RgbaImage::from_fn(16, 16, |x, y| {
                                    image::Rgba([x as u8 * 13, y as u8 * 11, 220, 255])
                                }))
                                .unwrap()
                            })
                        };
                        (request.key, resource)
                    })
                    .collect::<Vec<_>>();
                media.accept(&ready, &requests);
                let before = render(&legacy, &media);
                let after = render(&retained, &media);
                let error = before
                    .iter()
                    .zip(&after)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    error <= 2,
                    "scale {scale}, owned {owned}, builtin {builtin}: error {error}"
                );
            }
        }
    }
}
