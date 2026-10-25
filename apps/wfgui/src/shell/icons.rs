use retinue_ui::{image, painter::RasterImage};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Mastery,
    Foundry,
    Inventory,
    Relics,
    Builds,
    Market,
    Settings,
    Left,
    Right,
    Smaller,
    Larger,
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Icon {
    fn source(self) -> &'static [u8] {
        macro_rules! asset {
            ($name:literal) => {
                include_bytes!(concat!("../../resources/ui/", $name, ".png"))
            };
        }
        match self {
            Self::Mastery => asset!("nav_mastery"),
            Self::Foundry => asset!("nav_foundry"),
            Self::Inventory => asset!("nav_inventory"),
            Self::Relics => asset!("nav_relic"),
            Self::Builds => asset!("nav_builds"),
            Self::Market => asset!("market"),
            Self::Settings => asset!("settings"),
            Self::Left => asset!("panel-left"),
            Self::Right => asset!("panel-right"),
            Self::Smaller => asset!("zoomout"),
            Self::Larger => asset!("zoomin"),
            Self::Minimize => asset!("window-minimize"),
            Self::Maximize => asset!("window-maximize"),
            Self::Restore => asset!("window-restore"),
            Self::Close => asset!("window-close"),
        }
    }
}

#[derive(Default)]
pub struct Icons {
    scale: f32,
    images: HashMap<(Icon, bool), Prepared>,
    pub gradient: Option<Prepared>,
}

pub struct Prepared {
    pub image: RasterImage,
    pub width: u32,
    pub height: u32,
}
impl Prepared {
    fn new(pixels: image::RgbaImage) -> Result<Self, String> {
        let (width, height) = pixels.dimensions();
        Ok(Self {
            image: RasterImage::from_rgba(pixels).map_err(|e| e.to_string())?,
            width,
            height,
        })
    }
}

impl Icons {
    pub fn prepare(&mut self, scale: f32) -> Result<(), String> {
        if self.scale == scale {
            return Ok(());
        }
        let mut images = HashMap::new();
        for (icon, size) in [
            (Icon::Mastery, 24.0),
            (Icon::Foundry, 24.0),
            (Icon::Inventory, 24.0),
            (Icon::Relics, 24.0),
            (Icon::Builds, 24.0),
            (Icon::Market, 24.0),
            (Icon::Settings, 24.0),
            (Icon::Left, 16.0),
            (Icon::Right, 16.0),
            (Icon::Smaller, 14.0),
            (Icon::Larger, 14.0),
            (Icon::Minimize, 30.0),
            (Icon::Maximize, 30.0),
            (Icon::Restore, 30.0),
            (Icon::Close, 30.0),
        ] {
            let source = image::load_from_memory(icon.source()).map_err(|e| e.to_string())?;
            let size = (size * scale).round().max(1.0) as u32;
            let mut pixels = source
                .resize(size, size, image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            for pixel in pixels.pixels_mut() {
                pixel.0[..3].copy_from_slice(&[216, 219, 234]);
            }
            images.insert((icon, true), Prepared::new(pixels.clone())?);
            for pixel in pixels.pixels_mut() {
                pixel.0[3] = (u16::from(pixel.0[3]) * 45 / 100) as u8;
            }
            images.insert((icon, false), Prepared::new(pixels)?);
        }
        let width = (428.0 * scale).round() as u32;
        self.gradient = Some(Prepared::new(image::RgbaImage::from_fn(
            width,
            (40.0 * scale).round() as u32,
            |x, y| {
                let dx = x as f32 / scale - 7.0;
                let dy = y as f32 / scale - 3.0;
                let opacity = (1.0 - dx.hypot(dy) / 428.0).clamp(0.0, 1.0) * 0.102;
                let channel = |a: f32, b: f32| (a + (b - a) * opacity).round() as u8;
                image::Rgba([
                    channel(16.0, 53.0),
                    channel(22.0, 86.0),
                    channel(35.0, 179.0),
                    255,
                ])
            },
        ))?);
        self.images = images;
        self.scale = scale;
        Ok(())
    }

    pub fn get(&self, icon: Icon, enabled: bool) -> &Prepared {
        &self.images[&(icon, enabled)]
    }
}
