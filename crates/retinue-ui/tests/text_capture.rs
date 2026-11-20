use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use retinue_render::{
    Color, PaintFrame, PaintInstance, ProgramStore, RasterMode, Rect, Renderer, ResidencyPolicy,
    painter::Painter, text::Blob,
};
use retinue_ui::drawing::text::shaping::ShapedText;
use std::{path::PathBuf, sync::Arc};

#[test]
#[ignore = "text migration captures; set RETINUE_TEXT_CAPTURE_DIR"]
fn text_pipeline_capture() {
    let output = PathBuf::from(std::env::var_os("RETINUE_TEXT_CAPTURE_DIR").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let mut fonts = FontContext::new();
    fonts.collection.register_fonts(
        Blob::new(Arc::new(
            include_bytes!("../../../assets/fonts/roboto/Roboto-Regular.ttf").as_slice(),
        )),
        None,
    );
    let mut sources = Vec::new();
    if let Some(paths) = std::env::var_os("RETINUE_TEXT_CAPTURE_FONTS") {
        for path in std::env::split_paths(&paths) {
            let bytes = std::fs::read(&path).unwrap();
            fonts
                .collection
                .register_fonts(Blob::new(Arc::new(bytes)), None);
            sources.push(path);
        }
    }
    let mut fallback = fonts
        .collection
        .family_names()
        .filter(|name| *name != "Roboto")
        .map(str::to_owned)
        .collect::<Vec<_>>();
    fallback.sort();
    let families = std::iter::once("Roboto".to_owned())
        .chain(fallback)
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let labels = [
        "Galvanized Aptitude 16 / 20",
        "office ffi fi cafe\u{301} 0.25 100%",
        "j g y - fractional baseline",
        "\u{041F}\u{0440}\u{0438}\u{0432}\u{0456}\u{0442} - 120%",
        "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} 123",
        "\u{9AA8} - 100%",
    ];
    let mut context = LayoutContext::<()>::new();
    let mut texts = Vec::new();
    for label in labels {
        let mut builder = context.ranged_builder(&mut fonts, label, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(families.as_str().into()));
        builder.push_default(StyleProperty::FontSize(18.0));
        let mut layout = builder.build(label);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());
        texts.push(ShapedText::from_layout(label, layout).unwrap());
    }
    let mut frames = Vec::new();
    for scale in [1.0, 1.2, 1.75] {
        let width = (400.0 * scale) as u32;
        let height = (300.0 * scale) as u32;
        let mut pixels = vec![0; (width * height * 4) as usize];
        let clip = Rect {
            width,
            height,
            ..Rect::default()
        };
        let mut store = ProgramStore::default();
        let items = texts
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let origin = (18.25, 48.0 + i as f32 * 39.0);
                let program = store
                    .record(
                        i as u64 + 1,
                        (400.0, 300.0),
                        scale,
                        ::std::sync::Arc::new(
                            text.commands(origin, Color::rgb(0xeeeeee)).collect(),
                        ),
                    )
                    .unwrap();
                PaintInstance::new((i as u64 + 1).into(), program, (0, 0), clip)
            })
            .collect();
        let mut frame = PaintFrame::default();
        frame
            .replace((width, height), scale, items, Vec::new())
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        for _ in 0..4 {
            renderer.prepare(&frame, RasterMode::Explicit);
            let mut painter =
                Painter::with_threads(&mut pixels, width, height, 4.try_into().unwrap()).unwrap();
            painter.fill_rounded_rect(0, 0, width, height, 0, [24, 24, 24, 255]);
            renderer.draw_with(&frame, &mut painter, clip, |_, _| unreachable!());
            painter.finish().unwrap();
        }
        frames.push(
            serde_json::json!({"scale":scale, "fragment_blits":renderer.stats.fragment_blits,
            "retained_raster_bytes":renderer.residency_stats().live_bytes}),
        );
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        retinue_render::image::RgbaImage::from_raw(width, height, pixels)
            .unwrap()
            .save(output.join(format!("text-{}.png", (scale * 100.0).round() as u32)))
            .unwrap();
    }
    let rows: Vec<_> = labels.into_iter().zip(&texts).map(|(source, text)| serde_json::json!({
        "text":source, "width":text.layout().width(), "height":text.layout().height(),
        "missing_glyphs":text.runs().iter().flat_map(|run| run.glyphs()).filter(|g| g.id == 0).count(),
        "font_faces":text.runs().iter().map(|run| run.font().identity()).collect::<Vec<_>>()
    })).collect();
    std::fs::write(
        output.join("capture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "families":families, "additional_sources":sources, "rows": rows, "frames":frames,
            "path":"Retained Parley glyph commands with automatic renderer reuse after four repaints."
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "production Text captures; set RETINUE_TEXT_CAPTURE_DIR"]
fn text_view_capture() {
    use retinue_ui::{authoring::*, drawing::text::TextAlignment, ir::InstanceGraph};
    let output = PathBuf::from(std::env::var_os("RETINUE_TEXT_CAPTURE_DIR").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let mut frames = Vec::new();
    for scale in [1.0, 1.2] {
        let body = "Galvanized Aptitude: direct damage per status type affecting the target.";
        let mut tree = InstanceGraph::with_root_body(
            move |_| ui! {
                VStack(16.0) {
                    Text("Native Text / shared measured glyphs").font_size(20.0)
                    HStack(20.0) {
                        VStack(12.0) {
                            Text(body).font_size(18.0).frame(Frame::new().width(240.0))
                            Text("office ffi fi cafe\u{301}").font_size(18.0)
                            Text("\u{041F}\u{0440}\u{0438}\u{0432}\u{0456}\u{0442} - 120%").font_size(18.0)
                        }
                        VStack(12.0) {
                            Text(body).font_size(18.0).line_limit(2).frame(Frame::new().width(240.0))
                            Text("Centered\nwith two lines").font_size(18.0).alignment(TextAlignment::Center).frame(Frame::new().width(240.0))
                            Text("Blank line\n\nis retained").font_size(18.0).alignment(TextAlignment::Trailing).frame(Frame::new().width(240.0))
                        }
                    }
                }.padding(20.0)
            },
            Default::default(),
        ).unwrap();
        let size = ((560.0 * scale) as u32, (300.0 * scale) as u32);
        tree.update_inline(size, scale).unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        renderer.prepare(tree.frame(), RasterMode::Replay);
        let mut pixels = vec![0; (size.0 * size.1 * 4) as usize];
        let mut painter =
            Painter::with_threads(&mut pixels, size.0, size.1, 4.try_into().unwrap()).unwrap();
        renderer.paint(
            tree.frame(),
            &mut painter,
            Rect {
                x: 0,
                y: 0,
                width: size.0,
                height: size.1,
            },
            Color::rgb(0x181818),
        );
        painter.finish().unwrap();
        retinue_render::painter::straight_rgba(&pixels, size.0, size.1)
            .unwrap()
            .save(output.join(format!("text-view-{}.png", (scale * 100.0).round() as u32)))
            .unwrap();
        let missing = tree
            .frame()
            .instances()
            .iter()
            .flat_map(|item| item.program.commands().iter())
            .map(|command| match command {
                retinue_render::Command::GlyphRun { run, .. } => {
                    run.glyphs().iter().filter(|glyph| glyph.id == 0).count()
                }
                _ => panic!("Text must emit native glyph runs"),
            })
            .sum::<usize>();
        assert_eq!(missing, 0);
        frames.push(
            serde_json::json!({"scale":scale, "size":size, "missing_glyphs":missing,
            "prepared_fonts":renderer.stats.prepared_fonts}),
        );
    }
    std::fs::write(
        output.join("text-view.json"),
        serde_json::to_vec_pretty(&frames).unwrap(),
    )
    .unwrap();
}
