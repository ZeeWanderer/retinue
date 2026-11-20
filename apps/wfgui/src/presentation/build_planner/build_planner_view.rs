use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct BuildPageView<E, D, G, C, P, M> {
    pub metrics: State<Metrics>,
    pub state: State<PageState>,
    pub fonts: Fonts,
    pub equipment: E,
    pub discover: D,
    pub groups: G,
    pub content: C,
    pub previews: P,
    pub dialog: M,
}

#[view]
impl<
    E: View + Clone + PartialEq,
    D: View + Clone + PartialEq,
    G: View + Clone + PartialEq,
    C: View + Clone + PartialEq,
    P: View + Clone + PartialEq,
    M: View + Clone + PartialEq,
> View for BuildPageView<E, D, G, C, P, M>
{
    fn body(&self, _: &mut ViewContext<'_>) -> impl Body {
        let m = self.metrics.get();
        let s = self.state.get();
        ui! {
            ZStack() {
                fill(controls::PANEL, 10.0)
                place(fill(controls::PANEL, 0.0), m.bounds(0.0, 10.0, m.width, m.height - 10.0))
                ZStack() {
                    place(slot(node("warning.icon")), m.bounds(17.0, 17.0, 18.0, 18.0))
                    control(node("warning"), m.bounds(10.0, 10.0, 30.0, 30.0))
                    ForEach(["groups", "equipment", "discover"].into_iter().enumerate(), |(_, id)| node(id), |(i, id)| {
                        control(node(id), m.bounds(
                            46.0 + m.tabs[..i].iter().map(|w| w + 6.0).sum::<f32>(),
                            10.0, m.tabs[i], 32.0))
                    })
                    control(node("account"), m.bounds(m.width - 146.0, 10.0, 132.0, 32.0))
                }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, 42.0)).role(Role::Toolbar)
                match s.tab.as_str() {
                    "equipment" => compose(self.equipment.clone()),
                    "discover" => compose(self.discover.clone()),
                    _ => compose(self.groups.clone()),
                }
                compose(self.content.clone())
                if let Some(error) = &s.error {
                    place(label(&self.fonts, error, 13.0, BAD, false, m.scale), m.bounds(16.0, m.height - 28.0, m.width - 32.0, 24.0))
                }
                if let Some(notice) = &s.notice {
                    place(label(&self.fonts, notice, 13.0, GOOD, false, m.scale), m.bounds(16.0, 48.0, m.width - 32.0, 24.0))
                }
                compose(self.previews.clone())
                compose(self.dialog.clone())
            }.alignment(AnchorPoint::TOP_LEADING).frame(frame(m.width, m.height))
        }
    }
}
