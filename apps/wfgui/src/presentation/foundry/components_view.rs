use super::component_view::ComponentView;
use retinue_ui::authoring::*;

#[derive(PartialEq)]
pub(super) struct ComponentsView(pub Vec<ComponentView>);

#[retinue_ui::authoring::view]
impl View for ComponentsView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        ui! {
            ForEach(self.0.iter().cloned(), ComponentView::key) { |part|
                let key = part.key();
                compose(part).named(key)
            }
        }
    }
}
