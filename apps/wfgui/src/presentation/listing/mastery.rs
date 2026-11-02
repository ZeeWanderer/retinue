use super::{card_layout::Positioned, card_layout::place, *};
use crate::{presentation::widgets::AssetImage, presentation::widgets::ComponentImage};
use retinue_render::{painter::CachedFont, painter::text_width};

fn child(id: Key, name: &'static str) -> Key {
    Key::new(name, id.value)
}
fn part(id: Key, index: usize) -> Key {
    key("mastery.component", (id, index))
}

fn chance(item: &ListingItem) -> String {
    let chance = item.relic_probability * 100.0;
    if chance == 0.0 {
        "0%".into()
    } else if chance < 1.0 {
        format!("{chance:.2}%")
    } else {
        format!("{chance:.1}%")
    }
}

#[derive(Clone)]
struct Geometry {
    image: Bounds,
    title: Bounds,
    star: Bounds,
    xp: Bounds,
    level: Bounds,
    owned: Bounds,
    owned_label: Bounds,
    chance: Bounds,
    relic: Bounds,
    price: Bounds,
    platinum: Bounds,
    parts: Vec<Bounds>,
}

impl Geometry {
    fn new(
        cell: &Cell,
        size: LayoutSize,
        scale: f32,
        context: &CardContext,
        font: &CachedFont,
    ) -> Self {
        let (w, h) = (size.width, size.height);
        let rect = |x: f32, y: f32, w: f32, h: f32| {
            Bounds::new(x.max(0.0), y.max(0.0), w.max(0.0), h.max(0.0))
        };
        let measure = |s: &str, size| text_width(font, s, size * scale) / scale;
        let middle = (w - 201.0).max(1.0);
        let title = measure(&cell.item.name, 17.0).min((middle - 25.0).max(1.0));
        let title_x = 84.0 + (middle - title - 25.0) / 2.0;
        let owned = measure("Owned", 14.0);
        let owned_x = w - 117.0 + (109.0 - owned - 24.0) / 2.0;
        let cw = measure(&chance(&cell.item), 16.0);
        let pw = measure(&price(cell.cost, context.prices_ready), 16.0);
        let left = 84.0 + (middle - cw - pw - 87.0) / 2.0;
        let count = if cell.item.owned {
            0
        } else {
            cell.item.components.len().min(6)
        };
        let columns = count.clamp(1, 3);
        Self {
            image: rect(8.0, (h - 72.0) / 2.0, 72.0, 72.0),
            title: rect(title_x + 25.0, 10.0, title, 27.0),
            star: rect(title_x, 14.0, 20.0, 20.0),
            xp: rect(8.0, (h + 72.0) / 2.0 - 21.0, 72.0, 21.0),
            level: rect(84.0, 41.0, middle, 24.0),
            owned: rect(owned_x, (h - 20.0) / 2.0, 20.0, 20.0),
            owned_label: rect(owned_x + 24.0, (h - 24.0) / 2.0, owned, 24.0),
            chance: rect(left, 41.0, cw + 1.0, 28.0),
            relic: rect(left + cw + 2.0, 41.0, 28.0, 28.0),
            price: rect(left + cw + 57.0, 41.0, pw + 1.0, 28.0),
            platinum: rect(left + cw + pw + 59.0, 41.0, 28.0, 28.0),
            parts: (0..count)
                .map(|i| {
                    let n = (count - i / columns * columns).min(columns);
                    let width = n as f32 * 30.0 + (n - 1) as f32 * 4.0;
                    rect(
                        w - 117.0 + (109.0 - width) / 2.0 + (i % columns) as f32 * 34.0,
                        (h - if count > 3 { 64.0 } else { 30.0 }) / 2.0
                            + (i / columns) as f32 * 34.0,
                        30.0,
                        30.0,
                    )
                })
                .collect(),
        }
    }
}

#[derive(PartialEq)]
pub(super) struct MasteryCardView {
    pub id: Key,
    pub cell: State<Cell>,
    pub metrics: State<Metrics>,
    pub context: State<CardContext>,
    pub fonts: Fonts,
}

impl View for MasteryCardView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let id = self.id;
        let cell = self.cell.read();
        let m = self.metrics.read();
        let context = self.context.read();
        let size = m.uniform_grid().cell();
        let g = Geometry::new(&cell, size, m.scale, &context, &self.fonts.regular);
        let label = |text: String, size, font, center| {
            fitted_label(text, font, size, TEXT, center, m.scale)
        };
        ui! {
            LayoutView(Positioned::new(size)) {
                fill(if cell.hovered { crate::presentation::style::CARD_HOVER } else { controls::CARD }, 12.0)
                place(AssetImage::new(cell.item.asset.as_ref(), m.px(g.image.width.max(g.image.height))), g.image).id(child(id, "mastery.image"))
                place(AssetImage::icon(if cell.favorite { "favorite_active" } else { "favorite" }, m.px(g.star.width)), g.star).id(child(id, "mastery.favorite")).interactive()
                place(label(cell.item.name.clone(), 17.0, self.fonts.regular.clone(), false), g.title)
                    .id(child(id, "mastery.title")).interactive()
                place(label(format!("+{} XP", cell.item.potential_xp), 14.0, self.fonts.regular.clone(), true)
                    .background(fill([0, 0, 0, 138], 5.0)), g.xp)
                if cell.item.owned {
                    place(label(format!("Level {}/{}", cell.item.rank, cell.item.max_rank), 15.0, self.fonts.light.clone(), true), g.level)
                    place(AssetImage::icon("owned", m.px(g.owned.width)), g.owned).id(child(id, "mastery.owned"))
                    place(fitted_label("Owned", self.fonts.light.clone(), 14.0, crate::presentation::style::OWNED_TEXT, false, m.scale), g.owned_label)
                } else {
                    place(label(chance(&cell.item), 16.0, self.fonts.regular.clone(), false), g.chance)
                    place(AssetImage::icon("axi", m.px(g.relic.width)), g.relic).id(child(id, "mastery.relic"))
                    place(label(price(cell.cost, context.prices_ready), 16.0, self.fonts.regular.clone(), false), g.price)
                    place(AssetImage::icon("platinum", m.px(g.platinum.width)), g.platinum).id(child(id, "mastery.platinum"))
                    for (i, bounds) in g.parts.iter().take(6).enumerate() {
                        let component = &cell.item.components[i];
                        place(ComponentImage { asset: component.asset.clone(), owned: component.blueprint_owned > 0 || (component.required > 0 && component.owned >= component.required),
                            edge: m.px(bounds.width), scale: m.scale }, *bounds).id(part(id, i)).interactive()
                    }
                }
            }.interactive()
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, id: Key, cell: &Cell) {
    let item = &cell.item;
    let index = cell.index;
    ui.view_hit(id, format!("list:item:{index}:menu"), "", true);
    for (id, name) in [
        (child(id, "mastery.image"), "mastery.card.image"),
        (child(id, "mastery.title"), "mastery.card.title"),
    ] {
        if let Some(r) = ui.view_bounds(id) {
            ui.first_region(name, r);
        }
    }
    ui.view_hit(
        child(id, "mastery.favorite"),
        format!("list:item:{index}:favorite"),
        "Favorite",
        true,
    );
    ui.view_hit(
        child(id, "mastery.title"),
        format!("list:item:{index}:menu"),
        &item.name,
        true,
    );
    for (i, component) in item.components.iter().take(6).enumerate() {
        let id = part(id, i);
        if let Some(r) = ui.view_bounds(id) {
            ui.first_region("mastery.card.component", r);
        }
        ui.view_hit(
            id,
            format!("list:item:{index}:component:{i}"),
            &component.full_name,
            true,
        );
    }
}
