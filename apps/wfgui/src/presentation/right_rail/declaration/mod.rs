mod loading_view;
mod rail_view;
use crate::{
    presentation::controls, presentation::controls::BAD, presentation::controls::ControlHost,
    presentation::controls::MUTED, presentation::controls::TEXT, presentation::settings::rgb,
    presentation::widgets::fill, presentation::widgets::label, presentation::widgets::place,
    presentation::widgets::slot,
};
use retinue_render::{Bounds, Color, Key, painter::CachedFont};
use retinue_ui::{layout::anchors::AnchorPoint, prelude::*};
use std::rc::Rc;

pub const NOTIFICATION: Key = Key::new("rail.notification.control", 0);
pub const SCROLL: Key = Key::new("rail.fissures.scroll", 0);
const RETRY: Key = Key::new("rail.retry", 0);
const LOADING: Key = Key::new("rail.loading", 0);

pub(in crate::presentation) struct Rail {
    area: State<(f32, f32)>,
    market: State<bool>,
    ready: State<bool>,
    error: State<String>,
    pub installed: bool,
}

impl Rail {
    pub fn new() -> Self {
        Self {
            area: State::new((0.0, 0.0)),
            market: State::new(false),
            ready: State::new(false),
            error: State::new(String::new()),
            installed: false,
        }
    }

    pub fn sync(&self, ui: &ControlHost, market: bool, ready: bool, error: Option<&str>) {
        self.area
            .set((ui.size.0 as f32 / ui.scale, ui.size.1 as f32 / ui.scale));
        self.market.set(market);
        self.ready.set(ready);
        self.error.set(error.unwrap_or_default().to_owned());
    }

    pub fn view(&self, events: Element, market: Element, regular: Rc<CachedFont>) -> Element {
        rail_view::RailView {
            area: self.area.clone(),
            market: self.market.clone(),
            ready: self.ready.clone(),
            error: self.error.clone(),
            events,
            market_content: market,
            font: TextStyle::new(regular, 14.0, Color::from_bgra(TEXT)),
        }
        .into_element()
    }

    pub fn bind(&self, ui: &mut ControlHost, market: bool, connected: bool) {
        for (i, (id, name, icon)) in [
            ("events", "Timers & Events", "world"),
            ("market", "WFMarket", "market"),
        ]
        .into_iter()
        .enumerate()
        {
            ui.view_control(Key::new("rail.tab", i as u64), |ui, rect| {
                let w = rect.width as f32 / ui.scale;
                let selected = (i == 1) == market;
                let width = ui.measure(name, 16.0, false);
                let start = (w - width - 24.0).max(0.0) / 2.0;
                ui.tinted_icon(
                    ui.rect(start, 9.0, 19.0, 19.0),
                    icon,
                    if selected { TEXT } else { MUTED },
                );
                ui.text(
                    ui.rect(start + 24.0, 4.0, width + 1.0, 30.0),
                    name,
                    16.0,
                    if selected { TEXT } else { MUTED },
                );
                if selected {
                    ui.fill(ui.rect(7.0, 33.0, (w - 14.0).max(0.0), 2.0), 0.0, TEXT);
                }
                ui.hit(format!("rail:tab:{id}"), rect, name, true);
            });
        }
        ui.view_control(RETRY, |ui, rect| {
            ui.button("rail:retry", rect, "Retry", false, connected)
        });
        if let Some(bounds) = ui.view_bounds(LOADING) {
            ui.region("rail.activity.loading", bounds);
        }
    }
}
