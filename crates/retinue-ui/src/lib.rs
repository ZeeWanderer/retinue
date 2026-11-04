//! Retained UI authoring for Retinue's desktop clients and in-game overlays.
//!
//! Start with the [`prelude`], [`view::View`] and [`runtime::ViewTree`]. A view's
//! body describes active content; observed reads in that body establish its
//! invalidation dependencies. [`state`] and [`environment`] supply local and
//! inherited state without tying models to a window.
//!
//! Use stacks and ordered modifiers for ordinary composition, [`layout::Layout`]
//! for custom arrangement, and [`drawing`] for typed custom paint. These all feed
//! the same retained runtime and layout-independent `retinue-render` pipeline.
//! Lower-level drawing is supported, not a separate rendering fallback.
//!
//! [`input`] owns hit geometry and pointer capture; [`motion`] and [`timeline`]
//! provide animation and scheduled updates. Desktop and overlay hosts are feature
//! gated. This is an internal SDK: prefer clear contracts, not compatibility
//! wrappers around superseded designs.

extern crate self as retinue_ui;

pub use retinue_ui_macros::ui;
pub use retinue_ui_macros::{ViewProperties, observable};
pub use view::ViewProperties;

#[cfg(feature = "overlay")]
pub mod clipboard;
#[cfg(feature = "desktop")]
pub mod desktop;

pub mod drawing;
pub mod environment;
#[cfg(feature = "overlay")]
pub mod focus;
pub mod focus_visible;
pub mod grid;
pub mod image;
pub mod input;
#[cfg(feature = "overlay")]
pub mod layer;
pub mod layout;
pub mod line_edit;
pub mod motion;
pub mod popover;
pub mod prelude;
pub mod preparation;
#[cfg(any(feature = "desktop", feature = "overlay"))]
pub mod presentation_buffers;
pub mod runtime;
pub mod scene;
pub mod schedule;
pub mod scroll;
pub mod semantics;
#[cfg(feature = "overlay")]
pub mod shortcut;
pub mod state;
#[cfg(feature = "overlay")]
pub mod surface;
pub mod timeline;
pub mod transition;
pub mod view;
#[cfg(feature = "overlay")]
pub mod window_input;
pub use retinue_render::{Bounds, Key, Rect};
pub use taffy;
