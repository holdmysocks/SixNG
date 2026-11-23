//! Safe interfaces corresponding to Apple's QuartzCore framework.
//!
//! `CAMetalLayer` and drawable ownership are represented by RAII wrappers.

#[path = "CAMetalLayer.rs"]
mod ca_metal_layer;

pub use ca_metal_layer::{ColorSpace, Drawable, Layer};
/// Canonical Metal-CPP-compatible name for a safe drawable.
pub type MetalDrawable = Drawable;
/// Canonical Metal-CPP-compatible name for a safe Metal layer.
pub type MetalLayer = Layer;
