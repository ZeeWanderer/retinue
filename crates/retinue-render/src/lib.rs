//! Layout-independent paint programs, execution planning and raster materialization.
//!
//! [`Command`] describes content in local coordinates. [`ProgramStore`] retains
//! immutable programs; [`PaintFrame`] supplies placement and lifetime. The
//! [`Renderer`] plans optimized execution and [`RenderHost`] coordinates target
//! repair and presentation-facing damage. UI authoring and game policy stay above
//! this crate.
//!
//! Raster reuse is an execution choice, not the source representation. Keep
//! original commands and optimization context available when content, placement
//! or residency changes. [`damage`] preserves current-frame damage separately
//! from the repairs required by an acquired backing buffer.

pub mod damage;
mod geometry;
mod identity;
pub mod painter;
pub mod resources;
pub mod thumbnails;

pub mod relocate;
pub mod surface;

mod analysis;
mod background;
mod command;
mod context;
mod cost;
mod coverage;
mod dependencies;
mod execution;
mod fragments;
pub(crate) mod frame;
mod host;
mod layers;
mod materialization;
mod optimize;
mod paint;
mod program;
mod properties;
mod renderer;
mod replay_index;
mod residency;

#[cfg(test)]
mod quality;

pub use analysis::{Analysis, Effects, ResourceRead};
pub use background::{AnalysisPolicy, AnalysisStats};
pub use context::OptimizationContext;
pub use coverage::{
    CollectionContext, CollectionMotion, CollectionRow, Partition, ScopeId, TileHint,
};
pub use execution::DrawStep;
pub use fontdue;
pub use frame::{
    FrameSnapshot, GroupUpdate, PaintFrame, PaintGroup, PaintInstance, PaintPlacement, PaintUpdate,
    PlacementUpdate,
};
pub use geometry::{Bounds, Rect};
pub use host::{HostPolicy, HostStats, RenderHost, RenderPass};
pub use identity::{Key, PaintKey};
pub use image;
pub use layers::{CacheHint, LayerPolicy};
pub use paint::{
    CachePolicy, Color, Command, ImageFit, ImageSource, PaintStats, Pixels as PaintExtent,
    RasterMode, TextAlignment,
};
pub use program::{PaintProgram, ProgramId, ProgramStore};
pub use properties::{
    Clip, ClipId, PropertyBinding, PropertyChange, PropertyDefinitions, PropertyRevision,
    PropertyStats, TransformId, Translation,
};
pub use renderer::Renderer;
pub use residency::{ResidencyPolicy, ResidencyStats};

impl Bounds {
    pub fn pixels(self, scale: f32) -> PaintExtent {
        PaintExtent {
            x: (self.x * scale).round() as i32,
            y: (self.y * scale).round() as i32,
            width: (self.width * scale).round().max(0.0) as u32,
            height: (self.height * scale).round().max(0.0) as u32,
        }
    }
}
