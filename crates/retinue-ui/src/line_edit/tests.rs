use super::*;

fn font() -> CachedFont {
    CachedFont::from_bytes(include_bytes!(
        "../../../../assets/fonts/roboto/Roboto-Regular.ttf"
    ))
    .unwrap()
}

fn editor() -> LineEdit {
    LineEdit::new(&font()).unwrap()
}
fn cursor(e: &LineEdit) -> usize {
    e.editor.raw_selection().focus().index()
}

#[test]
fn preedit_is_visual_only_and_restores_selection_for_cancel_or_commit() {
    let mut e = editor();
    e.set_text("Mesa Prime", false);
    e.edit(|d| d.select_byte_range(10, 5));
    assert!(e.preedit("Ї", Some((2, 2))));
    assert_eq!(e.text(), "Mesa Prime");
    assert_eq!(e.display_text(), "Mesa Ї");
    assert!(!e.preedit("Ї", Some((1, 1))));
    assert!(e.preedit("éa", None));
    assert!(e.caret_x().is_none());
    assert!(paint(&mut e, RECT, 18.0).chunks_exact(4).any(|p| p[3] > 0));
    assert!(e.preedit("", None));
    assert_eq!(e.selected_text().as_deref(), Some("Prime"));
    assert_eq!(cursor(&e), 5);
    assert!(e.preedit("more", Some((4, 4))));
    assert!(!e.insert("bad", |_| false));
    assert_eq!(e.display_text(), "Mesa Prime");
    assert_eq!(cursor(&e), 5);
    assert!(e.preedit("preview", None));
    assert!(e.insert("é", |_| true));
    assert_eq!(e.text(), "Mesa é");
    assert!(!e.cancel_preedit());
}

#[test]
fn ime_candidate_rectangle_follows_field_and_password_never_composes() {
    let mut e = editor();
    e.set_text("Long search that scrolls", false);
    let field = Rect { width: 65, ..RECT };
    e.layout_scaled(field, 21.0, 1.5);
    assert!(e.preedit("test", Some((4, 4))));
    let area = e.ime_area(field).unwrap();
    assert!(area.x >= field.x && area.y >= field.y && area.width > 0 && area.height > 0);
    assert!(
        area.x + area.width <= field.x + field.width
            && area.y + area.height <= field.y + field.height
    );
    let moved = e
        .ime_area(Rect {
            x: field.x + 100,
            ..field
        })
        .unwrap();
    assert_eq!(moved.x, area.x + 100);
    let mut secret = LineEdit::password(&font()).unwrap();
    secret.set_text("private", false);
    assert!(!secret.preedit("draft", None));
    assert!(secret.ime_area(field).is_none());
    assert_eq!(secret.display_text(), "*******");
}

const COLORS: Colors = Colors {
    text: [255; 4],
    selection: [120, 60, 40, 255],
};
const RECT: Rect = Rect {
    x: 4,
    y: 4,
    width: 220,
    height: 36,
};

fn paint(e: &mut LineEdit, rect: Rect, size: f32) -> Vec<u8> {
    let width = rect.x + rect.width + 4;
    let height = rect.y + rect.height + 4;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut p = Painter::new(&mut pixels, width, height).unwrap();
    e.paint(&mut p, rect, size, COLORS);
    p.finish().unwrap();
    pixels
}

#[test]
fn canvas_editor_matches_label_baseline_and_scale() {
    use crate::drawing::PixelCanvas;
    use retinue_render::{
        PaintFrame, PaintInstance, PaintProgram, RasterMode, Renderer, ResidencyPolicy,
    };
    use std::sync::Arc;
    for bytes in [
        include_bytes!("../../../../assets/fonts/roboto/Roboto-Regular.ttf").as_slice(),
        include_bytes!("../../../../apps/poe2companion/assets/Lato-Regular.ttf").as_slice(),
    ] {
        let font = Rc::new(CachedFont::from_bytes(bytes).unwrap());
        for scale in [0.9, 1.0, 1.2, 1.3, 1.75, 2.0] {
            let width = (240.0 * scale) as u32;
            let height = (52.0 * scale) as u32;
            let rect = Rect {
                x: (8.0 * scale) as u32,
                y: (7.0 * scale) as u32,
                width: (220.0 * scale) as u32,
                height: (36.0 * scale) as u32,
            };
            let text = "Mesa 123";
            let mut edit = LineEdit::new(&font).unwrap();
            edit.set_text(text, false);
            edit.caret_on = false;
            let mut actual = PixelCanvas::new(scale);
            actual.editor(&mut edit, rect, 14.0 * scale, COLORS);
            let mut expected = PixelCanvas::new(scale);
            expected.text(rect, font.clone(), text.into(), 14.0, COLORS.text);
            let render = |canvas: PixelCanvas| {
                let program = PaintProgram::new(
                    1,
                    (width as f32 / scale, height as f32 / scale),
                    scale,
                    Arc::new(canvas.commands),
                )
                .unwrap();
                let bounds = Rect {
                    width,
                    height,
                    ..Rect::default()
                };
                let mut frame = PaintFrame::default();
                frame
                    .replace(
                        (width, height),
                        scale,
                        vec![PaintInstance::new(1.into(), program, (0, 0), bounds)],
                        vec![],
                    )
                    .unwrap();
                let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
                renderer.prepare(&frame, RasterMode::Replay);
                let packet = renderer
                    .freeze(&frame, &bounds.into(), |_, _| unreachable!())
                    .unwrap();
                let mut pixels = vec![0; (width * height * 4) as usize];
                let mut painter = Painter::new(&mut pixels, width, height).unwrap();
                packet.draw(&mut painter);
                painter.finish().unwrap();
                pixels
            };
            let actual = render(actual);
            let expected = render(expected);
            let maximum = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            let changed = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(8)
                .map(|(i, (a, b))| (i / 4 % width as usize, i / 4 / width as usize, *a, *b))
                .collect::<Vec<_>>();
            assert!(
                maximum <= 1,
                "scale={scale} max={maximum} first={changed:?}"
            );
            edit.layout_scaled(rect, 14.0 * scale, scale);
            let glyphs = edit.glyphs.as_ref().unwrap().as_ref().unwrap()[0]
                .glyphs()
                .as_ptr();
            edit.click(0.0, false);
            edit.drag(28.0 * scale);
            let mut oracle = LineEdit::new(&font).unwrap();
            oracle.set_text(text, false);
            oracle.layout(RECT, 14.0);
            oracle.click(0.0, false);
            oracle.drag(28.0);
            assert_eq!(
                edit.selected_text(),
                oracle.selected_text(),
                "input scale={scale}"
            );
            edit.drag(500.0 * scale);
            assert_eq!(edit.selected_text().as_deref(), Some(text));
            assert_eq!(
                edit.glyphs.as_ref().unwrap().as_ref().unwrap()[0]
                    .glyphs()
                    .as_ptr(),
                glyphs
            );
        }
    }
}

#[test]
fn recorded_editor_ink_survives_edits_and_renders_on_another_worker() {
    use retinue_render::{
        PaintFrame, PaintInstance, PaintProgram, RasterMode, Renderer, ResidencyPolicy,
    };
    use std::sync::Arc;
    for masked in [false, true] {
        for scale in [0.9, 1.0, 1.2, 1.3, 2.0] {
            let mut edit = if masked {
                LineEdit::password(&font()).unwrap()
            } else {
                editor()
            };
            edit.set_text("Selection and caret across the viewport", true);
            let rect = Rect { width: 110, ..RECT };
            let size = 14.0 * scale;
            let expected = paint(&mut edit, rect, size);
            let width = rect.x + rect.width + 4;
            let height = rect.y + rect.height + 4;
            let bounds = Rect {
                x: 0,
                y: 0,
                width,
                height,
            };
            let program = PaintProgram::new(
                1,
                (width as f32, height as f32),
                1.0,
                Arc::new(edit.commands(rect, size, COLORS).unwrap()),
            )
            .unwrap();
            let mut frame = PaintFrame::default();
            frame
                .replace(
                    (width, height),
                    1.0,
                    vec![PaintInstance::new(1.into(), program, (0, 0), bounds)],
                    vec![],
                )
                .unwrap();
            let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
            renderer.prepare(&frame, RasterMode::Replay);
            let packet = renderer
                .freeze(&frame, &bounds.into(), |_, _| unreachable!())
                .unwrap();
            edit.set_text("new state must not change submitted ink", false);
            drop(edit);
            drop(renderer);
            let pixels = std::thread::spawn(move || {
                let mut pixels = vec![0; width as usize * height as usize * 4];
                let mut painter = Painter::new(&mut pixels, width, height).unwrap();
                packet.draw(&mut painter);
                painter.finish().unwrap();
                pixels
            })
            .join()
            .unwrap();
            let mismatches = pixels
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(6)
                .map(|(i, (a, b))| (i, a, b))
                .collect::<Vec<_>>();
            assert!(
                mismatches.is_empty(),
                "masked={masked} scale={scale} {mismatches:?}"
            );
        }
    }
}

#[test]
fn shared_resources_keep_editors_independent_and_match_standalone_pixels() {
    let font = font();
    let resources = EditResources::new(&font).unwrap();
    let mut a = resources.editor();
    let mut b = resources.password();
    assert!(Rc::ptr_eq(&a.resources.face, &b.resources.face));
    assert!(Rc::ptr_eq(
        &b.resources.face,
        &b.masked.as_ref().unwrap().resources.face
    ));
    assert!(Rc::ptr_eq(
        &resources.face,
        &TextFace::for_font(&font).unwrap()
    ));
    a.set_text("Mesa Prime", true);
    b.set_text("private", false);
    for scale in [1.0, 1.2, 1.25, 2.0] {
        let rect = Rect {
            width: (220.0 * scale) as u32,
            height: (36.0 * scale) as u32,
            ..RECT
        };
        let mut oracle = editor();
        oracle.set_text("Mesa Prime", true);
        assert_eq!(
            paint(&mut a, rect, 14.0 * scale),
            paint(&mut oracle, rect, 14.0 * scale)
        );
        paint(&mut b, rect, 14.0 * scale);
    }
    assert_eq!(a.selected_text().as_deref(), Some("Mesa Prime"));
    assert_eq!(b.text(), "private");
    assert_eq!(b.selected_text(), None);
    assert!(b.glyphs.is_none());
}

#[test]
fn password_mask_tracks_unicode_editing_without_exposing_clipboard_text() {
    let mut e = LineEdit::password(&font()).unwrap();
    e.set_text("a\u{00e9}\u{1f512}z", false);
    e.layout(RECT, 14.0);
    assert_eq!(e.masked.as_ref().unwrap().text(), "****");
    e.key(0xff51, false, false);
    e.key(0xff08, false, false);
    assert_eq!(e.text(), "a\u{00e9}z");
    e.key(0x61, true, false);
    assert_eq!(e.selected_text(), None);
    e.insert("replacement", |_| true);
    assert_eq!(e.text(), "replacement");
    e.layout(RECT, 14.0);
    assert_eq!(e.masked.as_ref().unwrap().text(), "***********");
    e.click(0.0, false);
    e.insert("x", |_| true);
    assert_eq!(e.text(), "xreplacement");
    e.set_text("e\u{301}z", false);
    e.layout(RECT, 14.0);
    assert_eq!(e.masked.as_ref().unwrap().text(), "**");
    e.click(0.0, false);
    e.drag(500.0);
    assert!(e.delete_selection());
    assert_eq!(e.text(), "");
}

#[test]
fn resize_and_shorter_text_remove_overscroll_without_moving_caret() {
    let mut e = editor();
    let small = Rect { width: 60, ..RECT };
    e.layout(small, 14.0);
    e.set_text("Ash Prime Neuroptics Blueprint", false);
    assert!(e.scroll > 0.0);
    let before = cursor(&e);
    e.layout(
        Rect {
            width: 400,
            ..small
        },
        14.0,
    );
    assert_eq!(e.scroll, 0.0);
    assert_eq!(cursor(&e), before);
    e.layout(small, 14.0);
    assert!(e.scroll > 0.0);
    e.set_text("Ash", false);
    assert_eq!(e.scroll, 0.0);
    e.layout(Rect { x: 200, ..small }, 14.0);
    assert_eq!(e.scroll, 0.0);
}

#[test]
fn caret_deadline_tracks_edit_phase_and_missed_blinks() {
    let mut e = editor();
    let start = Instant::now();
    e.changed = start;
    assert_eq!(e.next_blink(start), start + Duration::from_millis(500));
    let late = start + Duration::from_millis(750);
    assert_eq!(e.next_blink(late), late);
    e.caret_on = false;
    assert_eq!(e.next_blink(late), start + Duration::from_secs(1));
    let late = start + Duration::from_millis(1250);
    assert_eq!(e.next_blink(late), late);
}

#[test]
fn insertion_selection_and_directional_delete() {
    let mut e = editor();
    e.set_text("1234", false);
    e.key(0xff51, false, false);
    e.insert("5", |_| true);
    assert_eq!(e.text(), "12354");
    e.key(0xff51, false, true);
    assert_eq!(e.selected_text().as_deref(), Some("5"));
    e.key(0xffff, false, false);
    assert_eq!(e.text(), "1234");
    e.key(0xff08, false, false);
    assert_eq!(e.text(), "124");
    e.key(0xff50, false, false);
    e.key(0xffff, false, false);
    assert_eq!(e.text(), "24");
}

#[test]
fn rejected_paste_preserves_selection_and_value() {
    let mut e = editor();
    e.set_text("15", true);
    assert!(!e.insert("not a number", |s| s.parse::<f64>().is_ok()));
    assert!(!e.insert("5\n6", |_| true));
    assert_eq!(e.selected_text().as_deref(), Some("15"));
    assert!(e.insert("-12.5", |s| s.parse::<f64>().is_ok()));
    assert_eq!(e.text(), "-12.5");
}

#[test]
fn collapsed_editor_does_not_prepare_native_resources() {
    let mut e = editor();
    assert!(
        paint(&mut e, Rect { width: 0, ..RECT }, 14.0)
            .iter()
            .all(|v| *v == 0)
    );
    assert!(e.fonts.is_empty());
    assert!(e.glyphs.is_none());
}

#[test]
fn fractional_scale_hit_testing_and_overflow() {
    for scale in [1.0, 1.2, 1.25, 1.5, 2.0] {
        let mut e = editor();
        e.set_text("1234567890123456", false);
        let rect = Rect {
            width: (50.0 * scale) as u32,
            height: (18.0 * scale) as u32,
            ..RECT
        };
        paint(&mut e, rect, 12.0 * scale);
        assert!(e.caret_x().unwrap() <= rect.width as f32);
        e.key(0xff50, false, false);
        paint(&mut e, rect, 12.0 * scale);
        e.click(0.0, false);
        assert_eq!(cursor(&e), 0);
        e.drag(rect.width as f32 / 2.0);
        assert!(e.selected_text().is_some());
        let pixels = paint(&mut e, rect, 12.0 * scale);
        assert!(pixels.iter().any(|v| *v != 0));
        let width = (rect.x + rect.width + 4) as usize;
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            let (x, y) = ((i % width) as u32, (i / width) as u32);
            if x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height {
                assert_eq!(pixel, [0; 4]);
            }
        }
    }
}

#[test]
fn selection_blink_color_and_viewport_changes_keep_native_runs() {
    let mut e = editor();
    e.set_text("office e\u{301} 123", false);
    paint(&mut e, RECT, 14.4);
    let run = e.glyphs.as_ref().unwrap().as_ref().unwrap()[0].clone();
    let generation = e.editor.generation();
    e.layout(
        Rect {
            width: 40,
            height: 40,
            ..RECT
        },
        14.4,
    );
    assert_eq!(generation, e.editor.generation());
    e.key(0xff51, false, true);
    e.caret_on = false;
    let mut pixels = vec![0; 300 * 100 * 4];
    let mut p = Painter::new(&mut pixels, 300, 100).unwrap();
    e.paint(
        &mut p,
        RECT,
        14.4,
        Colors {
            text: [80, 90, 100, 255],
            ..COLORS
        },
    );
    p.finish().unwrap();
    assert!(std::ptr::eq(
        run.glyphs().as_ptr(),
        e.glyphs.as_ref().unwrap().as_ref().unwrap()[0]
            .glyphs()
            .as_ptr()
    ));
}

#[test]
fn word_selection_and_multibyte_clusters() {
    let mut e = editor();
    e.set_text("Mesa Prime", false);
    e.key(0xff51, true, true);
    assert_eq!(e.selected_text().as_deref(), Some("Prime"));
    e.key(0xffff, false, false);
    assert_eq!(e.text(), "Mesa ");
    e.key(0xff08, true, false);
    assert_eq!(e.text(), "");
    e.set_text("a e\u{301} z", false);
    e.key(0xff50, false, false);
    e.key(0xff53, false, false);
    e.key(0xff53, false, false);
    e.key(0xff53, false, true);
    assert_eq!(e.selected_text().as_deref(), Some("e\u{301}"));
    e.insert("\u{0411}", |_| true);
    assert_eq!(e.text(), "a \u{0411} z");
}

#[test]
fn invalid_size_fails_paint_without_corrupting_editor() {
    let mut e = editor();
    e.set_text("123", false);
    let mut pixels = vec![0; 300 * 100 * 4];
    let mut p = Painter::new(&mut pixels, 300, 100).unwrap();
    e.paint(&mut p, RECT, f32::NAN, COLORS);
    assert!(p.finish().unwrap_err().to_string().contains("text size"));
    assert!(paint(&mut e, RECT, 14.0).iter().any(|v| *v != 0));
}

#[test]
fn editor_and_paragraph_resolve_identical_glyph_geometry() {
    let font = font();
    let face = TextFace::for_font(&font).unwrap();
    let fallback = CachedFont::from_bytes(include_bytes!(
        "../../../../apps/poe2companion/assets/Lato-Regular.ttf"
    ))
    .unwrap()
    .native();
    assert!(!font.native().contains('\u{180}'));
    assert!(fallback.contains('\u{180}'));
    face.register_fallback("\u{180}", fallback.clone());
    let mut editor = LineEdit::new(&font).unwrap();
    for text in [
        "",
        " ",
        "office e\u{301} 123",
        "Mesa Prime  ",
        "A\u{180}Z",
        "\u{041f}\u{0440}\u{0438}\u{0432}\u{0456}\u{0442}",
    ] {
        for size in [14.0, 16.8, 24.5] {
            editor.set_text(text, false);
            editor.caret_on = false;
            let actual = paint(&mut editor, RECT, size);
            let paragraph = face.shape(text, size).unwrap();
            let glyphs = editor.glyphs.as_ref().unwrap().as_ref().unwrap();
            if text.contains('\u{180}') {
                assert!(glyphs.iter().any(|r| r.font() == &fallback));
                assert!(glyphs.iter().any(|r| r.font() != &fallback));
                assert!(glyphs.iter().flat_map(|r| r.glyphs()).all(|g| g.id != 0));
                let width = RECT.x + RECT.width + 4;
                let height = RECT.y + RECT.height + 4;
                let mut expected = vec![0; actual.len()];
                let mut painter = Painter::new(&mut expected, width, height).unwrap();
                let y = (RECT.height as f32 - paragraph.layout().height()) * 0.5;
                painter.local(RECT, |p, _| {
                    paragraph.prepare().unwrap().draw(
                        p,
                        (0.0, f64::from(y)),
                        1.0,
                        Color::from_bgra(COLORS.text),
                    );
                });
                painter.finish().unwrap();
                assert!(actual == expected, "direct editor differs at {size}px");
            }
            assert_eq!(glyphs.len(), paragraph.runs().len());
            for (editor, paragraph) in glyphs.iter().zip(paragraph.runs()) {
                assert_eq!(editor.font(), paragraph.font());
                assert_eq!(editor.size(), paragraph.size());
                assert_eq!(editor.glyphs(), paragraph.glyphs());
                assert_eq!(editor.ink_bounds(), paragraph.ink_bounds());
                assert_eq!(editor.source(), None);
            }
            assert_eq!(
                editor.editor.try_layout().unwrap().height(),
                paragraph.layout().height()
            );
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires installed static outline fonts covering Arabic and Devanagari"]
fn installed_outline_fallback_shapes_scripts_and_preserves_visual_editing() {
    let font = font();
    let face = TextFace::for_font(&font).unwrap();
    let mut edit = LineEdit::new(&font).unwrap();
    let arabic = "\u{633}\u{644}\u{627}\u{645}";
    let indic = "\u{915}\u{93f}";
    let math = "\u{1d6c2}";
    let has_math = face
        .shape(math, 24.0)
        .unwrap()
        .runs()
        .iter()
        .flat_map(|r| r.glyphs())
        .any(|g| g.id != 0);
    let prefix = if has_math { math } else { "A" };
    for text in [arabic, indic, &format!("{prefix} {arabic} {indic} Z")] {
        edit.set_text(text, false);
        edit.caret_on = false;
        let pixels = paint(&mut edit, RECT, 24.0);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let paragraph = face.shape(text, 24.0).unwrap();
        let runs = edit.glyphs.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(runs.len(), paragraph.runs().len());
        assert!(runs.iter().any(|r| r.font() != &font.native()));
        for (a, b) in runs.iter().zip(paragraph.runs()) {
            assert_eq!(a.font(), b.font());
            assert_eq!(a.glyphs(), b.glyphs());
            assert!(a.glyphs().iter().all(|g| g.id != 0));
            if b.source().is_some_and(|s| s.contains(arabic)) {
                assert!(
                    b.font().has_script_substitutions(*b"Arab"),
                    "earlier symbols cannot steal Arabic runs"
                );
            }
        }
        if text == arabic {
            let glyph_ids = |value: &str| {
                face.shape(value, 24.0)
                    .unwrap()
                    .runs()
                    .iter()
                    .flat_map(|r| r.glyphs().iter().map(|g| g.id))
                    .collect::<Vec<_>>()
            };
            let isolated = arabic
                .chars()
                .rev()
                .flat_map(|c| glyph_ids(&c.to_string()))
                .collect::<Vec<_>>();
            assert_ne!(
                glyph_ids(arabic),
                isolated,
                "Arabic letters must use joining forms"
            );
        }
    }
    edit.set_text(arabic, false);
    edit.layout(RECT, 24.0);
    edit.key(0xff50, false, false);
    let right = edit.caret_x().unwrap();
    edit.key(0xff51, false, true);
    assert_eq!(edit.selected_text().as_deref(), Some("\u{633}"));
    assert!(edit.caret_x().unwrap() < right);

    edit.set_text(indic, false);
    edit.layout(RECT, 24.0);
    edit.key(0xff50, false, false);
    edit.key(0xff53, false, true);
    assert_eq!(edit.selected_text().as_deref(), Some(indic));
}

#[test]
fn double_click_shift_click_and_arrow_collapse_preserve_selection_rules() {
    let mut editor = editor();
    editor.set_text("Mesa Prime", false);
    editor.layout(RECT, 14.0);
    editor.click(8.0, false);
    editor.click(8.0, false);
    assert_eq!(editor.selected_text().as_deref(), Some("Mesa"));
    editor.key(0xff53, false, false);
    assert_eq!(cursor(&editor), 4);
    assert!(editor.selected_text().is_none());
    editor.click(0.0, true);
    assert_eq!(editor.selected_text().as_deref(), Some("Mesa"));
    editor.key(0xff51, false, false);
    assert_eq!(cursor(&editor), 0);
    assert!(editor.selected_text().is_none());
}

#[test]
fn changed_text_and_size_replace_runs_and_match_a_fresh_editor() {
    let mut retained = editor();
    retained.set_text("Mesa", false);
    paint(&mut retained, RECT, 14.0);
    let old = retained.glyphs.as_ref().unwrap().as_ref().unwrap()[0].clone();
    retained.insert(" Prime", |_| true);
    let changed = paint(&mut retained, RECT, 14.0);
    let mut fresh = editor();
    fresh.set_text("Mesa Prime", false);
    assert_eq!(changed, paint(&mut fresh, RECT, 14.0));
    assert_ne!(old, retained.glyphs.as_ref().unwrap().as_ref().unwrap()[0]);
    assert_eq!(
        paint(&mut retained, RECT, 16.8),
        paint(&mut fresh, RECT, 16.8)
    );
    retained.key(0xff08, false, false);
    fresh.set_text("Mesa Prim", false);
    assert_eq!(
        paint(&mut retained, RECT, 16.8),
        paint(&mut fresh, RECT, 16.8)
    );
}

#[cfg(feature = "desktop")]
mod accessibility {
    use super::*;
    use accesskit::{Action, Node, NodeId, Role, TextPosition, TextSelection};

    fn tree(
        e: &mut LineEdit,
        field: Rect,
        scale: f32,
        next: &mut u64,
    ) -> (Node, Vec<(NodeId, Node)>) {
        let mut field_node = Node::new(Role::TextInput);
        let mut runs = Vec::new();
        e.accessible_text(
            &mut field_node,
            &mut runs,
            field,
            14.0 * scale,
            scale,
            || {
                *next += 1;
                NodeId(*next)
            },
        );
        (field_node, runs)
    }

    #[test]
    fn published_clusters_roundtrip_directed_selection_and_stale_ids_are_rejected() {
        let mut e = editor();
        let mut next = 0;
        e.set_text("e\u{301}clair", false);
        let (_, runs) = tree(&mut e, RECT, 1.0, &mut next);
        let (id, run) = &runs[0];
        assert_eq!(run.character_lengths()[0], 3);
        assert_eq!(
            run.character_lengths().len(),
            run.character_widths().unwrap().len()
        );
        assert_eq!(
            run.character_positions().unwrap().len(),
            run.character_widths().unwrap().len()
        );
        let selection = TextSelection {
            anchor: TextPosition {
                node: *id,
                character_index: 1,
            },
            focus: TextPosition {
                node: *id,
                character_index: 0,
            },
        };
        assert!(e.select_accessible_text(&selection));
        assert_eq!(e.selected_text().as_deref(), Some("e\u{301}"));
        let (field, scaled) = tree(
            &mut e,
            Rect {
                x: RECT.x + 10,
                ..RECT
            },
            1.2,
            &mut next,
        );
        assert_eq!(field.text_selection(), Some(&selection));
        assert_eq!(scaled[0].0, *id);
        assert!(scaled[0].1.character_widths().unwrap()[0] > run.character_widths().unwrap()[0]);
        assert!(scaled[0].1.bounds().unwrap().x0 > run.bounds().unwrap().x0);
        e.set_text("changed", false);
        assert!(!e.select_accessible_text(&selection));
    }

    #[test]
    fn long_text_preserves_every_cluster_and_empty_field_exposes_a_caret() {
        let mut e = editor();
        let mut next = 0;
        let text = "a ".repeat(300);
        e.set_text(&text, false);
        let (field, runs) = tree(&mut e, RECT, 1.0, &mut next);
        assert_eq!(field.children().len(), runs.len());
        assert!(runs.len() >= 3);
        assert_eq!(
            runs.iter()
                .map(|(_, n)| n.value().unwrap())
                .collect::<String>(),
            text
        );
        for (_, n) in &runs {
            assert!(n.character_lengths().len() <= 256);
            assert!(
                n.word_starts()
                    .iter()
                    .all(|i| (*i as usize) < n.character_lengths().len())
            );
        }
        e.set_text("", false);
        let (field, _) = tree(&mut e, RECT, 1.0, &mut next);
        assert_eq!(field.text_selection().unwrap().focus.character_index, 0);
    }

    #[test]
    fn bidi_run_endpoints_preserve_visual_affinity_at_shared_byte_offsets() {
        let mut e = editor();
        let mut next = 0;
        let text = "A \u{5d0}\u{5d1} Z";
        e.set_text(text, false);
        let (_, runs) = tree(&mut e, RECT, 1.0, &mut next);
        assert_eq!(
            runs.iter()
                .map(|(_, n)| n.value().unwrap())
                .collect::<String>(),
            text
        );
        let index = runs
            .iter()
            .position(|(_, n)| n.text_direction() == Some(accesskit::TextDirection::RightToLeft))
            .unwrap();
        let (prev, preceding) = &runs[index - 1];
        let rtl = runs[index].0;
        for endpoint in [
            TextPosition {
                node: *prev,
                character_index: preceding.character_lengths().len(),
            },
            TextPosition {
                node: rtl,
                character_index: 0,
            },
        ] {
            let caret = TextSelection {
                anchor: endpoint,
                focus: endpoint,
            };
            assert!(e.select_accessible_text(&caret));
            let (field, _) = tree(&mut e, RECT, 1.0, &mut next);
            assert_eq!(field.text_selection(), Some(&caret));
        }
    }

    #[test]
    fn password_and_draft_never_publish_private_geometry() {
        let mut next = 0;
        let mut secret = LineEdit::password(&font()).unwrap();
        secret.set_text("secret", false);
        let (field, runs) = tree(&mut secret, RECT, 1.0, &mut next);
        assert!(field.value().is_none() && runs.is_empty());
        let mut e = editor();
        e.set_text("committed", false);
        assert!(e.preedit("draft", None));
        let (field, runs) = tree(&mut e, RECT, 1.0, &mut next);
        assert_eq!(field.value(), Some("committed"));
        assert!(runs.is_empty() && !field.supports_action(Action::SetTextSelection));
    }
}
