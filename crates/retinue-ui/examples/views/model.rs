mod card;
mod grid_view;
mod root;
mod tab;

use retinue_render::{
    Color, Renderer, ResidencyPolicy,
    damage::Damage,
    painter::{RasterImage, load_overlay_font},
};
use retinue_ui::{
    environment::Environment,
    image::ImageResource,
    ir::{InstanceGraph, Work},
    layout::grid::UniformGrid,
    prelude::*,
    scene::RecordingMode,
};
use std::{rc::Rc, sync::Arc, time::Instant};

pub const BACKGROUND: Color = Color::rgb(0x181c27);
const CARD: Color = Color::rgb(0x212734);
const TEXT: Color = Color::rgb(0xe8ecf2);
const MUTED: Color = Color::rgb(0x919db1);
const GREEN: Color = Color::rgb(0x3da46b);
const ACCENT: Color = Color::rgb(0x8c5edc);
pub const ITEM_COUNT: usize = 5000;

#[derive(Clone, PartialEq)]
struct Model {
    size: State<(f32, f32)>,
    offset: State<f32>,
    counter: State<u32>,
    replacement: State<bool>,
    selected: State<u64>,
    order: KeyedOrder,
    images: [ImageResource; 2],
}

pub struct Demo {
    pub view: InstanceGraph,
    pub work: Work,
    pub renderer: Renderer,
    pub size: (u32, u32),
    pub scale: f32,
    pub scroll: f32,
    pub counter: u32,
    pub replacement: bool,
    pub selected: State<u64>,
    model: Model,
    environment: Environment,
}

fn grid(width: f32) -> UniformGrid {
    UniformGrid::adaptive(
        (width - 40.0).max(1.0),
        260.0,
        108.0,
        LayoutSize::new(10.0, 10.0),
        Insets::default(),
    )
    .unwrap()
}

impl Demo {
    pub fn new(scale: f32) -> Self {
        let font = Rc::new(load_overlay_font().unwrap());
        let selected = State::new(10);
        let image = |color| {
            ImageResource::ready(Arc::new(
                RasterImage::render(72, 72, |painter| {
                    painter.fill_rounded_rect(8, 8, 56, 56, 12, color);
                })
                .unwrap(),
            ))
        };
        let model = Model {
            size: State::new((960.0, 680.0)),
            offset: State::new(0.0),
            counter: State::new(2),
            replacement: State::new(false),
            selected: selected.clone(),
            order: KeyedOrder::new((0..ITEM_COUNT).map(|i| retinue_ui::Key::new("card", i as u64)))
                .unwrap(),
            images: [image([70, 170, 130, 255]), image([205, 145, 70, 255])],
        };
        let environment = Environment::default();
        environment.set::<Typography>(TextStyle::new(font, 14.0, TEXT));
        let view = InstanceGraph::with_root_view(
            root::Root {
                model: model.clone(),
            },
            environment.clone(),
        )
        .unwrap();
        Self {
            view,
            work: Work::default(),
            renderer: Renderer::new(ResidencyPolicy::desktop(16 * 1024 * 1024)),
            size: ((960.0 * scale) as u32, (680.0 * scale) as u32),
            scale,
            scroll: 0.0,
            counter: 2,
            replacement: false,
            selected,
            model,
            environment,
        }
    }

    pub fn update(&mut self, now: Instant, mode: RecordingMode) -> Damage {
        self.model.size.set((
            self.size.0 as f32 / self.scale,
            self.size.1 as f32 / self.scale,
        ));
        self.model.offset.set(self.scroll);
        self.model.counter.set(self.counter);
        self.model.replacement.set(self.replacement);
        if matches!(mode, RecordingMode::Always) {
            self.view = InstanceGraph::with_root_view(
                root::Root {
                    model: self.model.clone(),
                },
                self.environment.clone(),
            )
            .unwrap();
        }
        let update = self
            .view
            .advance_at(
                self.size,
                self.scale,
                FrameTime {
                    instant: now,
                    wall_time: std::time::SystemTime::now(),
                },
                None,
            )
            .unwrap();
        self.work = update.work;
        match update.completion.unwrap() {
            std::task::Poll::Ready(damage) => damage,
            std::task::Poll::Pending => unreachable!("inline layout"),
        }
    }

    pub fn scroll_by(&mut self, distance: f32) {
        let maximum = (grid(self.size.0 as f32 / self.scale)
            .extent(ITEM_COUNT)
            .unwrap()
            .height
            - (self.size.1 as f32 / self.scale - 140.0))
            .max(0.0);
        self.scroll = (self.scroll + distance).clamp(0.0, maximum);
    }
}
