use crate::{
    Bounds, CacheHint, CachePolicy, Color, Command, ImageFit, ImageSource, OptimizationContext,
    PaintFrame, PaintGroup, PaintInstance, PaintProgram, ProgramStore, Rect, TextAlignment,
    painter::{CachedFont, GlyphCache, RasterImage, load_overlay_font},
};
use serde::Serialize;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Art {
    Solid,
    Decorated,
    Translucent,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Case {
    pub name: &'static str,
    pub size: (u32, u32),
    pub scale: f32,
    pub columns: usize,
    pub rows: usize,
    pub art: Art,
    pub moving: bool,
    pub layers: bool,
    pub frames: usize,
    pub buffers: usize,
    pub peak_bytes: usize,
    pub seed: u64,
}

pub(super) fn cases() -> Vec<Case> {
    let base = Case {
        name: "decorated-scroll",
        size: (960, 600),
        scale: 1.0,
        columns: 6,
        rows: 5,
        art: Art::Decorated,
        moving: true,
        layers: false,
        frames: 18,
        buffers: 3,
        peak_bytes: 32 * 1024 * 1024,
        seed: 0x9e37,
    };
    vec![
        base,
        Case {
            name: "stable-rows",
            layers: true,
            ..base
        },
        Case {
            name: "solid-scroll",
            art: Art::Solid,
            ..base
        },
        Case {
            name: "few-columns",
            columns: 2,
            ..base
        },
        Case {
            name: "larger-pixels",
            scale: 1.2,
            ..base
        },
        Case {
            name: "translucent-overlap",
            art: Art::Translucent,
            ..base
        },
        Case {
            name: "sparse-updates",
            moving: false,
            buffers: 2,
            ..base
        },
        Case {
            name: "short-lived",
            frames: 3,
            ..base
        },
        Case {
            name: "cache-pressure",
            peak_bytes: 24 * 1024,
            ..base
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Step {
    pub offset: i32,
    pub hover: Option<usize>,
    pub image: bool,
    pub reverse: bool,
    pub remove: bool,
}

impl Case {
    pub fn steps(self) -> Vec<Step> {
        let mut seed = self.seed;
        (0..self.frames)
            .map(|n| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let position = n.min(self.frames - 1 - n);
                Step {
                    offset: if self.moving {
                        -(position as i32 * 7)
                    } else {
                        0
                    },
                    hover: (n % 3 != 0).then_some(seed as usize % (self.columns * self.rows)),
                    image: n >= self.frames / 3,
                    reverse: n >= self.frames * 2 / 3,
                    remove: n == self.frames - 2,
                }
            })
            .collect()
    }

    pub fn small(self) -> Self {
        Self {
            size: (320, 240),
            columns: self.columns.min(3),
            rows: 3,
            ..self
        }
    }

    pub fn bounds(self) -> Rect {
        Rect {
            width: self.size.0,
            height: self.size.1,
            ..Rect::default()
        }
    }
}

pub(super) struct Assets {
    font: Rc<CachedFont>,
    image: ImageSource,
}

impl Assets {
    pub fn new() -> Self {
        Self {
            font: Rc::new(load_overlay_font(&GlyphCache::new(1024 * 1024)).unwrap()),
            image: Rc::new(
                RasterImage::render(28, 28, |p| {
                    p.fill_rounded_rect(0, 0, 28, 28, 6, [90, 150, 190, 255]);
                })
                .unwrap(),
            )
            .into(),
        }
    }
}

pub(super) struct Fixture {
    case: Case,
    programs: Vec<[PaintProgram; 4]>,
    stride: (f32, f32),
    pub frame: PaintFrame,
}

impl Fixture {
    pub fn new(case: Case, assets: &Assets) -> Self {
        let side = (case.size.0 as f32 / case.columns as f32 - 14.0).max(20.0);
        let height = (case.size.1 as f32 / 4.0 - 12.0).max(36.0);
        let bounds = Bounds::new(0.0, 0.0, side, height);
        let overlap = if case.art == Art::Translucent {
            0.75
        } else {
            1.0
        };
        let mut store = ProgramStore::default();
        let programs = (0..case.rows * case.columns)
            .map(|n| {
                std::array::from_fn(|state| {
                    let mut commands = vec![
                        Command::Fill {
                            bounds,
                            radius: 0.0,
                            color: Color::rgba(1, 2, 3, 0),
                        },
                        Command::Fill {
                            bounds,
                            radius: if case.art == Art::Solid { 0.0 } else { 4.0 },
                            color: Color::rgba(
                                30 + n as u8 % 20,
                                if state % 2 == 0 { 42 } else { 90 },
                                55,
                                if case.art == Art::Translucent {
                                    180
                                } else {
                                    255
                                },
                            ),
                        },
                    ];
                    if case.art != Art::Solid {
                        commands.push(Command::InsetShadow {
                            bounds: Bounds::new(6.0, 6.0, 34.0, 34.0),
                            radius: 17.0,
                            blur: 4.0,
                            spread: 1.0,
                            color: Color::rgba(65, 190, 60, 200),
                        });
                        for line in 0..3 {
                            commands.push(Command::Text {
                                bounds: Bounds::new(
                                    6.0,
                                    44.0 + line as f32 * 16.0,
                                    side - 12.0,
                                    16.0,
                                ),
                                size: 12.0,
                                text: format!("Item {n}, field {line}"),
                                font: assets.font.clone(),
                                color: Color::rgb(0xdde4ed),
                                align: TextAlignment::Top,
                            });
                        }
                        if state >= 2 {
                            commands.push(Command::Image {
                                bounds: Bounds::new(side - 36.0, 6.0, 28.0, 28.0),
                                image: assets.image.clone(),
                                fit: ImageFit::Contain,
                            });
                        }
                    }
                    store
                        .record(
                            n as u64 + 1,
                            (side, height),
                            case.scale,
                            Rc::new(vec![Command::Translate {
                                offset: (0.0, 0.0),
                                commands: Rc::new(commands),
                            }]),
                        )
                        .unwrap()
                })
            })
            .collect();
        Self {
            case,
            programs,
            stride: ((side + 12.0) * overlap, height + 12.0),
            frame: PaintFrame::default(),
        }
    }

    pub fn update(&mut self, step: Step) {
        let case = self.case;
        let mut indices: Vec<_> = (0..self.programs.len()).collect();
        if step.remove {
            indices.pop();
        }
        if step.reverse {
            indices.reverse();
        }
        let items: Vec<_> = indices
            .into_iter()
            .map(|n| {
                let state = usize::from(step.image) * 2 + usize::from(step.hover == Some(n));
                let origin = (
                    ((6.0 + (n % case.columns) as f32 * self.stride.0) * case.scale) as i32,
                    ((6.0 + (n / case.columns) as f32 * self.stride.1) * case.scale) as i32
                        + step.offset,
                );
                let mut item = PaintInstance::new(
                    (n as u64 + 1).into(),
                    self.programs[n][state].clone(),
                    origin,
                    case.bounds(),
                );
                item.cache = CachePolicy::Prefer;
                item
            })
            .collect();
        let count = items.len();
        let groups = if case.layers {
            (0..count)
                .step_by(case.columns)
                .map(|start| PaintGroup {
                    key: (1000 + start as u64).into(),
                    instances: start..(start + case.columns).min(count),
                    context: OptimizationContext::default().with(CacheHint::Stable),
                })
                .collect()
        } else {
            Vec::new()
        };
        self.frame
            .replace(case.size, case.scale, items, groups)
            .unwrap();
    }
}
