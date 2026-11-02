use super::*;
use crate::presentation::widgets::LoadingPlaceholder;

#[derive(PartialEq)]
pub(super) struct LoadingView {
    pub width: f32,
    pub height: f32,
    pub font: TextStyle,
}

impl View for LoadingView {
    fn body(&self, _: &mut ViewContext<'_>) -> impl IntoElement {
        let (w, h) = (self.width, self.height);
        let skeleton = |radius| LoadingPlaceholder {
            radius,
            color: Color::from_bgra([MUTED[0], MUTED[1], MUTED[2], 64]),
        };
        ui! {
            ZStack() {
                place(skeleton(12.0), Bounds::new(45.0, 44.0, 152.0, 24.0))
                place(skeleton(12.0), Bounds::new(w - 157.0, 44.0, 141.0, 24.0))
                for i in 0..4 {
                    place(ui! {
                        ZStack() {
                            fill(rgb(0x101623), 24.5)
                            place(skeleton(24.5), Bounds::new(0.0, 0.0, 49.0, 49.0))
                            place(skeleton(3.0), Bounds::new(62.0, 9.0, ((w - 22.0) / 2.0 - 78.0).max(0.0), 10.0))
                            place(skeleton(3.0), Bounds::new(74.0, 30.0, ((w - 22.0) / 2.0 - 102.0).max(0.0), 10.0))
                        }.alignment(Alignment::Start)
                    }, Bounds::new(7.0 + (i % 2) as f32 * (w - 14.0) / 2.0, 77.0 + (i / 2) as f32 * 56.0, (w - 22.0) / 2.0, 49.0))
                }
                place(fill(controls::CARD, 8.0), Bounds::new(5.0, 190.0, (w - 10.0).max(0.0), 28.0))
                place(label("Void fissures", self.font.font.clone(), 17.0, TEXT, false), Bounds::new(11.0, 190.0, 140.0, 28.0))
                place(ui! {
                    ZStack() {
                        for i in 0..4 {
                            place(ui! {
                                ZStack() {
                                    fill(controls::CARD, 6.0)
                                    place(skeleton(16.0), Bounds::new(16.0, 36.0, 32.0, 32.0))
                                    for j in 0..4 {
                                        place(skeleton(3.0), Bounds::new(63.0, 10.0 + j as f32 * 24.0, (w - 102.0).max(0.0), 10.0))
                                    }
                                }.alignment(Alignment::Start)
                            }, Bounds::new(0.0, i as f32 * 113.0, (w - 14.0).max(0.0), 107.0))
                        }
                    }.alignment(Alignment::Start).clipped()
                }, Bounds::new(7.0, 224.0, (w - 14.0).max(0.0), (h - 224.0).max(0.0)))
            }.alignment(Alignment::Start).frame_aligned(w, h, AnchorPoint::TOP_LEADING)
        }
    }
}
