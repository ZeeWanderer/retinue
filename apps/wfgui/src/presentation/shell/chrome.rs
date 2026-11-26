use super::*;
use retinue_render::resources::ResourceId;
use std::cell::RefCell;

// Commands remain in the shell composition; page and right-rail data are recorded independently.
#[derive(Default)]
pub(super) struct RecordingInputs(RefCell<Option<(Inputs, String)>>);

#[derive(PartialEq)]
struct Inputs {
    geometry: Geometry,
    controls: [(Rect, bool); composition::CONTROLS.len()],
    scale: f32,
    ui_scale: f32,
    page: Page,
    hovered: Option<Control>,
    pressed: Option<Control>,
    focused: Option<Control>,
    tooltip: bool,
    collapsed: (bool, bool),
    maximized: bool,
    preference_error: bool,
    rank: Option<u64>,
    emblem: Option<ResourceId>,
    artwork: Option<ResourceId>,
}

impl RecordingInputs {
    pub(super) fn changed(&self, shell: &Shell) -> bool {
        let profile = shell.foundry.snapshot.profile.as_ref();
        let name = profile.map_or("", |p| p.player_name.as_str());
        let inputs = Inputs {
            geometry: shell.geometry,
            controls: composition::CONTROLS.map(|c| {
                (
                    shell.composition.rect(composition::control_key(c)),
                    shell.control_enabled(c),
                )
            }),
            scale: shell.scale(),
            ui_scale: shell.settings.ui_scale,
            page: shell.page,
            hovered: shell.hover,
            pressed: match shell.press {
                Some(PressOwner::Chrome(c)) => Some(c),
                _ => None,
            },
            focused: match shell.focus {
                Some(Focus::Chrome(c)) if shell.focus_visible.shows(true, false) => Some(c),
                _ => None,
            },
            tooltip: shell.tooltip,
            collapsed: (shell.left_collapsed, shell.right_collapsed),
            maximized: shell.maximized,
            preference_error: shell.settings.preference_error.is_some(),
            rank: profile.and_then(|p| p.player_level),
            emblem: shell.foundry.rank_image().map(|i| i.id()),
            artwork: shell.icons.gradient.as_ref().map(|i| i.image.id()),
        };
        let mut previous = self.0.borrow_mut();
        if previous
            .as_ref()
            .is_some_and(|(p, n)| p == &inputs && n == name)
        {
            return false;
        }
        *previous = Some((inputs, name.to_owned()));
        true
    }
}
