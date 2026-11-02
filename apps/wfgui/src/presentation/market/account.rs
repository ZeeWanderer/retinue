use super::*;

fn node(name: &'static str) -> Key {
    Key::new(name, 0)
}

#[derive(PartialEq)]
pub(super) struct AccountView {
    pub model: Model,
    pub fonts: Fonts,
}

impl View for AccountView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.model.metrics.get();
        let state = self.model.account.read();
        let frame = |y, h| {
            m.bounds(
                ((m.width - 360.0) / 2.0).max(18.0),
                y,
                (m.width - 36.0).min(360.0),
                h,
            )
        };
        let label = |text: &str, size, color| {
            fitted_label(
                text,
                self.fonts.regular.clone(),
                size,
                color,
                false,
                m.scale,
            )
        };
        ui! {
            ZStack() {
                place(fitted_label("Warframe.market", self.fonts.bold.clone(), 24.0, TEXT, false, m.scale), frame(42.0, 35.0))
                if state.authenticated.is_none() {
                    if let Some(error) = &state.account_error {
                        place(label(error, 14.0, BAD), frame(88.0, 65.0))
                        place(slot(node("market.account.retry")), frame(164.0, 34.0)).interactive()
                    } else {
                        place(label("Checking account...", 15.0, MUTED), frame(82.0, 32.0))
                        place(slot(node("market.account.progress")), frame(125.0, 3.0))
                    }
                } else {
                    place(label("Sign in to manage your orders", 15.0, MUTED), frame(82.0, 46.0))
                    place(slot(node("market.account.email")), frame(143.0, 36.0)).interactive()
                    place(slot(node("market.account.password")), frame(190.0, 36.0)).interactive()
                    place(slot(node("market.account.login")), frame(239.0, 36.0)).interactive()
                    place(slot(node("market.account.browse")), frame(292.0, 34.0)).interactive()
                    if let Some(error) = &state.error {
                        place(label(error, 14.0, BAD), frame(340.0, 70.0))
                    }
                }
            }.alignment(Alignment::Start).frame_aligned(m.width, m.height, retinue_ui::layout::anchors::AnchorPoint::TOP_LEADING)
        }
    }
}

pub(super) fn bind(ui: &mut ControlHost, market: &Market, state: &AccountState, connected: bool) {
    ui.view_control(node("market.account.retry"), |ui, r| {
        ui.button("market:refresh", r, "Retry", false, connected)
    });
    if let Some(r) = ui.view_bounds(node("market.account.progress")) {
        ui.region("market.account.loading", r);
    }
    ui.view_control(node("market.account.progress"), |ui, r| ui.progress(r));
    ui.view_control(node("market.account.email"), |ui, r| {
        ui.field("market:email", r, &market.email, "Email")
    });
    ui.view_control(node("market.account.password"), |ui, r| {
        ui.password("market:password", r, &market.password)
    });
    ui.view_control(node("market.account.login"), |ui, r| {
        ui.button(
            "market:login",
            r,
            if state.busy {
                "Signing in..."
            } else {
                "Sign in"
            },
            true,
            !state.busy && !market.email.is_empty() && !market.password.is_empty(),
        )
    });
    ui.view_control(node("market.account.browse"), |ui, r| {
        ui.button("market:browse", r, "Browse public listings", false, true)
    });
}
