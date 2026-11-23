//! Checked tensor layouts and descriptor configuration.

use crate::Error;

/// The largest rank accepted by Metal tensor extents.
pub const MAX_TENSOR_RANK: usize = metal_rust_ffi::MAX_TENSOR_RANK;

/// One checked buffer attachment retained by a tensor attachment map.
#[derive(Clone)]
pub struct TensorBufferAttachment {
    buffer: crate::metal::Buffer,
    offset: usize,
    layout: TensorLayout,
}

impl TensorBufferAttachment {
    /// Returns the retained backing buffer.
    #[must_use]
    pub const fn buffer(&self) -> &crate::metal::Buffer {
        &self.buffer
    }

    /// Returns the checked byte offset into the buffer.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the layout whose complete span was checked against the buffer.
    #[must_use]
    pub const fn layout(&self) -> &TensorLayout {
        &self.layout
    }
}

/// A safe tensor buffer-attachment map keyed by declared plane types.
pub struct CheckedTensorBufferAttachments {
    inner: metal_rust_ffi::CheckedTensorBufferAttachments,
}

impl CheckedTensorBufferAttachments {
    /// Creates an empty attachment map after checking framework availability.
    pub fn new() -> Result<Self, Error> {
        metal_rust_ffi::CheckedTensorBufferAttachments::new()
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Associates a buffer range with a declared tensor plane.
    pub fn set_buffer(
        &mut self,
        plane: crate::metal::TensorPlaneType,
        buffer: &crate::metal::Buffer,
        offset: usize,
        layout: &TensorLayout,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer(plane, &buffer.inner, offset, &layout.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns the retained buffer for a plane, if it was configured.
    pub fn buffer(
        &self,
        plane: crate::metal::TensorPlaneType,
    ) -> Result<Option<crate::metal::Buffer>, Error> {
        self.inner
            .buffer(plane)
            .map(|value| value.map(crate::metal::Buffer::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns the checked byte offset for a plane, if it was configured.
    pub fn offset(&self, plane: crate::metal::TensorPlaneType) -> Result<Option<usize>, Error> {
        self.inner.offset(plane).map_err(Error::from_ffi)
    }

    /// Returns all checked attachment information for a plane.
    pub fn attachment(
        &self,
        plane: crate::metal::TensorPlaneType,
    ) -> Result<Option<TensorBufferAttachment>, Error> {
        self.inner
            .attachment(plane)
            .map(|value| {
                value.map(|attachment| TensorBufferAttachment {
                    buffer: crate::metal::Buffer::from_ffi(attachment.buffer().clone()),
                    offset: attachment.offset(),
                    layout: TensorLayout {
                        inner: attachment.layout().clone(),
                    },
                })
            })
            .map_err(Error::from_ffi)
    }

    /// Removes every native and Rust-side attachment.
    pub fn reset(&mut self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }

    pub(crate) const fn as_ffi(&self) -> &metal_rust_ffi::CheckedTensorBufferAttachments {
        &self.inner
    }
}

/// Owned, checked dimension or stride values for a Metal tensor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedTensorExtents {
    pub(crate) inner: metal_rust_ffi::CheckedTensorExtents,
}

impl CheckedTensorExtents {
    /// Copies extent values after checking Metal's maximum rank and integer ABI.
    pub fn new(values: &[usize]) -> Result<Self, Error> {
        metal_rust_ffi::CheckedTensorExtents::new(values)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns the number of dimensions.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.inner.rank()
    }

    /// Returns the extent at a checked dimension index.
    #[must_use]
    pub fn get(&self, dimension: usize) -> Option<usize> {
        self.inner.get(dimension)
    }

    /// Returns all extent values in innermost-to-outermost order.
    #[must_use]
    pub fn as_slice(&self) -> &[usize] {
        self.inner.as_slice()
    }
}

/// A tensor layout whose complete backing byte span has been checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorLayout {
    inner: metal_rust_ffi::TensorLayout,
}

impl TensorLayout {
    /// Builds a dense tensor layout.
    pub fn dense(
        dimensions: &[usize],
        data_type: crate::metal::TensorDataType,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TensorLayout::dense(dimensions, data_type)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Builds a checked non-overlapping layout from explicit element strides.
    pub fn strided(
        dimensions: &[usize],
        strides: &[usize],
        data_type: crate::metal::TensorDataType,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TensorLayout::strided(dimensions, strides, data_type)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Builds a layout with Metal's machine-learning alignment rules.
    pub fn machine_learning(
        dimensions: &[usize],
        strides: &[usize],
        data_type: crate::metal::TensorDataType,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TensorLayout::machine_learning(dimensions, strides, data_type)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns dimensions in innermost-to-outermost order.
    #[must_use]
    pub fn dimensions(&self) -> &[usize] {
        self.inner.dimensions().as_slice()
    }

    /// Returns element strides in innermost-to-outermost order.
    #[must_use]
    pub fn strides(&self) -> &[usize] {
        self.inner.strides().as_slice()
    }

    /// Returns the tensor element type.
    #[must_use]
    pub const fn data_type(&self) -> crate::metal::TensorDataType {
        self.inner.data_type()
    }

    /// Returns the complete backing byte span reachable by this layout.
    #[must_use]
    pub const fn byte_span(&self) -> usize {
        self.inner.byte_span()
    }

    /// Checks the exact range used by a buffer-backed tensor.
    pub fn checked_buffer_range(
        &self,
        buffer_length: usize,
        offset: usize,
        usage: crate::metal::TensorUsage,
    ) -> Result<std::ops::Range<usize>, Error> {
        self.inner
            .checked_buffer_range(buffer_length, offset, usage)
            .map_err(Error::from_ffi)
    }
}

/// A safe Metal tensor descriptor that retains its checked layout proof.
#[derive(Clone)]
pub struct CheckedTensorDescriptor {
    inner: metal_rust_ffi::CheckedTensorDescriptor,
}

impl CheckedTensorDescriptor {
    /// Creates a Metal descriptor from a checked layout and validated options.
    pub fn new(
        layout: TensorLayout,
        usage: crate::metal::TensorUsage,
        resource_options: crate::metal::ResourceOptions,
    ) -> Result<Self, Error> {
        metal_rust_ffi::CheckedTensorDescriptor::new(layout.inner, usage, resource_options)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns dimensions in innermost-to-outermost order.
    #[must_use]
    pub fn dimensions(&self) -> &[usize] {
        self.inner.layout().dimensions().as_slice()
    }

    /// Returns element strides in innermost-to-outermost order.
    #[must_use]
    pub fn strides(&self) -> &[usize] {
        self.inner.layout().strides().as_slice()
    }

    /// Returns the checked complete byte span.
    #[must_use]
    pub const fn byte_span(&self) -> usize {
        self.inner.layout().byte_span()
    }

    /// Returns the validated tensor usage.
    #[must_use]
    pub const fn usage(&self) -> crate::metal::TensorUsage {
        self.inner.usage()
    }

    /// Returns the validated resource allocation options.
    #[must_use]
    pub const fn resource_options(&self) -> crate::metal::ResourceOptions {
        self.inner.resource_options()
    }

    /// Checks the exact backing range using this descriptor's own usage bits.
    pub fn checked_buffer_range(
        &self,
        buffer_length: usize,
        offset: usize,
    ) -> Result<std::ops::Range<usize>, Error> {
        self.inner
            .checked_buffer_range(buffer_length, offset)
            .map_err(Error::from_ffi)
    }

    pub(crate) const fn as_ffi(&self) -> &metal_rust_ffi::CheckedTensorDescriptor {
        &self.inner
    }
}

impl crate::generated_facade::metal::TensorAuxiliaryPlaneDescriptorMap {
    /// Associates a checked scale-plane descriptor with this map.
    pub fn set_descriptor(
        &self,
        plane: crate::metal::TensorPlaneType,
        descriptor: &crate::metal::TensorAuxiliaryPlaneDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .set_descriptor(plane, &descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns the retained descriptor for a declared auxiliary plane.
    pub fn descriptor(
        &self,
        plane: crate::metal::TensorPlaneType,
    ) -> Result<Option<crate::metal::TensorAuxiliaryPlaneDescriptor>, Error> {
        self.inner
            .descriptor(plane)
            .map(|value| value.map(crate::metal::TensorAuxiliaryPlaneDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Removes every auxiliary descriptor from this map.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }
}

impl crate::generated_facade::metal::Tensor {
    /// Returns the opaque GPU resource identifier as an owned Rust value.
    pub fn gpu_resource_id(&self) -> Result<crate::metal::ResourceID, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }

    /// Returns all auxiliary planes as retained safe wrappers.
    pub fn auxiliary_plane_objects(
        &self,
    ) -> Result<Vec<crate::metal::TensorAuxiliaryPlane>, Error> {
        self.inner
            .auxiliary_plane_objects()
            .map(|values| {
                values
                    .into_iter()
                    .map(crate::metal::TensorAuxiliaryPlane::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Copies a checked data-plane slice into an owned byte vector.
    pub fn read_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
    ) -> Result<Vec<u8>, Error> {
        self.inner
            .read_slice(origin, dimensions, &memory_layout.inner)
            .map_err(Error::from_ffi)
    }

    /// Copies a checked plane slice into an owned byte vector.
    pub fn read_plane_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        plane: crate::metal::TensorPlaneType,
    ) -> Result<Vec<u8>, Error> {
        self.inner
            .read_plane_slice(origin, dimensions, &memory_layout.inner, plane)
            .map_err(Error::from_ffi)
    }

    /// Replaces a checked data-plane slice from a Rust byte slice.
    pub fn write_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.inner
            .write_slice(origin, dimensions, &memory_layout.inner, bytes)
            .map_err(Error::from_ffi)
    }

    /// Replaces a checked plane slice from a Rust byte slice.
    pub fn write_plane_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        plane: crate::metal::TensorPlaneType,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.inner
            .write_plane_slice(origin, dimensions, &memory_layout.inner, plane, bytes)
            .map_err(Error::from_ffi)
    }
}
