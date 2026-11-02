use super::*;

fn node(name: &'static str) -> Key {
    key("market.toolbar", name)
}

#[derive(PartialEq)]
pub(super) struct ToolbarView {
    pub model: Model,
    pub fonts: Fonts,
}

impl View for ToolbarView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.read();
        let state = self.model.account.read();
        let control = |name, bounds| place(slot(node(name)), bounds).interactive();
        let r = m.bounds(273.0, 138.0, 14.0, 14.0);
        let pad = 2.0;
        ui! {
            ZStack() {
                place(fitted_label(&state.name, self.fonts.regular.clone(), 17.0, TEXT, false, m.scale),
                    m.bounds(14.0, 8.0, m.width - 375.0, 35.0))
                control("presence", m.bounds(m.width - 350.0, 10.0, 130.0, 31.0))
                control("refresh", m.bounds(m.width - 210.0, 10.0, 90.0, 31.0))
                control("logout", m.bounds(m.width - 110.0, 10.0, 95.0, 31.0))
                place(fitted_label("My orders", self.fonts.bold.clone(), 17.0, TEXT, false, m.scale),
                    m.bounds(14.0, 52.0, 130.0, 28.0))
                control("search", m.bounds(14.0, 89.0, m.width - 335.0, 32.0))
                control("side", m.bounds(m.width - 311.0, 89.0, 110.0, 32.0))
                control("visibility", m.bounds(m.width - 191.0, 89.0, 177.0, 32.0))
                control("category", m.bounds(14.0, 130.0, 120.0, 30.0))
                control("sort", m.bounds(144.0, 130.0, 115.0, 30.0))
                place(slot(node("direction.icon")), Bounds::new(r.x - pad, r.y - pad, r.width + pad * 2.0, r.height + pad * 2.0))
                control("direction", m.bounds(265.0, 130.0, 30.0, 30.0))
                control("stock", m.bounds(304.0, 130.0, 135.0, 30.0))
                control("bulk", m.bounds(m.width - 190.0, m.actions_y(), 176.0, 30.0))
            }.alignment(Alignment::Start).frame_aligned(m.width, m.actions_y() + 30.0,
                retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING).role(Role::Toolbar)
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, m: &Market, busy: bool) {
    ui.view_control(node("presence"), |ui, r| {
        ui.choice(
            "market:presence",
            r,
            text(&m.account["presence"], "mode"),
            &[
                ("auto", "Auto"),
                ("ingame", "In game"),
                ("online", "Online"),
                ("invisible", "Offline"),
            ],
        )
    });
    ui.view_control(node("refresh"), |ui, r| {
        ui.button("market:refresh", r, "Refresh", false, !busy)
    });
    ui.view_control(node("logout"), |ui, r| {
        ui.button("market:logout", r, "Log out", false, !busy)
    });
    ui.view_control(node("search"), |ui, r| {
        ui.styled_search_field(
            "market:search",
            r,
            &m.search,
            "Filter orders",
            crate::presentation::widgets::search::Style::Field,
        );
    });
    ui.view_control(node("side"), |ui, r| {
        ui.choice(
            "market:side",
            r,
            &m.order_side,
            &[
                ("all", "All orders"),
                ("sell", "Sell orders"),
                ("buy", "Buy orders"),
            ],
        )
    });
    ui.view_control(node("visibility"), |ui, r| {
        ui.choice(
            "market:visibility",
            r,
            &m.visibility,
            &[
                ("all", "Public and hidden"),
                ("public", "Public"),
                ("hidden", "Hidden"),
            ],
        )
    });
    ui.view_control(node("category"), |ui, r| {
        ui.choice(
            "market:category",
            r,
            &m.category,
            &[
                ("all", "All items"),
                ("parts", "Parts"),
                ("sets", "Sets"),
                ("mods", "Mods"),
                ("arcanes", "Arcanes"),
                ("relics", "Relics"),
                ("misc", "Misc"),
            ],
        )
    });
    ui.view_control(node("sort"), |ui, r| {
        ui.choice(
            "market:sort",
            r,
            &m.sort,
            &[
                ("name", "Name"),
                ("price", "Price"),
                ("quantity", "Quantity"),
            ],
        )
    });
    ui.view_control(node("direction.icon"), |ui, r| {
        let pad = ui.px(2.0);
        ui.arrow(
            Rect {
                x: pad,
                y: pad,
                width: r.width - pad * 2,
                height: r.height - pad * 2,
            },
            m.descending,
            TEXT,
        )
    });
    ui.view_hit(node("direction"), "market:direction", "Reverse order", true);
    ui.view_control(node("stock"), |ui, r| {
        ui.choice(
            "market:stock",
            r,
            &m.stock,
            &[
                ("all", "Any inventory"),
                ("available", "Available"),
                ("missing", "Missing"),
            ],
        )
    });
    ui.view_control(node("bulk"), |ui, r| {
        ui.choice(
            "market:bulk",
            r,
            "actions",
            &[
                ("actions", "Actions"),
                ("public", "Set all public"),
                ("hidden", "Set all hidden"),
                ("delete", "Delete selected..."),
                ("filtered", "Delete filtered..."),
                ("reconcile", "Match inventory..."),
            ],
        )
    });
}
