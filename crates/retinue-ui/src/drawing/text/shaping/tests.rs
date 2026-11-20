use super::*;
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, StyleProperty};
use retinue_render::text::Blob;
use std::sync::Arc;

const ROBOTO: &[u8] = include_bytes!("../../../../../../assets/fonts/roboto/Roboto-Regular.ttf");

mod reflow;

fn shape(text: &str, width: Option<f32>) -> ShapedText {
    let mut fonts = FontContext::new();
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(ROBOTO)), None);
    let mut context = LayoutContext::<()>::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily("Roboto".into()));
    builder.push_default(StyleProperty::FontSize(22.0));
    let mut layout = builder.build(text);
    layout.break_all_lines(width);
    layout.align(Alignment::Start, AlignmentOptions::default());
    ShapedText::from_layout(text, layout).unwrap()
}

#[test]
fn resolved_geometry_and_clusters_are_the_same_ones_that_paint() {
    fn shared<T: Send + Sync>() {}
    shared::<ShapedText>();
    let text = "office cafe\u{301} - fractional placement and wrapping";
    let shaped = shape(text, Some(190.5));
    assert!(shaped.layout().len() > 1);
    let expected: Vec<_> = shaped
        .layout
        .lines()
        .flat_map(|line| {
            line.items().filter_map(|item| {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    return None;
                };
                Some(run.positioned_glyphs().collect::<Vec<_>>())
            })
        })
        .flatten()
        .collect();
    let actual: Vec<_> = shaped.runs().iter().flat_map(|run| run.glyphs()).collect();
    assert_eq!(expected.len(), actual.len());
    for (expected, actual) in expected.iter().zip(actual) {
        assert_eq!(expected.id, actual.id);
        assert_eq!(expected.x as f64, actual.x);
        assert_eq!(expected.y as f64, actual.y);
    }
    let mut ranges: Vec<_> = shaped
        .layout
        .lines()
        .flat_map(|line| {
            line.runs()
                .flat_map(|run| run.clusters().map(|cluster| cluster.text_range()))
        })
        .collect();
    ranges.sort_by_key(|range| range.start);
    assert_eq!(ranges.first().unwrap().start, 0);
    assert_eq!(ranges.last().unwrap().end, text.len());
    assert!(ranges.iter().any(|range| range.len() > 1));
}

#[test]
fn native_text_is_nonblank_at_fractional_origins_and_scales() {
    let shaped = shape("office cafe\u{301}\nBaseline 123", None);
    let prepared = shaped.prepare().unwrap();
    for scale in [1.0, 1.2, 1.75] {
        let mut pixels = vec![0; 480 * 200 * 4];
        let mut painter =
            Painter::with_threads(&mut pixels, 480, 200, 4.try_into().unwrap()).unwrap();
        prepared.draw(&mut painter, (11.25, 12.75), scale, Color::rgb(0xffffff));
        painter.finish().unwrap();
        assert!(pixels.chunks_exact(4).filter(|p| p[3] > 0).count() > 100);
        for run in shaped.runs() {
            assert!(run.ink_bounds().width > 0.0);
            assert!(run.ink_bounds().height > 0.0);
        }
    }
}

#[test]
fn missing_font_is_not_a_successful_empty_paragraph() {
    let mut fonts = FontContext::new();
    let mut context = LayoutContext::<()>::new();
    let builder = context.ranged_builder(&mut fonts, "hello", 1.0, false);
    let mut layout = builder.build("hello");
    layout.break_all_lines(None);
    assert!(ShapedText::from_layout("hello", layout).is_err());
}

#[test]
fn synthetic_style_is_an_explicit_capability_error() {
    let mut fonts = FontContext::new();
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(ROBOTO)), None);
    let mut context = LayoutContext::<()>::new();
    let mut builder = context.ranged_builder(&mut fonts, "hello", 1.0, false);
    builder.push_default(StyleProperty::FontFamily("Roboto".into()));
    builder.push_default(StyleProperty::FontStyle(parley::FontStyle::Italic));
    let mut layout = builder.build("hello");
    layout.break_all_lines(None);
    assert!(matches!(
        ShapedText::from_layout("hello", layout),
        Err(TextError::UnsupportedFont(_))
    ));
}

#[test]
fn empty_and_whitespace_layouts_remain_valid() {
    for text in ["", "   ", "\n", "\n\n"] {
        let shaped = shape(text, Some(180.0));
        shaped.prepare().unwrap();
    }
}

#[test]
fn retained_commands_keep_the_shaped_array_and_match_direct_native_drawing() {
    use retinue_render::{
        PaintFrame, PaintInstance, PaintProgram, RasterMode, Rect, Renderer, ResidencyPolicy,
    };
    let shaped = shape("office cafe\u{301}\nBaseline 123", None);
    let commands: Vec<_> = shaped
        .commands((11.25, 12.75), Color::rgb(0xffffff))
        .collect();
    for (command, run) in commands.iter().zip(shaped.runs()) {
        let Command::GlyphRun { run: recorded, .. } = command else {
            panic!("not a glyph command")
        };
        assert_eq!(run.glyphs().as_ptr(), recorded.glyphs().as_ptr());
    }
    let clip = Rect {
        x: 0,
        y: 0,
        width: 480,
        height: 200,
    };
    for scale in [1.0, 1.2, 1.75] {
        let program = PaintProgram::new(
            1,
            (300.0, 100.0),
            scale,
            ::std::sync::Arc::new(commands.clone()),
        )
        .unwrap();
        let mut frame = PaintFrame::default();
        frame
            .replace(
                (480, 200),
                scale,
                vec![PaintInstance::new(1.into(), program, (0, 0), clip)],
                Vec::new(),
            )
            .unwrap();
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        renderer.set_optimizations(false);
        renderer.prepare(&frame, RasterMode::Replay);
        let mut actual = vec![0; 480 * 200 * 4];
        let mut painter = Painter::new(&mut actual, 480, 200).unwrap();
        renderer.draw_with(&frame, &mut painter, clip, |_, _| unreachable!());
        painter.finish().unwrap();
        let mut expected = vec![0; actual.len()];
        let mut painter = Painter::new(&mut expected, 480, 200).unwrap();
        shaped.prepare().unwrap().draw(
            &mut painter,
            (11.25 * scale as f64, 12.75 * scale as f64),
            scale as f64,
            Color::rgb(0xffffff),
        );
        painter.finish().unwrap();
        assert_eq!(actual, expected);
    }
}
