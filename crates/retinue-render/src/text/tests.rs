use super::*;
use crate::{Color, painter::Painter};
use std::sync::Arc;

mod retained;

const ROBOTO: &[u8] = include_bytes!("../../../../assets/fonts/roboto/Roboto-Regular.ttf");

fn font() -> Font {
    Font::new(FontData::new(Blob::new(Arc::new(ROBOTO.to_vec())), 0)).unwrap()
}

fn run(font: Font) -> GlyphRun {
    let id = font.metrics.charmap().map_unicode('A').unwrap().to_u32();
    GlyphRun::new(
        font,
        24.0,
        vec![Glyph {
            id,
            x: 1.25,
            y: 26.5,
        }],
        Hinting::None,
    )
    .unwrap()
}

#[test]
fn immutable_resources_are_shareable_without_native_handles() {
    fn shared<T: Send + Sync>() {}
    shared::<Font>();
    shared::<GlyphRun>();
    let run = run(font());
    let other = run.clone();
    assert_eq!(run.glyphs().as_ptr(), other.glyphs().as_ptr());
    assert_eq!(
        run.font().data().data.data().as_ptr(),
        other.font().data().data.data().as_ptr()
    );
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let prepared = PreparedFont::new(other.font(), other.size()).unwrap();
                let mut pixels = vec![0; 80 * 50 * 4];
                let mut painter = Painter::new(&mut pixels, 80, 50).unwrap();
                painter.draw_glyph_run(&prepared, &other, (0.0, 0.0), 1.0, Color::rgb(0xffffff));
                painter.finish().unwrap();
                assert!(pixels.iter().any(|&byte| byte != 0));
            })
            .join()
            .unwrap();
    });
}

#[test]
fn source_text_is_validated_and_shared_without_changing_glyph_storage() {
    let original = run(font());
    let source: Arc<str> = "first A last".into();
    let tagged = original.clone().with_source(source.clone(), 6..7).unwrap();
    assert_eq!(tagged.source(), Some("A"));
    assert_eq!(original.glyphs().as_ptr(), tagged.glyphs().as_ptr());
    assert_eq!(original.fingerprint(), tagged.fingerprint());
    assert!(original.clone().with_source(source.clone(), 0..99).is_err());
    assert!(original.with_source("\u{e9}".into(), 0..1).is_err());
}

#[test]
fn native_queue_owns_bytes_after_all_rust_leases_are_dropped() {
    let face = font();
    let weak = face.data().data.downgrade();
    let run = run(face);
    let prepared = PreparedFont::new(run.font(), run.size()).unwrap();
    let mut pixels = vec![0; 80 * 50 * 4];
    let mut painter = Painter::with_threads(&mut pixels, 80, 50, 4.try_into().unwrap()).unwrap();
    painter.draw_glyph_run(&prepared, &run, (0.0, 0.0), 1.0, Color::rgb(0xffffff));
    drop(run);
    drop(prepared);
    assert!(weak.upgrade().is_some());
    painter.finish().unwrap();
    assert!(weak.upgrade().is_none());
    assert!(pixels.iter().any(|&byte| byte != 0));
}

#[test]
fn rejects_invalid_font_glyph_size_and_hinting() {
    let blob = Blob::new(Arc::new(ROBOTO.to_vec()));
    assert!(matches!(
        Font::new(FontData::new(blob, 1)),
        Err(TextError::InvalidFont)
    ));
    let face = font();
    assert!(matches!(
        GlyphRun::new(face.clone(), 0.0, vec![], Hinting::None),
        Err(TextError::InvalidSize)
    ));
    assert!(matches!(
        GlyphRun::new(face.clone(), 16.0, vec![], Hinting::Full),
        Err(TextError::UnsupportedHinting)
    ));
    for glyph in [
        Glyph {
            id: u32::MAX,
            x: 0.0,
            y: 0.0,
        },
        Glyph {
            id: 1,
            x: f64::NAN,
            y: 0.0,
        },
    ] {
        assert!(matches!(
            GlyphRun::new(face.clone(), 16.0, vec![glyph], Hinting::None),
            Err(TextError::InvalidGlyph)
        ));
    }
}

#[test]
fn cannot_submit_a_run_with_a_different_face_or_size() {
    let run = run(font());
    for prepared in [
        PreparedFont::new(&font(), run.size()).unwrap(),
        PreparedFont::new(run.font(), run.size() + 1.0).unwrap(),
    ] {
        let mut pixels = vec![0; 80 * 50 * 4];
        let mut painter = Painter::new(&mut pixels, 80, 50).unwrap();
        painter.draw_glyph_run(&prepared, &run, (0.0, 0.0), 1.0, Color::rgb(0xffffff));
        assert!(painter.finish().is_err());
    }
}

fn render(
    run: &GlyphRun,
    origin: (f64, f64),
    scale: f64,
    threads: u32,
    repair: Option<&crate::damage::Region>,
) -> Vec<u8> {
    let prepared = PreparedFont::new(run.font(), run.size()).unwrap();
    let mut pixels = vec![0; 120 * 90 * 4];
    let mut painter =
        Painter::with_threads(&mut pixels, 120, 90, threads.try_into().unwrap()).unwrap();
    if let Some(region) = repair {
        painter.repair(region);
    }
    painter.draw_glyph_run(&prepared, run, origin, scale, Color::rgb(0xffffff));
    painter.finish().unwrap();
    pixels
}

#[test]
fn native_placement_preserves_fractional_baselines_scale_and_ink() {
    let run = run(font());
    let origin = (12.25, 9.75);
    for scale in [1.0, 1.2, 1.75] {
        let pixels = render(&run, origin, scale, 1, None);
        assert_eq!(pixels, render(&run, origin, scale, 4, None));
        let shifted = render(&run, (origin.0 + 7.0, origin.1 + 9.0), scale, 4, None);
        for y in 0..81 {
            for x in 0..113 {
                let i = (y * 120 + x) * 4;
                let j = ((y + 9) * 120 + x + 7) * 4;
                assert_eq!(&pixels[i..i + 4], &shifted[j..j + 4]);
            }
        }
        let ink = run.ink_bounds();
        let xs: Vec<_> = pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, pixel)| pixel[3] > 0)
            .map(|(i, _)| (i % 120, i / 120))
            .collect();
        assert!(!xs.is_empty());
        for (actual, expected) in [
            (
                xs.iter().map(|p| p.0).min().unwrap(),
                origin.0 + ink.x as f64 * scale,
            ),
            (
                xs.iter().map(|p| p.1).min().unwrap(),
                origin.1 + ink.y as f64 * scale,
            ),
            (
                xs.iter().map(|p| p.0).max().unwrap() + 1,
                origin.0 + (ink.x + ink.width) as f64 * scale,
            ),
            (
                xs.iter().map(|p| p.1).max().unwrap() + 1,
                origin.1 + (ink.y + ink.height) as f64 * scale,
            ),
        ] {
            assert!(
                (actual as f64 - expected).abs() < 1.5,
                "actual {actual}, expected {expected}"
            );
        }
    }
}

#[test]
fn glyph_runs_obey_disjoint_repair_without_changing_following_draw_state() {
    let run = run(font());
    let region = crate::damage::Region::new([
        crate::Rect {
            x: 15,
            y: 10,
            width: 7,
            height: 48,
        },
        crate::Rect {
            x: 27,
            y: 15,
            width: 12,
            height: 25,
        },
    ]);
    for scale in [1.0, 1.2, 1.75] {
        let full = render(&run, (12.25, 9.75), scale, 4, None);
        let partial = render(&run, (12.25, 9.75), scale, 4, Some(&region));
        for (i, pixel) in partial.chunks_exact(4).enumerate() {
            let (x, y) = ((i % 120) as u32, (i / 120) as u32);
            if region
                .rects()
                .iter()
                .any(|r| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            {
                assert!(
                    pixel
                        .iter()
                        .zip(&full[i * 4..i * 4 + 4])
                        .all(|(a, b)| a.abs_diff(*b) <= 1)
                );
            } else {
                assert_eq!(pixel, &[0; 4]);
            }
        }
    }
    let prepared = PreparedFont::new(run.font(), run.size()).unwrap();
    let mut pixels = vec![0; 120 * 90 * 4];
    let mut painter = Painter::new(&mut pixels, 120, 90).unwrap();
    painter.draw_glyph_run(&prepared, &run, (10.0, 10.0), 1.75, Color::rgb(0xffffff));
    painter.fill_rounded_rect(90, 70, 10, 10, 0, [20, 40, 60, 255]);
    painter.finish().unwrap();
    assert_eq!(
        &pixels[(75 * 120 + 95) * 4..(75 * 120 + 95) * 4 + 4],
        &[20, 40, 60, 255]
    );
}

#[test]
fn clipped_curved_glyph_preserves_coverage_within_native_quantization() {
    let face = font();
    let id = face.metrics.charmap().map_unicode('o').unwrap().to_u32();
    let run = GlyphRun::new(
        face,
        14.0,
        vec![Glyph {
            id,
            x: 7.5947265625,
            y: 12.98828125,
        }],
        Hinting::None,
    )
    .unwrap();
    let clip = crate::Rect {
        x: 33,
        y: 8,
        width: 50,
        height: 50,
    };
    let region = crate::damage::Region::new([clip]);
    let full = render(&run, (63.0, 47.0), 1.25, 4, None);
    let partial = render(&run, (63.0, 47.0), 1.25, 4, Some(&region));
    for (index, (a, b)) in partial
        .chunks_exact(4)
        .zip(full.chunks_exact(4))
        .enumerate()
    {
        if clip.contains((index % 120) as f64, (index / 120) as f64) {
            assert!(a.iter().zip(b).all(|(a, b)| a.abs_diff(*b) <= 1));
        } else {
            assert_eq!(a, [0; 4]);
        }
    }
}

#[test]
fn capability_detection_does_not_silently_use_default_or_monochrome_outlines() {
    for tag in [
        b"fvar", b"CFF2", b"VARC", b"COLR", b"SVG ", b"sbix", b"CBDT", b"EBDT",
    ] {
        let mut bytes = ROBOTO.to_vec();
        let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
        let record = bytes[12..12 + count * 16]
            .chunks_exact_mut(16)
            .find(|r| &r[..4] == b"name")
            .unwrap();
        record[..4].copy_from_slice(tag);
        // SFNT directories are tag-sorted; keep lookup valid after the mutation.
        let mut records: Vec<[u8; 16]> = bytes[12..12 + count * 16]
            .chunks_exact(16)
            .map(|r| r.try_into().unwrap())
            .collect();
        records.sort_by_key(|r| <[u8; 4]>::try_from(&r[..4]).unwrap());
        for (dst, src) in bytes[12..12 + count * 16].chunks_exact_mut(16).zip(records) {
            dst.copy_from_slice(&src);
        }
        assert!(
            matches!(
                Font::new(FontData::new(Blob::new(Arc::new(bytes)), 0)),
                Err(TextError::UnsupportedFont(_))
            ),
            "tag {tag:?}"
        );
    }
}

fn collection() -> Blob<u8> {
    let fonts: [&[u8]; 2] = [
        ROBOTO,
        include_bytes!("../../../../assets/fonts/roboto/Roboto-Bold.ttf"),
    ];
    let mut bytes = b"ttcf\0\x01\0\0\0\0\0\x02\0\0\0\0\0\0\0\0".to_vec();
    for (index, font) in fonts.into_iter().enumerate() {
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        let offset = bytes.len();
        bytes[12 + index * 4..16 + index * 4].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes.extend_from_slice(font);
        let count = u16::from_be_bytes(font[4..6].try_into().unwrap()) as usize;
        for record in bytes[offset + 12..offset + 12 + count * 16].chunks_exact_mut(16) {
            let table = u32::from_be_bytes(record[8..12].try_into().unwrap()) + offset as u32;
            record[8..12].copy_from_slice(&table.to_be_bytes());
        }
    }
    Blob::new(Arc::new(bytes))
}

#[test]
fn native_font_uses_exact_collection_face_index() {
    let data = collection();
    let regular = run(Font::new(FontData::new(data.clone(), 0)).unwrap());
    let bold = run(Font::new(FontData::new(data, 1)).unwrap());
    let standalone = run(Font::new(FontData::new(
        Blob::new(Arc::new(
            include_bytes!("../../../../assets/fonts/roboto/Roboto-Bold.ttf").as_slice(),
        )),
        0,
    ))
    .unwrap());
    assert_eq!(regular.font().identity().0, bold.font().identity().0);
    let image = render(&bold, (0.0, 0.0), 1.2, 4, None);
    assert_eq!(image, render(&standalone, (0.0, 0.0), 1.2, 4, None));
    assert_ne!(image, render(&regular, (0.0, 0.0), 1.2, 4, None));
}
