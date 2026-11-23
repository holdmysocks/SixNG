//! Safe Metal buffer resources.

use super::{
    BufferSparseTier, CheckedTensorDescriptor, Device, StorageMode, Tensor, Texture,
    TextureDescriptor,
};
use crate::Error;

/// An owned Metal buffer.
#[derive(Clone)]
pub struct Buffer {
    pub(crate) inner: metal_rust_ffi::Buffer,
}

/// An opaque staging readback bound to the command submission that recorded it.
pub struct BufferReadback {
    pub(crate) inner: metal_rust_ffi::BufferReadback,
}

impl Buffer {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Buffer) -> Self {
        Self { inner }
    }

    /// Returns the byte length of the buffer.
    #[must_use]
    pub fn length(&self) -> usize {
        self.inner.length()
    }

    /// Returns the storage mode selected when the buffer was allocated.
    #[must_use]
    pub const fn storage_mode(&self) -> StorageMode {
        self.inner.storage_mode()
    }

    /// Adds a debug marker for a checked byte range.
    pub fn add_debug_marker(
        &self,
        marker: &str,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .add_debug_marker(marker, range)
            .map_err(Error::from_ffi)
    }

    /// Removes all debug markers.
    pub fn remove_all_debug_markers(&self) {
        self.inner.remove_all_debug_markers();
    }

    /// Returns the GPU virtual address.
    #[must_use]
    pub fn gpu_address(&self) -> u64 {
        self.inner.gpu_address()
    }

    /// Returns the sparse-buffer tier.
    #[must_use]
    pub fn sparse_buffer_tier(&self) -> BufferSparseTier {
        self.inner.sparse_buffer_tier()
    }

    /// Returns the remote-storage buffer, if present.
    #[must_use]
    pub fn remote_storage_buffer(&self) -> Option<Self> {
        self.inner.remote_storage_buffer().map(Self::from_ffi)
    }

    /// Creates a remote view for another device when supported.
    pub fn new_remote_view(&self, device: &Device) -> Result<Self, Error> {
        self.inner
            .new_remote_view(&device.inner)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a texture view over a checked buffer offset and row stride.
    pub fn new_texture(
        &self,
        descriptor: &TextureDescriptor,
        offset: usize,
        bytes_per_row: usize,
    ) -> Result<Texture, Error> {
        self.inner
            .new_texture(&descriptor.inner, offset, bytes_per_row)
            .map(Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a tensor sharing this buffer after checking its complete layout range.
    pub fn new_tensor(
        &self,
        offset: usize,
        descriptor: &CheckedTensorDescriptor,
    ) -> Result<Tensor, Error> {
        self.inner
            .new_tensor(offset, descriptor.as_ffi())
            .map(Tensor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Copies bytes into CPU-visible storage after checking the complete range.
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.inner.write(offset, bytes).map_err(Error::from_ffi)
    }

    /// Copies CPU-visible bytes into owned memory.
    ///
    /// The caller must wait for all GPU accesses to the requested range to
    /// complete before reading.
    pub fn read(&self, offset: usize, length: usize) -> Result<Vec<u8>, Error> {
        self.inner.read(offset, length).map_err(Error::from_ffi)
    }
}
