//! Safe interfaces corresponding to Apple's MetalFX framework.
//!
//! Availability-sensitive descriptor and scaler wrappers are exposed here.

#[path = "MTLFXSpatialScaler.rs"]
mod mtl_fx_spatial_scaler;
#[path = "MTLFXTemporalScaler.rs"]
mod mtl_fx_temporal_scaler;

pub use crate::generated_facade::metal_fx::*;
pub use metal_rust_ffi::{Matrix4x4, MatrixValidationError, SpatialScalerColorProcessingMode};
pub use mtl_fx_spatial_scaler::{SpatialScaler, SpatialScalerDescriptor};
pub use mtl_fx_temporal_scaler::{TemporalScaler, TemporalScalerDescriptor};

#[cfg(test)]
mod tests {
    use super::{Matrix4x4, MatrixValidationError};

    #[test]
    fn facade_exposes_typed_matrix_validation_errors() {
        let columns = [[f32::INFINITY; 4]; 4];
        assert_eq!(Matrix4x4::new(columns), Err(MatrixValidationError));
    }
}
