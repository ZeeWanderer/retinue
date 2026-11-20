#[cfg(any(test, feature = "reference-frontend"))]
use taffy::AlignItems;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment {
    Start,
    #[default]
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Distribution {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Distribution {
    pub(crate) fn taffy(self) -> taffy::JustifyContent {
        match self {
            Self::Start => taffy::JustifyContent::START,
            Self::Center => taffy::JustifyContent::CENTER,
            Self::End => taffy::JustifyContent::END,
            Self::SpaceBetween => taffy::JustifyContent::SPACE_BETWEEN,
            Self::SpaceAround => taffy::JustifyContent::SPACE_AROUND,
            Self::SpaceEvenly => taffy::JustifyContent::SPACE_EVENLY,
        }
    }
}

#[cfg(any(test, feature = "reference-frontend"))]
impl Alignment {
    pub(crate) fn taffy(self) -> AlignItems {
        match self {
            Self::Start => AlignItems::START,
            Self::Center => AlignItems::CENTER,
            Self::End => AlignItems::END,
            Self::Stretch => AlignItems::STRETCH,
        }
    }
}
