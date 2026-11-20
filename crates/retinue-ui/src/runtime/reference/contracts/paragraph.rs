use retinue_render::{Bounds, text::TextError};
use retinue_ui::{
    drawing::text::{Paragraph, TextAlignment, TextStyle},
    runtime::ViewTree,
    runtime::reference::prelude::*,
};

#[test]
fn changing_width_restores_full_text_and_matches_fresh_placement() {
    let style = TextStyle::default();
    for text in [
        "office cafe\u{301} has more than one line of text",
        "first line\n\nlast line\n",
        "a\u{301}a\u{301}a\u{301}",
        "",
        "   ",
    ] {
        for limit in [None, Some(0), Some(1), Some(2)] {
            let paragraph = Paragraph::new(text, &style, limit);
            for width in [80.5, 240.25, 0.0, 18.75, 1000.0, 80.5] {
                for alignment in [
                    TextAlignment::Leading,
                    TextAlignment::Center,
                    TextAlignment::Trailing,
                ] {
                    let layout = paragraph.layout(width, alignment).unwrap();
                    let other = Paragraph::new(text, &style, limit);
                    let expected = other.layout(width, alignment).unwrap();
                    assert_eq!(
                        layout.lines().collect::<Vec<_>>(),
                        expected.lines().collect::<Vec<_>>()
                    );
                    assert_eq!(layout.size, expected.size);
                    assert_eq!(layout.baseline, expected.baseline);
                    assert_eq!(layout.last_baseline, expected.last_baseline);
                    assert_eq!(layout.shaped().runs(), expected.shaped().runs());
                    if let Some(limit) = limit {
                        assert!(layout.lines().count() <= limit);
                    }
                }
            }
            if limit != Some(0) {
                let restored = paragraph.layout(1000.0, TextAlignment::Leading).unwrap();
                if limit.is_none_or(|limit| text.split('\n').count() <= limit) {
                    assert_eq!(
                        restored.lines().collect::<Vec<_>>().join("\n"),
                        text.trim_end_matches(' ')
                    );
                }
            }
        }
    }
}

#[test]
fn paragraph_alignment_uses_logical_direction_and_actual_line_baselines() {
    let style = TextStyle::default();
    for (text, rtl) in [
        ("abc\nabc abc", false),
        ("\u{05d0}\u{05d1}\n\u{05d0}\u{05d1}\u{05d2}", true),
    ] {
        let paragraph = Paragraph::new(text, &style, None);
        let width = paragraph.ideal_width().unwrap();
        let bounds = Bounds::new(7.25, 19.5, width + 100.0, 100.0);
        for (alignment, fraction) in [
            (TextAlignment::Leading, if rtl { 1.0 } else { 0.0 }),
            (TextAlignment::Center, 0.5),
            (TextAlignment::Trailing, if rtl { 0.0 } else { 1.0 }),
        ] {
            let layout = paragraph.layout(bounds.width, alignment).unwrap();
            assert_eq!(layout.shaped().layout().is_rtl(), rtl);
            assert_eq!(
                layout.origin(bounds),
                (bounds.x + 100.0 * fraction, bounds.y)
            );
            let lines = layout.shaped().layout().lines().collect::<Vec<_>>();
            assert_eq!(layout.baseline, lines.first().unwrap().metrics().baseline);
            assert_eq!(
                layout.last_baseline,
                lines.last().unwrap().metrics().baseline
            );
            assert!(layout.last_baseline > layout.baseline);
        }
    }
}

#[test]
fn invalid_width_does_not_poison_valid_placement() {
    let paragraph = Paragraph::new("word", &TextStyle::default(), None);
    for width in [-1.0, f32::NAN, f32::NEG_INFINITY] {
        assert!(matches!(
            paragraph.layout(width, TextAlignment::Leading),
            Err(TextError::UnsupportedLayout(_))
        ));
    }
    let layout = paragraph
        .layout(f32::INFINITY, TextAlignment::Leading)
        .unwrap();
    assert_eq!(layout.lines().collect::<Vec<_>>(), ["word"]);
    assert_eq!(layout.size.width, paragraph.ideal_width().unwrap());
}

#[test]
fn measured_width_is_a_stable_placement_proposal() {
    for size in [9.0, 16.0, 18.5, 27.0] {
        let mut style = TextStyle::default();
        style.size = size;
        for text in [
            "Galvanized Aptitude: direct damage per status type affecting the target.",
            "office cafe\u{301}\n\nfirst line and a second line\n",
            "abc \u{05d0}\u{05d1} 123",
        ] {
            for limit in [None, Some(1), Some(2)] {
                let paragraph = Paragraph::new(text, &style, limit);
                for width in [0.0, 1.0, 16.25, 63.5, 88.125, 180.5, 240.25] {
                    let (measured, lines) = {
                        let layout = paragraph.layout(width, TextAlignment::Leading).unwrap();
                        (
                            layout.size,
                            layout.lines().map(str::to_owned).collect::<Vec<_>>(),
                        )
                    };
                    let placed = paragraph
                        .layout(measured.width, TextAlignment::Leading)
                        .unwrap();
                    assert_eq!(
                        placed.size, measured,
                        "proposal {width}, size {size}, limit {limit:?}, text {text:?}"
                    );
                    assert_eq!(placed.lines().collect::<Vec<_>>(), lines);
                }
            }
        }
    }
}

#[test]
fn invalid_text_size_keeps_published_frame_even_when_view_has_fixed_dimensions() {
    for fixed in [false, true] {
        let element = |size| {
            let text = Text::new("Unchanged until the next valid layout").font_size(size);
            if fixed {
                text.frame(200.0, 60.0).into_element()
            } else {
                text.into_element()
            }
        };
        let mut tree = ViewTree::new(element(16.0), || {});
        tree.update((320, 180), 1.0).unwrap();
        let generation = tree.frame().generation();
        let program = tree.frame().instances()[0].program.id();
        for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            tree.set_root(element(size));
            assert!(tree.update((320, 180), 1.0).is_err());
            assert_eq!(
                tree.scene().layout_failure().unwrap().error,
                LayoutError::Text(TextError::InvalidSize)
            );
            assert_eq!(tree.frame().generation(), generation);
            assert_eq!(tree.frame().instances()[0].program.id(), program);
        }
        tree.set_root(element(18.0));
        tree.update((320, 180), 1.0).unwrap();
        assert!(tree.frame().generation() > generation);
        assert!(tree.scene().layout_failure().is_none());
    }
}
