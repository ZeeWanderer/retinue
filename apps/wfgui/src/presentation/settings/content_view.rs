use super::{CONTENT, label_view::LabelView, section_view::SectionView};
use retinue_render::Key;
use retinue_ui::{authoring::*, layout::Insets};

#[derive(Clone, PartialEq)]
pub(super) enum Item {
    Label(LabelView),
    Section(SectionView),
}

impl Item {
    fn id(&self) -> Key {
        match self {
            Self::Label(view) => view.id,
            Self::Section(view) => view.id,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ContentView {
    pub items: Vec<Item>,
    pub scale: f32,
}

#[view]
impl View for ContentView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            VStack(12.0 * self.scale) {
                ForEach(self.items.iter(), |item| item.id()) { |item|
                    match item {
                        Item::Label(view) => compose(view.clone()),
                        Item::Section(view) => compose(view.clone()),
                    }
                }
            }.stretch_children()
                .padding(Insets { bottom: 24.0 * self.scale, ..Default::default() }).named(CONTENT)
        }
    }
}
