use crate::{UiEvent, desktop, incident};
use retinue_overlay::shortcut::Binding;
pub(crate) use retinue_overlay::shortcut::{Controller, Service};

pub(crate) fn spawn(events: crate::runtime::presentation::Sender) -> Result<Service, String> {
    desktop::ensure_identity()?;
    Service::spawn(
        Binding {
            on_release: false,
            app_id: desktop::APP_ID,
            id: "interaction-mode",
            description: "Toggle overlay interaction mode",
            trigger: "CTRL+Tab",
        },
        move || {
            incident::info("shortcut.trigger", "id=interaction-mode");
            let _ = events.send(UiEvent::InteractionToggle);
        },
        |event, detail| {
            if event == "shortcut.unavailable" {
                incident::warn(event, detail);
            } else {
                incident::info(event, detail);
            }
        },
    )
}
