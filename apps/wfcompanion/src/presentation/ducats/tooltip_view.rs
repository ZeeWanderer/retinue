use super::*;
use retinue_render::{Bounds, Command, ImageFit, ImageSource};
use retinue_ui::{drawing::DrawList, layout::anchors::AnchorPoint};

#[derive(PartialEq)]
pub(super) struct TooltipView {
    pub metrics: State<Metrics>,
    pub price: State<Option<String>>,
    pub style: TextStyle,
    pub images: [ImageSource; 4],
}

impl View for TooltipView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let m = self.metrics.get();
        let image = |index: usize, optical: f32, key| {
            let drawn = (optical * m.scale).round() as u32;
            Canvas::new(DrawList::from(vec![Command::Image {
                bounds: Bounds::new(
                    ((m.icon - drawn) / 2) as f32,
                    (m.row.height.saturating_sub(drawn) / 2) as f32,
                    drawn as f32,
                    drawn as f32,
                ),
                image: self.images[index].clone(),
                fit: ImageFit::Contain,
            }]))
            .frame(m.icon as f32, m.row.height as f32)
            .id(key_for(key))
        };
        ui! {
            HStack(m.gap as f32) {
                if m.visible[0] {
                    image(0, 18.0, 1)
                    price_view::PriceView {
                        text: self.price.clone(),
                        style: TextStyle { size: self.style.size * m.scale, ..self.style.clone() },
                    }.frame(m.price_width as f32, m.row.height as f32).id(key_for(2))
                        .padding(Insets { right: m.gap as f32, ..Insets::default() })
                }
                if m.visible[1] { image(1, 14.0, 3) }
                if m.visible[2] { image(2, 18.0, 4) }
                if m.visible[3] { image(3, 18.0, 5) }
            }.frame_aligned(m.row.width as f32, m.row.height as f32, AnchorPoint::TOP_LEADING)
                .offset(m.row.x as f32, m.row.y as f32).id(key_for(0))
        }
    }
}
