//! Audited MetalFX Objective-C boundary.

#[path = "MetalFXMatrix.rs"]
pub(crate) mod matrix;
#[path = "MTLFXSpatialScaler.rs"]
mod mtl_fx_spatial_scaler;
#[path = "MTLFXTemporalScaler.rs"]
mod mtl_fx_temporal_scaler;
#[path = "MTLFXTypes.rs"]
mod mtl_fx_types;

pub use mtl_fx_spatial_scaler::{SpatialScaler, SpatialScalerDescriptor};
pub use mtl_fx_temporal_scaler::{TemporalScaler, TemporalScalerDescriptor};
pub use mtl_fx_types::{Matrix4x4, MatrixValidationError, SpatialScalerColorProcessingMode};
