//! Rust-native safe extensions for Metal 4 pipeline descriptors.

use crate::Error;

fn extents_to_ffi(
    value: &crate::metal::CheckedTensorExtents,
) -> Result<metal_rust_ffi::CheckedTensorExtents, Error> {
    metal_rust_ffi::CheckedTensorExtents::new(value.as_slice()).map_err(Error::from_ffi)
}

impl super::MeshRenderPipelineDescriptor {
    /// Returns required mesh threadgroup dimensions after availability checks.
    pub fn required_threads_per_mesh_threadgroup_safe(&self) -> Result<crate::metal::Size, Error> {
        self.inner
            .required_threads_per_mesh_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero required mesh threadgroup dimensions.
    pub fn set_required_threads_per_mesh_threadgroup_safe(
        &self,
        value: crate::metal::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_mesh_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }

    /// Returns required object threadgroup dimensions after availability checks.
    pub fn required_threads_per_object_threadgroup_safe(
        &self,
    ) -> Result<crate::metal::Size, Error> {
        self.inner
            .required_threads_per_object_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero required object threadgroup dimensions.
    pub fn set_required_threads_per_object_threadgroup_safe(
        &self,
        value: crate::metal::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_object_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl super::MachineLearningPipelineDescriptor {
    /// Returns optional checked input dimensions for a non-negative buffer index.
    pub fn input_dimensions(
        &self,
        buffer_index: usize,
    ) -> Result<Option<crate::metal::CheckedTensorExtents>, Error> {
        self.inner
            .input_dimensions(buffer_index)
            .map_err(Error::from_ffi)?
            .map(|value| crate::metal::CheckedTensorExtents::new(value.as_slice()))
            .transpose()
    }

    /// Sets or clears checked input dimensions for a non-negative buffer index.
    pub fn set_input_dimensions(
        &self,
        buffer_index: usize,
        dimensions: Option<&crate::metal::CheckedTensorExtents>,
    ) -> Result<(), Error> {
        let dimensions = dimensions.map(extents_to_ffi).transpose()?;
        self.inner
            .set_input_dimensions(buffer_index, dimensions.as_ref())
            .map_err(Error::from_ffi)
    }

    /// Sets a contiguous range of checked input dimensions from a Rust slice.
    pub fn set_input_dimensions_slice(
        &self,
        start_index: usize,
        dimensions: &[crate::metal::CheckedTensorExtents],
    ) -> Result<(), Error> {
        let dimensions = dimensions
            .iter()
            .map(extents_to_ffi)
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .set_input_dimensions_slice(start_index, &dimensions)
            .map_err(Error::from_ffi)
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl super::MachineLearningPipelineReflection {
    /// Returns reflection bindings as an owned Rust vector.
    pub fn bindings_vec(&self) -> Result<Vec<crate::metal::Binding>, Error> {
        self.inner
            .bindings_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(crate::metal::Binding::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }
}
