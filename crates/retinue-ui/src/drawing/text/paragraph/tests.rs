use super::*;

#[test]
fn first_proposal_matches_unconstrained_shaping_then_reflow() {
    let style = TextStyle::default();
    for (source, limit) in [
        ("Lith 013", Some(1)),
        ("", None),
        ("office a\u{301} next\n\nlast", Some(2)),
        ("abc אבג 123", None),
        ("a word that needs elision", Some(1)),
        ("hidden", Some(0)),
    ] {
        let spec = ParagraphSpec::new(source, &style, limit);
        for proposal in [
            Available::Ideal,
            Available::MinContent,
            Available::Limit(0.),
            Available::Limit(39.25),
        ] {
            for alignment in [
                TextAlignment::Leading,
                TextAlignment::Center,
                TextAlignment::Trailing,
            ] {
                let natural = spec.shape(source).unwrap();
                let widths = natural.layout().calculate_content_widths();
                let width = column_width(widths, proposal);
                let mut reference = Placement {
                    full: Some(natural),
                    visible: None,
                    key: None,
                    error: None,
                };
                reference.place(&spec, width, alignment).unwrap();
                let expected = reference.visible.as_ref().unwrap();
                let mut state = ParagraphState::default();
                let actual = state.layout_for(&spec, proposal, alignment).unwrap();
                assert_eq!(
                    (actual.size, actual.baseline, actual.last_baseline),
                    (expected.size, expected.baseline, expected.last_baseline)
                );
                assert_eq!(
                    actual.lines().collect::<Vec<_>>(),
                    expected.lines().collect::<Vec<_>>()
                );
                assert_eq!(actual.shaped().runs(), expected.shaped().runs());
                if source == "Lith 013" && alignment == TextAlignment::Center {
                    use retinue_render::painter::Painter;
                    let pixels = |layout: &TextLayout| {
                        let mut bytes = vec![0; 160 * 64 * 4];
                        let mut p = Painter::new(&mut bytes, 160, 64).unwrap();
                        layout.shaped().prepare().unwrap().draw(
                            &mut p,
                            (3.25, 5.5),
                            1.2,
                            Color::rgb(0xffffff),
                        );
                        p.finish().unwrap();
                        bytes
                    };
                    let before = pixels(expected);
                    if proposal == Available::Ideal {
                        assert!(before.iter().any(|&v| v != 0));
                    }
                    assert_eq!(before, pixels(actual));
                }
                let measured = state.current().unwrap().widths;
                assert_eq!((measured.min, measured.max), (widths.min, widths.max));
            }
        }
    }
}

#[test]
fn intrinsic_width_roundoff_does_not_elide_but_real_constraints_do() {
    for text in ["Ember Prime", "Warframes / Archwings", "office AV"] {
        let paragraph = Paragraph::new(text, &TextStyle::default(), Some(1));
        let width = paragraph.ideal_width().unwrap();
        for alignment in [
            TextAlignment::Leading,
            TextAlignment::Center,
            TextAlignment::Trailing,
        ] {
            let glyphs = {
                let layout = paragraph.layout(width, alignment).unwrap();
                assert_eq!(layout.lines().collect::<Vec<_>>(), [text]);
                layout.shaped().runs()[0].glyphs().as_ptr()
            };
            {
                let layout = paragraph.layout(width.next_down(), alignment).unwrap();
                assert_eq!(layout.lines().collect::<Vec<_>>(), [text]);
                assert_eq!(layout.shaped().runs()[0].glyphs().as_ptr(), glyphs);
            }
            assert_ne!(
                paragraph
                    .layout(width - 0.25, alignment)
                    .unwrap()
                    .lines()
                    .collect::<Vec<_>>(),
                [text]
            );
        }
    }
}

#[test]
fn text_inputs_share_and_placement_moves_without_cloning_glyph_storage() {
    fn shared<T: Send + Sync>() {}
    fn owned<T: Send>() {}
    shared::<ParagraphSpec>();
    owned::<ParagraphState>();

    let spec = Arc::new(ParagraphSpec::new(
        "A shaped paragraph can move between workers without copying its placement.",
        &TextStyle::default(),
        None,
    ));
    let mut state = ParagraphState::default();
    let initial = state.layout(&spec, 175.0, TextAlignment::Leading).unwrap();
    let glyphs = initial.shaped().runs()[0].glyphs().as_ptr() as usize;
    let font = initial.shaped().runs()[0].font().identity();
    let retained: Vec<_> = initial
        .shaped()
        .commands((0.0, 0.0), Color::rgb(0xffffff))
        .collect();
    let input = spec.clone();
    let mut state = std::thread::spawn(move || {
        let placed = state.layout(&input, 175.0, TextAlignment::Leading).unwrap();
        assert_eq!(placed.shaped().runs()[0].glyphs().as_ptr() as usize, glyphs);
        assert_eq!(placed.shaped().runs()[0].font().identity(), font);
        state.layout(&input, 82.5, TextAlignment::Trailing).unwrap();
        state
    })
    .join()
    .unwrap();
    let restored = state.layout(&spec, 175.0, TextAlignment::Leading).unwrap();
    assert!(
        retained
            .into_iter()
            .eq(restored.shaped().commands((0.0, 0.0), Color::rgb(0xffffff)))
    );
}

#[test]
fn shared_text_inputs_have_independent_worker_local_placement() {
    for limit in [None, Some(0), Some(1), Some(2)] {
        let spec = Arc::new(ParagraphSpec::new(
            "first line\n\nseveral words and a\u{301} combining cluster",
            &TextStyle::default(),
            limit,
        ));
        let jobs: Vec<_> = [60.25, 170.75, 800.0]
            .into_iter()
            .map(|width| {
                let spec = spec.clone();
                std::thread::spawn(move || {
                    let mut state = ParagraphState::default();
                    for alignment in [
                        TextAlignment::Leading,
                        TextAlignment::Center,
                        TextAlignment::Trailing,
                    ] {
                        let layout = state.layout(&spec, width, alignment).unwrap();
                        let mut oracle = ParagraphState::default();
                        let expected = oracle.layout(&spec, width, alignment).unwrap();
                        assert_eq!(layout.size, expected.size);
                        assert_eq!(
                            layout.lines().collect::<Vec<_>>(),
                            expected.lines().collect::<Vec<_>>()
                        );
                        assert!(
                            layout
                                .shaped()
                                .commands((0.0, 0.0), Color::rgb(0xffffff))
                                .eq(expected.shaped().commands((0.0, 0.0), Color::rgb(0xffffff)))
                        );
                    }
                })
            })
            .collect();
        for job in jobs {
            job.join().unwrap();
        }
    }
}
