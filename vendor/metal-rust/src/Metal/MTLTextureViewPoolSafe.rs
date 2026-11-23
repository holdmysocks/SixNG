//! Safe lightweight texture-view-pool extensions.

use super::{
    Buffer, ResourceID, Texture, TextureDescriptor, TextureViewDescriptor, TextureViewPool,
};
use crate::Error;

impl TextureViewPool {
    /// Copies a texture's default view into one checked pool slot.
    pub fn set_texture_view(&self, texture: &Texture, index: usize) -> Result<ResourceID, Error> {
        self.inner
            .set_texture_view(&texture.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Creates a validated texture view, then copies it into one pool slot.
    pub fn set_texture_view_with_descriptor(
        &self,
        texture: &Texture,
        descriptor: &TextureViewDescriptor,
        index: usize,
    ) -> Result<ResourceID, Error> {
        self.inner
            .set_texture_view_with_descriptor(&texture.inner, &descriptor.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked buffer-backed texture view, then copies it into a pool slot.
    pub fn set_texture_view_from_buffer(
        &self,
        buffer: &Buffer,
        descriptor: &TextureDescriptor,
        offset: usize,
        bytes_per_row: usize,
        index: usize,
    ) -> Result<ResourceID, Error> {
        self.inner
            .set_texture_view_from_buffer(
                &buffer.inner,
                &descriptor.inner,
                offset,
                bytes_per_row,
                index,
            )
            .map_err(Error::from_ffi)
    }
}
