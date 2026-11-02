use retinue_render::{
    Key, RasterMode, Renderer, ResidencyPolicy, damage::Damage, painter::Painter,
};
use retinue_ui::{layout::grid::UniformGrid, prelude::*, runtime::ViewTree};

#[derive(Clone, PartialEq)]
struct GridModel {
    rows: Items<Color>,
    offset: State<f32>,
}
#[derive(PartialEq)]
struct Row(State<Color>);
impl View for Row {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        Rectangle::new(self.0.get()).corner_radius(5.0)
    }
}
#[derive(PartialEq)]
struct GridView(GridModel);
impl View for GridView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let grid = UniformGrid::new(
            3,
            LayoutSize::new(80.0, 50.0),
            LayoutSize::new(8.0, 8.0),
            Insets::default(),
        )
        .unwrap();
        let offset = self.0.offset.get();
        let order = self.0.rows.order();
        let visible = grid.visible(order.len(), offset, 160.0, 1).unwrap();
        let content = LayoutView::new(
            grid.arrange(order.clone()),
            order.keys()[visible]
                .iter()
                .map(|key| Row(self.0.rows.item(*key).unwrap()).id(*key))
                .collect::<Vec<_>>(),
        )
        .reuse(retinue_render::CacheHint::Stable)
        .id("content");
        ScrollViewport::new(content)
            .position(0.0, offset)
            .content_insets(Insets {
                left: 7.0,
                right: 9.0,
                top: 3.0,
                bottom: 12.0,
            })
            .frame(272.0, 160.0)
            .id("viewport")
    }
}

fn model() -> GridModel {
    let rows = Items::default();
    rows.replace((0..1000).map(|i| {
        (
            Key::new("row", i),
            Color::rgba((i * 19) as u8, 120, 180, 200),
        )
    }))
    .unwrap();
    GridModel {
        rows,
        offset: State::new(0.0),
    }
}

#[test]
fn virtual_grid_retains_rows_and_scroll_changes_no_layout_or_recording() {
    let model = model();
    let mut tree = ViewTree::new(GridView(model.clone()), || {});
    tree.update((272, 160), 1.0).unwrap();
    let row = tree.unique(Key::new("row", 0)).unwrap().unwrap();
    assert!(tree.node_count() < 50);
    model.offset.set(1.0);
    tree.update((272, 160), 1.0).unwrap();
    assert_eq!(tree.unique(Key::new("row", 0)).unwrap(), Some(row));
    assert_eq!(tree.scene().stats.layout_computations, 0);
    assert_eq!(tree.scene().stats.recordings, 0);
    assert_eq!(
        tree.stats.projected, 1,
        "scroll only lowers its offset node"
    );
    let bounds = tree.geometry(row).unwrap();
    assert_eq!((bounds.hit.x, bounds.hit.y), (7, 2));
    let group = tree
        .frame()
        .groups()
        .iter()
        .find(|group| group.collection().is_some())
        .unwrap();
    let context = group.collection().unwrap();
    assert_eq!(context.origin, (7, 2));
    assert_eq!(context.rows[0].key, Key::new("row", 0));
    assert_eq!(context.rows[0].instances.len(), 3);
    model.offset.set(800.0);
    tree.update((272, 160), 1.0).unwrap();
    assert!(tree.unique(Key::new("row", 0)).unwrap().is_none());
    assert!(tree.node_count() < 50);
}

fn paint(tree: &ViewTree, renderer: &mut Renderer, pixels: &mut [u8], damage: Damage) {
    let size = tree.frame().size();
    renderer.prepare(tree.frame(), RasterMode::Explicit);
    let mut p = Painter::with_threads(pixels, size.0, size.1, 4.try_into().unwrap()).unwrap();
    p.repair(&damage.regions(size));
    if let Some(bounds) = damage.bounds(size) {
        renderer.paint(tree.frame(), &mut p, bounds, Color::rgb(0x181818));
    }
    p.finish().unwrap();
}

#[test]
fn scrolling_recycling_and_content_changes_repair_like_a_new_tree() {
    for scale in [1.0, 1.25, 2.0] {
        let model = model();
        let size = ((272.0 * scale) as u32, (160.0 * scale) as u32);
        let mut tree = ViewTree::new(GridView(model.clone()), || {});
        let mut renderer = Renderer::with_layers(
            ResidencyPolicy::desktop(4 * 1024 * 1024),
            retinue_render::LayerPolicy {
                stable_repaints: 1,
                min_work: 1,
                ..retinue_render::LayerPolicy::desktop()
            },
        );
        let mut pixels = vec![0; size.0 as usize * size.1 as usize * 4];
        tree.update(size, scale).unwrap();
        paint(&tree, &mut renderer, &mut pixels, Damage::Full);
        let mut reused = false;
        for offset in [
            1.0, 2.0, 3.0, 10.0, 14.0, 62.0, 80.0, 115.0, 200.0, 85.0, 1.0,
        ] {
            model.offset.set(offset);
            model
                .rows
                .set(Key::new("row", 4), Color::rgb((offset as u32) * 301));
            let damage = tree.update(size, scale).unwrap();
            paint(&tree, &mut renderer, &mut pixels, damage);
            reused |= renderer.stats.layer_bytes > 0;
            let mut fresh = ViewTree::new(GridView(model.clone()), || {});
            fresh.update(size, scale).unwrap();
            let mut full = vec![0; pixels.len()];
            let mut direct = Renderer::new(ResidencyPolicy::desktop(0));
            direct.set_optimizations(false);
            paint(&fresh, &mut direct, &mut full, Damage::Full);
            assert!(
                pixels == full,
                "scroll repair differs at offset {offset}, scale {scale}"
            );
        }
        assert!(
            reused,
            "grid row hints never reached raster reuse at scale {scale}"
        );
    }
}
