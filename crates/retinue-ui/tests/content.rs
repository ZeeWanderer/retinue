use retinue_render::{
    Bounds, Color, ImageFit, RasterMode, Renderer, ResidencyPolicy,
    damage::Damage,
    painter::{Painter, RasterImage},
};
use retinue_ui::{
    drawing::text::{Paragraph, TextAlignment},
    image::{ImageRendition, ImageStatus, ImageUpdateError},
    prelude::*,
    runtime::ViewTree,
};
use std::rc::Rc;

fn pixels(tree: &ViewTree, renderer: &mut Renderer, image: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Replay);
    let mut painter = Painter::with_threads(image, size.0, size.1, 2.try_into().unwrap()).unwrap();
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut painter, bounds, Color::rgb(0x202020));
    }
    painter.finish().unwrap();
}

fn full(tree: &ViewTree) -> Vec<u8> {
    let size = tree.frame().size();
    let mut image = vec![0; size.0 as usize * size.1 as usize * 4];
    pixels(
        tree,
        &mut Renderer::new(ResidencyPolicy::desktop(0)),
        &mut image,
        Damage::Full,
    );
    image
}

fn source(color: u32) -> retinue_render::ImageSource {
    Rc::new(
        RasterImage::render(8, 8, |painter| {
            painter.fill_rounded_rect(0, 0, 8, 8, 0, Color::rgb(color).bgra())
        })
        .unwrap(),
    )
    .into()
}

#[test]
fn paragraph_wrap_limit_and_borrowed_layout_cache_are_shared_by_drawing() {
    let style = TextStyle::default();
    let paragraph = Paragraph::new("first line\n\nlast line", &style, None);
    let layout = paragraph.layout(200.0);
    assert_eq!(layout.lines, ["first line", "", "last line"]);
    assert!(Rc::ptr_eq(&layout, &paragraph.layout(200.0)));
    assert!(
        Rc::ptr_eq(&layout, &paragraph.layout(220.0)),
        "unwrapped text is independent of excess available width"
    );
    let clipped = Paragraph::new("the long text has more than one line", &style, Some(1));
    let layout = clipped.layout(80.0);
    assert_eq!(layout.lines.len(), 1);
    assert!(layout.lines[0].ends_with('\u{2026}'));
    assert!(layout.size.width <= 80.0);
    let combined = Paragraph::new("a\u{301}a\u{301}a\u{301}", &style, None);
    assert_eq!(
        combined.layout(0.0).lines,
        ["a\u{301}", "a\u{301}", "a\u{301}"]
    );
}

#[test]
fn text_measures_without_canvas_and_color_updates_do_not_relayout() {
    let mut tree = ViewTree::new(Text::new("Hello, world"), || {});
    tree.update((320, 180), 1.0).unwrap();
    let measured = tree
        .scene()
        .bounds(tree.root().unwrap().key())
        .unwrap()
        .layout;
    let expected = Paragraph::new("Hello, world", &TextStyle::default(), None).layout(320.0);
    assert!((measured.width - expected.size.width).abs() < 0.001);
    assert!((measured.height - expected.size.height).abs() < 0.001);
    let mut style = TextStyle::default();
    style.color = Color::rgb(0xee3344);
    tree.environment().set::<Typography>(style);
    tree.update((320, 180), 1.0).unwrap();
    assert_eq!(tree.stats.bodies, 0);
    assert_eq!(tree.stats.projected, 1);
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert_eq!(tree.scene().stats.recordings, 1);
}

#[derive(PartialEq)]
struct DirectText {
    text: String,
    style: TextStyle,
}
impl CanvasContent for DirectText {
    fn draw(&self, context: &mut DrawingContext) {
        let paragraph = Paragraph::new(self.text.as_str(), &self.style, None);
        paragraph.layout(context.bounds.width).draw(
            context,
            context.bounds,
            self.style.color,
            TextAlignment::Leading,
        );
    }
}

#[test]
fn direct_and_composed_text_have_identical_metrics_and_pixels_at_all_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let size = ((320.0 * scale) as u32, (180.0 * scale) as u32);
        let text = "Reliable resource updates";
        let style = TextStyle::default();
        let measured = Paragraph::new(text, &style, None).layout(320.0).size;
        let mut composed = ViewTree::new(
            Text::new(text).frame(measured.width, measured.height),
            || {},
        );
        let mut direct = ViewTree::new(
            Canvas::new(DirectText {
                text: text.into(),
                style,
            })
            .frame(measured.width, measured.height),
            || {},
        );
        composed.update(size, scale).unwrap();
        direct.update(size, scale).unwrap();
        assert!(
            full(&composed) == full(&direct),
            "direct text differs at {scale}"
        );
    }
}

#[test]
fn image_updates_reject_old_jobs_and_lower_quality_without_erasing_usable_pixels() {
    let image = ImageResource::default();
    let first = image.begin();
    let a = source(0x447722);
    image
        .publish(
            first,
            ImageRendition {
                revision: 1,
                quality: 5,
                pixels: a.clone(),
                fresh: false,
            },
        )
        .unwrap();
    assert!(image.pixels().as_ref() == Some(&a));
    assert!(
        !image
            .publish(
                first,
                ImageRendition {
                    revision: 1,
                    quality: 2,
                    pixels: source(0xff0000),
                    fresh: true
                }
            )
            .unwrap()
    );
    let next = image.begin();
    assert!(image.pixels().as_ref() == Some(&a));
    assert_eq!(
        image.complete(first),
        Err(ImageUpdateError::ObsoleteRequest)
    );
    assert_eq!(
        image.set_metadata(first, LayoutSize::new(12.0, 12.0)),
        Err(ImageUpdateError::ObsoleteRequest)
    );
    assert_eq!(
        image.fail(first, "late error"),
        Err(ImageUpdateError::ObsoleteRequest)
    );
    image.fail(next, "offline").unwrap();
    assert!(matches!(image.status(), ImageStatus::Failed(_)));
    assert!(image.pixels().as_ref() == Some(&a));
    let unrelated = ImageResource::default().begin();
    assert_eq!(
        image.complete(unrelated),
        Err(ImageUpdateError::ObsoleteRequest)
    );
}

#[test]
fn resizable_images_ignore_metadata_status_and_typography_but_repair_new_pixels() {
    for scale in [1.0, 1.25, 2.0] {
        let image = ImageResource::default();
        let request = image.begin();
        let mut tree = ViewTree::new(
            Image::new(image.clone())
                .resizable()
                .fit(ImageFit::Stretch)
                .frame(80.0, 60.0),
            || {},
        );
        let size = ((200.0 * scale) as u32, (120.0 * scale) as u32);
        tree.update(size, scale).unwrap();
        let mut output = full(&tree);
        let mut renderer = Renderer::new(ResidencyPolicy::desktop(0));
        image
            .set_metadata(request, LayoutSize::new(800.0, 600.0))
            .unwrap();
        image.complete(request).unwrap();
        assert!(!tree.needs_update());
        tree.environment().set::<Typography>(TextStyle::new(
            TextStyle::default().font,
            30.0,
            Color::rgb(0xaa3311),
        ));
        assert!(!tree.needs_update());
        for (quality, color) in [(0, 0x00bb55), (1, 0x55aaff), (2, 0xffaaaa)] {
            image
                .publish(
                    request,
                    ImageRendition {
                        revision: 1,
                        quality,
                        pixels: source(color),
                        fresh: quality == 2,
                    },
                )
                .unwrap();
            let damage = tree.update(size, scale).unwrap();
            assert_eq!(tree.stats.bodies, 0);
            assert_eq!(tree.scene().stats.layout_computations, 0);
            pixels(&tree, &mut renderer, &mut output, damage);
            assert!(
                output == full(&tree),
                "image repair differs at {scale}/{quality}"
            );
        }
        tree.set_root(empty());
        tree.update(size, scale).unwrap();
        image
            .publish(
                request,
                ImageRendition {
                    revision: 2,
                    quality: 0,
                    pixels: source(0xff00ff),
                    fresh: false,
                },
            )
            .unwrap();
        assert!(!tree.needs_update(), "removed image remains subscribed");
    }
}

#[test]
fn intrinsic_metadata_changes_layout_independently_of_pixels() {
    let image = ImageResource::default();
    let mut tree = ViewTree::new(
        Image::new(image.clone()).placeholder_size(30.0, 20.0),
        || {},
    );
    tree.update((320, 180), 1.0).unwrap();
    let key = tree.root().unwrap().key();
    assert_eq!(
        tree.scene().bounds(key).unwrap().layout,
        Bounds::new(0.0, 0.0, 30.0, 20.0)
    );
    let request = image.begin();
    assert!(!tree.needs_update());
    image
        .set_metadata(request, LayoutSize::new(60.0, 40.0))
        .unwrap();
    tree.update((320, 180), 1.0).unwrap();
    assert_eq!(
        tree.scene().bounds(key).unwrap().layout,
        Bounds::new(0.0, 0.0, 60.0, 40.0)
    );
}
