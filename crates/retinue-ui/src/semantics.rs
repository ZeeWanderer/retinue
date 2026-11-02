use crate::Key;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    None,
    Window,
    Title,
    Toolbar,
    Content,
    Overlay,
    Control,
    Custom(Key),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Move,
    Resize(ResizeEdge),
    Minimize,
    ToggleMaximized,
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowCapabilities {
    pub movable: bool,
    pub resizable: bool,
    pub minimizable: bool,
    pub maximizable: bool,
    pub closable: bool,
}

impl WindowCapabilities {
    pub const DESKTOP: Self = Self {
        movable: true,
        resizable: true,
        minimizable: true,
        maximizable: true,
        closable: true,
    };

    pub fn supports(self, action: WindowAction) -> bool {
        match action {
            WindowAction::Move => self.movable,
            WindowAction::Resize(_) => self.resizable,
            WindowAction::Minimize => self.minimizable,
            WindowAction::ToggleMaximized => self.maximizable,
            WindowAction::Close => self.closable,
        }
    }
}
