//! Safe scalar value types for MetalFX.

use std::fmt;

/// Error returned when a MetalFX matrix contains a non-finite component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixValidationError;

impl fmt::Display for MatrixValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MetalFX matrix components must be finite")
    }
}

impl std::error::Error for MatrixValidationError {}

/// A column-major 4×4 floating-point matrix matching `simd_float4x4` layout.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4x4 {
    /// Four column vectors.
    pub columns: [[f32; 4]; 4],
}

impl Matrix4x4 {
    /// Creates a matrix after rejecting non-finite components.
    pub fn new(columns: [[f32; 4]; 4]) -> Result<Self, MatrixValidationError> {
        if columns.iter().flatten().all(|value| value.is_finite()) {
            Ok(Self { columns })
        } else {
            Err(MatrixValidationError)
        }
    }

    /// Returns the identity matrix.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            columns: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub(crate) fn is_finite(self) -> bool {
        self.columns.iter().flatten().all(|value| value.is_finite())
    }
}

impl Default for Matrix4x4 {
    fn default() -> Self {
        Self::identity()
    }
}

/// Color-processing mode used by a MetalFX spatial scaler.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpatialScalerColorProcessingMode(isize);

impl SpatialScalerColorProcessingMode {
    /// Perceptual color processing.
    pub const PERCEPTUAL: Self = Self(0);
    /// Linear color processing.
    pub const LINEAR: Self = Self(1);
    /// High-dynamic-range color processing.
    pub const HDR: Self = Self(2);

    /// Returns the framework representation.
    #[must_use]
    pub const fn as_raw(self) -> isize {
        self.0
    }
}

impl TryFrom<isize> for SpatialScalerColorProcessingMode {
    type Error = ();

    fn try_from(value: isize) -> Result<Self, Self::Error> {
        match value {
            0..=2 => Ok(Self(value)),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Matrix4x4, MatrixValidationError};

    #[test]
    fn matrix_constructor_accepts_finite_components() {
        let columns = Matrix4x4::identity().columns;
        assert_eq!(Matrix4x4::new(columns), Ok(Matrix4x4::identity()));
    }

    #[test]
    fn matrix_constructor_reports_non_finite_components() {
        let mut columns = Matrix4x4::identity().columns;
        columns[2][1] = f32::NAN;
        let error = Matrix4x4::new(columns).expect_err("NaN must be rejected");
        assert_eq!(error, MatrixValidationError);
        assert_eq!(
            error.to_string(),
            "MetalFX matrix components must be finite"
        );
    }
}
