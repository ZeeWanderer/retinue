use super::*;
use retinue_render::thumbnails::Thumbnails;

fn pixels(ui: &ControlHost, media: &Media) -> Vec<u8> {
    let (w, h) = ui.size;
    let mut out = vec![0; (w * h * 4) as usize];
    let mut p = Painter::new(&mut out, w, h).unwrap();
    ui.paint(&mut p, media);
    p.finish().unwrap();
    out
}

fn check_fresh_tree(
    pages: &mut Pages,
    images: &mut crate::presentation::tests::TestImages,
    label: &str,
) {
    images.load(pages);
    let retained = pixels(&pages.ui, &pages.media);
    pages.ui.install_view(pages.build_view.view(&pages.ui));
    pages.ui.begin(pages.ui.size, pages.ui.scale);
    pages.ui.update_view(FrameTime::now());
    pages.build_view.bind(&mut pages.ui, &pages.builds);
    pages.ui.finish();
    let fresh = pixels(&pages.ui, &pages.media);
    crate::presentation::tests::assert_pixels_match(&retained, &fresh, label);
}

#[test]
fn scrolling_and_hovered_build_content_matches_a_fresh_tree() {
    for scale in [1.0, 1.25, 2.0] {
        let mut pages = Pages::new(Default::default()).unwrap();
        pages.fixture();
        pages.set_page(Page::Builds);
        pages.configure(
            ((1000.0 * scale) as u32, (700.0 * scale) as u32),
            (400, 700),
            scale,
        );
        let mut images = crate::presentation::tests::TestImages::new();
        for offset in [0.0, 8.0, 36.0, 90.0, 135.0, 270.0, 75.0, 0.0] {
            pages
                .builds
                .scroll
                .to(offset * scale, false, Instant::now());
            pages.layout_center();
            check_fresh_tree(
                &mut pages,
                &mut images,
                &format!("scroll scale={scale} offset={offset}"),
            );
        }
        for key in ["build:mod:mod-1", "build:mod:mod-2", "build:mod:mod-1"] {
            pages.capture_hover(key).unwrap();
            check_fresh_tree(
                &mut pages,
                &mut images,
                &format!("hover scale={scale} key={key}"),
            );
        }
        pages.pointer(false, (0.0, 0.0));
        pages.layout_center();
        pages
            .builds
            .preview
            .tick(Instant::now() + Duration::from_millis(250));
        pages.layout_center();
        check_fresh_tree(
            &mut pages,
            &mut images,
            &format!("hover exit scale={scale}"),
        );
    }
}

#[test]
fn bounded_canvases_preserve_artwork_text_and_clips_at_fractional_origins() {
    let mut pages = Pages::new(Default::default()).unwrap();
    pages.fixture();
    let model = build_planner_model::Model::new(&pages.builds);
    for scale in [1.0, 1.25, 2.0] {
        let mut reference = ControlHost::new().unwrap();
        let mut declared = ControlHost::new().unwrap();
        let m = Metrics {
            scale,
            ..Default::default()
        };
        let mut notes = notes::Notes::default();
        let layout = layout::build(&reference, &model, "equipment", 980.0, &mut notes);
        let cases = layout
            .pieces
            .into_iter()
            .filter(|p| matches!(&p.drawing,Drawing::Slot(s) if !s.upgrade.is_null()))
            .collect::<Vec<_>>();
        for mut piece in cases {
            piece.bounds.x = 278.0;
            piece.bounds.y = 127.5;
            for offset in [0.0, 130.0] {
                let viewport = m.bounds(10.0, 41.0, 700.0, 350.0);
                let frame = piece.frame(&m);
                let scroll_pixels = (offset * scale).round() as u32;
                declared.install_view(ui! {
                    ZStack() {
                        fill(controls::PANEL, 0.0)
                        place(ScrollViewport::new(
                            place(slot(piece.key), frame).frame_aligned(viewport.width, 500.0, AnchorPoint::TOP_LEADING)
                        ).position(0.0, scroll_pixels as f32 / scale), viewport)
                    }.alignment(Alignment::Start).frame(800.0, 500.0)
                });
                let size = ((800.0 * scale) as u32, (500.0 * scale) as u32);
                declared.begin(size, scale);
                reference.begin(size, scale);
                declared.update_view(FrameTime::now());
                declared.view_control(piece.key, |ui, _| piece.paint(ui, &m, false));
                reference.fill(reference.rect(0.0, 0.0, 800.0, 500.0), 0.0, controls::PANEL);
                reference.begin_scroll(
                    reference.rect(viewport.x, viewport.y, viewport.width, viewport.height),
                    scroll_pixels,
                );
                piece.paint_at(&mut reference, piece.bounds.x, piece.bounds.y, false);
                reference.end_scroll();
                declared.finish();
                reference.finish();
                let mut media = Media::default();
                let requests = declared
                    .requests(&media)
                    .into_iter()
                    .chain(reference.requests(&media))
                    .collect::<Vec<_>>();
                let mut cache = Thumbnails::new(0, 0);
                let ready = requests
                    .iter()
                    .filter_map(|r| cache.prepare(r).ok().map(|i| (r.key, i)))
                    .collect::<Vec<_>>();
                media.accept(&ready, &requests);
                let a = pixels(&reference, &media);
                let b = pixels(&declared, &media);
                let maximum = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                assert!(
                    maximum <= 1,
                    "canvas {:?}, scale {scale}, offset {offset}: max delta {maximum}",
                    piece.key
                );
                assert_eq!(
                    reference.layout_report()["controls"],
                    declared.layout_report()["controls"]
                );
            }
        }
    }
}

#[test]
fn hover_handoff_keeps_departing_card_above_the_scroll_content() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = Pages::new(Default::default()).unwrap();
        p.fixture();
        p.set_page(Page::Builds);
        p.configure(
            ((1000.0 * scale) as u32, (1100.0 * scale) as u32),
            (400, 1100),
            scale,
        );
        p.capture_hover("build:mod:mod-1").unwrap();
        let original = p.ui.bounds("build.topology");
        let r = p.ui.bounds("build:mod:mod-2").unwrap();
        p.pointer(
            false,
            ((r.x + r.width / 2) as f64, (r.y + r.height / 2) as f64),
        );
        p.dirty = true;
        p.layout();
        p.builds
            .preview
            .tick(Instant::now() + Duration::from_millis(50));
        p.dirty = true;
        p.layout();
        let frames = p.build_view.content.previews.order();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames
                .keys()
                .iter()
                .filter(|&&id| p.build_view.content.previews.get(id).unwrap().active)
                .count(),
            1
        );
        assert_eq!(p.ui.bounds("build.topology"), original);
        assert!(p.ui.bounds("build:mod-preview").unwrap().x > r.x.saturating_sub(p.ui.px(20.0)));
        p.builds
            .preview
            .tick(Instant::now() + Duration::from_millis(250));
        p.dirty = true;
        p.layout();
        assert_eq!(p.build_view.content.previews.order().len(), 1);
        p.builds.dialog = Some("rename".into());
        p.dirty = true;
        p.layout();
        assert!(p.ui.has_modal());
        p.builds
            .preview
            .tick(Instant::now() + Duration::from_millis(250));
        p.dirty = true;
        p.layout();
        assert_eq!(p.build_view.content.previews.order().len(), 0);
    }
}
