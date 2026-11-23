//! Safe Core Animation integration for Metal presentation.

#[path = "CAMetalLayer.rs"]
mod ca_metal_layer;

pub use ca_metal_layer::{ColorSpace, Drawable, Layer};
