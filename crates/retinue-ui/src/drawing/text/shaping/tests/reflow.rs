use super::*;
use parley::{OverflowWrap, editing::Cursor, layout::Affinity};

fn cluster_signature(text: &ShapedText) -> Vec<(std::ops::Range<usize>, Vec<u32>)> {
    let mut clusters: Vec<_> = text
        .layout()
        .lines()
        .flat_map(|line| {
            line.runs().flat_map(|run| {
                run.clusters().map(|cluster| {
                    (
                        cluster.text_range(),
                        cluster.glyphs().map(|glyph| glyph.id).collect(),
                    )
                })
            })
        })
        .collect();
    clusters.sort_by_key(|(range, _)| range.start);
    clusters
}

fn assert_positions(text: &ShapedText) {
    let expected: Vec<_> = text
        .layout()
        .lines()
        .flat_map(|line| {
            line.items().flat_map(|item| match item {
                PositionedLayoutItem::GlyphRun(run) => run.positioned_glyphs().collect(),
                _ => Vec::new(),
            })
        })
        .collect();
    let actual: Vec<_> = text.runs().iter().flat_map(|run| run.glyphs()).collect();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_eq!(actual.id, expected.id);
        assert_eq!((actual.x, actual.y), (expected.x as f64, expected.y as f64));
    }
}

#[test]
fn wrapping_and_alignment_reuse_shaping_and_registered_fonts() {
    let text = "office cafe\u{301} - fractional placement and wrapping";
    let mut shaped = shape(text, None);
    let signature = cluster_signature(&shaped);
    let font = shaped.runs()[0].font().identity();
    let widths = shaped.layout().calculate_content_widths();
    for width in [Some(160.5), Some(350.25), Some(90.0), None] {
        for alignment in [Alignment::Start, Alignment::End, Alignment::Justify] {
            shaped = shaped
                .reflow(width, alignment, AlignmentOptions::default())
                .unwrap();
            assert_eq!(cluster_signature(&shaped), signature);
            assert!(
                shaped
                    .runs()
                    .iter()
                    .all(|run| run.font().identity() == font)
            );
            assert_eq!(shaped.layout().calculate_content_widths().min, widths.min);
            assert_eq!(shaped.layout().calculate_content_widths().max, widths.max);
            assert_positions(&shaped);
        }
    }
}

#[test]
fn identical_placement_retains_the_positioned_arrays() {
    let text = shape("office cafe\u{301}", None)
        .reflow(Some(185.5), Alignment::Center, AlignmentOptions::default())
        .unwrap();
    let runs = text.runs().to_vec();
    let text = text
        .reflow(Some(185.5), Alignment::Center, AlignmentOptions::default())
        .unwrap();
    for (old, new) in runs.iter().zip(text.runs()) {
        assert_eq!(old.glyphs().as_ptr(), new.glyphs().as_ptr());
    }
}

#[test]
fn changing_alignment_options_invalidates_placement() {
    let text = shape("office cafe\u{301}", None)
        .reflow(Some(185.5), Alignment::Justify, AlignmentOptions::default())
        .unwrap();
    let old = text.runs().to_vec();
    let options = AlignmentOptions {
        last_line_alignment: Some(Alignment::End),
        ..AlignmentOptions::default()
    };
    let text = text
        .reflow(Some(185.5), Alignment::Justify, options)
        .unwrap();
    assert_ne!(old[0].glyphs()[0].x, text.runs()[0].glyphs()[0].x);
    assert_positions(&text);
}

#[test]
fn caret_and_baselines_follow_reflowed_glyphs() {
    let source = "alpha beta\ngamma delta";
    let mut text = shape(source, None);
    let cursor = Cursor::from_byte_index(text.layout(), 11, Affinity::Downstream);
    for width in [90.5, 200.25, 64.0] {
        text = text
            .reflow(Some(width), Alignment::Center, AlignmentOptions::default())
            .unwrap();
        let cursor = cursor.refresh(text.layout());
        let rect = cursor.geometry(text.layout(), 1.0);
        let hit = Cursor::from_point(
            text.layout(),
            rect.x0 as f32,
            ((rect.y0 + rect.y1) * 0.5) as f32,
        );
        assert_eq!(cursor.index(), hit.index());
        assert_eq!(cursor.index(), 11);
        assert_eq!(
            text.layout().first_baseline(),
            Some(text.layout().lines().next().unwrap().metrics().baseline)
        );
        assert_eq!(
            text.layout().last_baseline(),
            Some(text.layout().lines().last().unwrap().metrics().baseline)
        );
        assert_positions(&text);
    }
}

#[test]
fn recorded_commands_remain_valid_after_reflow_on_another_thread() {
    let text = shape("office cafe\u{301}", None);
    let old = text.runs().to_vec();
    let commands: Vec<_> = text.commands((0.0, 0.0), Color::rgb(0xffffff)).collect();
    let text = std::thread::spawn(move || {
        text.reflow(Some(70.0), Alignment::End, AlignmentOptions::default())
            .unwrap()
    })
    .join()
    .unwrap();
    assert!(text.layout().len() > 1);
    for (command, original) in commands.iter().zip(&old) {
        let Command::GlyphRun { run, .. } = command else {
            panic!("not native text")
        };
        assert_eq!(run.glyphs().as_ptr(), original.glyphs().as_ptr());
        assert_eq!(run, original);
    }
}

#[test]
fn line_breaks_do_not_split_combining_sequences() {
    let source = "a\u{301}a\u{301}a\u{301}";
    let mut fonts = FontContext::new();
    fonts
        .collection
        .register_fonts(Blob::new(Arc::new(ROBOTO)), None);
    let mut context = LayoutContext::<()>::new();
    let mut builder = context.ranged_builder(&mut fonts, source, 1.0, false);
    builder.push_default(StyleProperty::FontFamily("Roboto".into()));
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    let mut layout = builder.build(source);
    layout.break_all_lines(None);
    let text = ShapedText::from_layout(source, layout)
        .unwrap()
        .reflow(Some(0.0), Alignment::Start, AlignmentOptions::default())
        .unwrap();
    let lines: Vec<_> = text
        .layout()
        .lines()
        .map(|line| &source[line.text_range()])
        .collect();
    assert_eq!(lines, ["a\u{301}", "a\u{301}", "a\u{301}"]);
}

#[test]
fn blank_lines_and_invalid_widths_have_explicit_behavior() {
    let source = "first line\n\nlast line";
    let text = shape(source, None)
        .reflow(Some(500.0), Alignment::Start, AlignmentOptions::default())
        .unwrap();
    assert_eq!(text.layout().len(), 3);
    assert_eq!(text.layout().lines().nth(1).unwrap().text_range().len(), 1);
    for width in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(
            shape(source, None)
                .reflow(Some(width), Alignment::Start, AlignmentOptions::default())
                .is_err()
        );
    }
}
