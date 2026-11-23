//! Checked descriptor-array access for vertex and stage-input configuration.

use crate::Error;

macro_rules! descriptor_array_access {
    ($array:ty, $item:ty) => {
        impl $array {
            /// Returns the descriptor at a checked Metal binding slot.
            pub fn get(&self, index: usize) -> Result<$item, Error> {
                self.inner
                    .get(index)
                    .map(<$item>::from_ffi)
                    .map_err(Error::from_ffi)
            }

            /// Sets or clears the descriptor at a checked Metal binding slot.
            pub fn set(&self, index: usize, value: Option<&$item>) -> Result<(), Error> {
                self.inner
                    .set(index, value.map(|value| &value.inner))
                    .map_err(Error::from_ffi)
            }
        }
    };
}

descriptor_array_access!(
    super::VertexBufferLayoutDescriptorArray,
    super::VertexBufferLayoutDescriptor
);
descriptor_array_access!(
    super::VertexAttributeDescriptorArray,
    super::VertexAttributeDescriptor
);
descriptor_array_access!(
    super::BufferLayoutDescriptorArray,
    super::BufferLayoutDescriptor
);
descriptor_array_access!(super::AttributeDescriptorArray, super::AttributeDescriptor);

impl super::VertexDescriptor {
    /// Creates the framework convenience descriptor after availability checks.
    pub fn default_descriptor() -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::VertexDescriptor::default_descriptor()
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl super::StageInputOutputDescriptor {
    /// Creates the framework convenience descriptor after availability checks.
    pub fn default_descriptor() -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::StageInputOutputDescriptor::default_descriptor()
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}
