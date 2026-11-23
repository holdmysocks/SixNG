//! Safe buffer-copy command encoder.

use super::{
    BlitOption, Buffer, BufferReadback, CounterSampleBuffer, Fence, IndirectCommandBuffer, Origin,
    Region, Size, Tensor, TensorLayout, TensorPlaneType, Texture, TextureReadback,
};
use crate::Error;

/// A blit encoder that borrows its command buffer until encoding ends.
pub struct BlitCommandEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::BlitCommandEncoder<'a>,
}

impl BlitCommandEncoder<'_> {
    /// Copies a checked byte range between two buffers.
    pub fn copy_buffer(
        &self,
        source: &Buffer,
        source_offset: usize,
        destination: &Buffer,
        destination_offset: usize,
        size: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_buffer(
                &source.inner,
                source_offset,
                &destination.inner,
                destination_offset,
                size,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies a checked texture region between matching textures.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_region(
        &self,
        source: &Texture,
        source_slice: usize,
        source_level: usize,
        source_region: Region,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: Origin,
    ) -> Result<(), Error> {
        self.inner
            .copy_texture_region(
                &source.inner,
                source_slice,
                source_level,
                source_region,
                &destination.inner,
                destination_slice,
                destination_level,
                destination_origin,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies a checked linear buffer layout into a texture region.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture(
        &self,
        source: &Buffer,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: Size,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: Origin,
    ) -> Result<(), Error> {
        self.inner
            .copy_buffer_to_texture(
                &source.inner,
                source_offset,
                source_bytes_per_row,
                source_bytes_per_image,
                source_size,
                &destination.inner,
                destination_slice,
                destination_level,
                destination_origin,
            )
            .map_err(Error::from_ffi)
    }

    /// Safe substitute for the option-bearing buffer-to-texture overload.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture_with_options(
        &self,
        source: &Buffer,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: Size,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: Origin,
        options: BlitOption,
    ) -> Result<(), Error> {
        self.inner
            .copy_buffer_to_texture_with_options(
                &source.inner,
                source_offset,
                source_bytes_per_row,
                source_bytes_per_image,
                source_size,
                &destination.inner,
                destination_slice,
                destination_level,
                destination_origin,
                options,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies a checked texture region into a linear buffer layout.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_buffer(
        &self,
        source: &Texture,
        source_slice: usize,
        source_level: usize,
        source_region: Region,
        destination: &Buffer,
        destination_offset: usize,
        destination_bytes_per_row: usize,
        destination_bytes_per_image: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_texture_to_buffer(
                &source.inner,
                source_slice,
                source_level,
                source_region,
                &destination.inner,
                destination_offset,
                destination_bytes_per_row,
                destination_bytes_per_image,
            )
            .map_err(Error::from_ffi)
    }

    /// Safe substitute for the option-bearing texture-to-buffer overload.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_to_buffer_with_options(
        &self,
        source: &Texture,
        source_slice: usize,
        source_level: usize,
        source_region: Region,
        destination: &Buffer,
        destination_offset: usize,
        destination_bytes_per_row: usize,
        destination_bytes_per_image: usize,
        options: BlitOption,
    ) -> Result<(), Error> {
        self.inner
            .copy_texture_to_buffer_with_options(
                &source.inner,
                source_slice,
                source_level,
                source_region,
                &destination.inner,
                destination_offset,
                destination_bytes_per_row,
                destination_bytes_per_image,
                options,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies a checked tensor slice, including safe reshape copies.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_tensor(
        &self,
        source: &Tensor,
        source_layout: &TensorLayout,
        source_origin: &[usize],
        source_dimensions: &[usize],
        destination: &Tensor,
        destination_layout: &TensorLayout,
        destination_origin: &[usize],
        destination_dimensions: &[usize],
    ) -> Result<(), Error> {
        let source_layout = ffi_tensor_layout(source_layout)?;
        let destination_layout = ffi_tensor_layout(destination_layout)?;
        self.inner
            .copy_tensor(
                &source.inner,
                &source_layout,
                source_origin,
                source_dimensions,
                &destination.inner,
                &destination_layout,
                destination_origin,
                destination_dimensions,
            )
            .map_err(Error::from_ffi)
    }

    /// Safe data-plane substitute for the plane-selecting tensor overload.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_tensor_plane(
        &self,
        source: &Tensor,
        source_layout: &TensorLayout,
        source_origin: &[usize],
        source_dimensions: &[usize],
        source_plane: TensorPlaneType,
        destination: &Tensor,
        destination_layout: &TensorLayout,
        destination_origin: &[usize],
        destination_dimensions: &[usize],
        destination_plane: TensorPlaneType,
    ) -> Result<(), Error> {
        let source_layout = ffi_tensor_layout(source_layout)?;
        let destination_layout = ffi_tensor_layout(destination_layout)?;
        self.inner
            .copy_tensor_plane(
                &source.inner,
                &source_layout,
                source_origin,
                source_dimensions,
                source_plane,
                &destination.inner,
                &destination_layout,
                destination_origin,
                destination_dimensions,
                destination_plane,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies checked whole mip levels across a checked slice range.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_texture_subresources(
        &self,
        source: &Texture,
        source_slice: usize,
        source_level: usize,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        slice_count: usize,
        level_count: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_texture_subresources(
                &source.inner,
                source_slice,
                source_level,
                &destination.inner,
                destination_slice,
                destination_level,
                slice_count,
                level_count,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies a checked range into an internal staging buffer tied to this
    /// command submission.
    pub fn read_buffer(
        &self,
        source: &Buffer,
        source_offset: usize,
        size: usize,
    ) -> Result<BufferReadback, Error> {
        self.inner
            .read_buffer(&source.inner, source_offset, size)
            .map(|inner| BufferReadback { inner })
            .map_err(Error::from_ffi)
    }

    /// Copies a checked level-zero 2D texture region into submission-bound staging.
    pub fn read_texture(&self, source: &Texture, region: Region) -> Result<TextureReadback, Error> {
        self.inner
            .read_texture(&source.inner, region)
            .map(|inner| TextureReadback { inner })
            .map_err(Error::from_ffi)
    }

    /// Fills a checked buffer range with a byte value.
    pub fn fill_buffer(
        &self,
        buffer: &Buffer,
        range: std::ops::Range<usize>,
        value: u8,
    ) -> Result<(), Error> {
        self.inner
            .fill_buffer(&buffer.inner, range, value)
            .map_err(Error::from_ffi)
    }

    /// Copies an entire compatible texture.
    pub fn copy_texture(&self, source: &Texture, destination: &Texture) -> Result<(), Error> {
        self.inner
            .copy_texture(&source.inner, &destination.inner)
            .map_err(Error::from_ffi)
    }

    /// Generates mipmaps for a texture.
    pub fn generate_mipmaps(&self, texture: &Texture) -> Result<(), Error> {
        self.inner
            .generate_mipmaps(&texture.inner)
            .map_err(Error::from_ffi)
    }

    /// Synchronizes a checked texture slice and mip level for CPU access.
    pub fn synchronize_texture(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        self.inner
            .synchronize_texture(&texture.inner, slice, level)
            .map_err(Error::from_ffi)
    }

    /// Synchronizes a managed buffer for subsequent CPU access.
    pub fn synchronize_buffer(&self, buffer: &Buffer) -> Result<(), Error> {
        self.inner
            .synchronize_buffer(&buffer.inner)
            .map_err(Error::from_ffi)
    }

    /// Synchronizes an entire managed texture through resource semantics.
    pub fn synchronize_texture_resource(&self, texture: &Texture) -> Result<(), Error> {
        self.inner
            .synchronize_texture_resource(&texture.inner)
            .map_err(Error::from_ffi)
    }

    /// Optimizes one checked texture slice and mip level for CPU access.
    pub fn optimize_texture_slice_for_cpu(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        self.inner
            .optimize_texture_slice_for_cpu(&texture.inner, slice, level)
            .map_err(Error::from_ffi)
    }

    /// Optimizes one checked texture slice and mip level for GPU access.
    pub fn optimize_texture_slice_for_gpu(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        self.inner
            .optimize_texture_slice_for_gpu(&texture.inner, slice, level)
            .map_err(Error::from_ffi)
    }

    /// Resets a checked range in an indirect command buffer.
    pub fn reset_indirect_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .reset_indirect_commands(&buffer.inner, range)
            .map_err(Error::from_ffi)
    }

    /// Copies a checked indirect-command range.
    pub fn copy_indirect_commands(
        &self,
        source: &IndirectCommandBuffer,
        source_range: std::ops::Range<usize>,
        destination: &IndirectCommandBuffer,
        destination_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_indirect_commands(
                &source.inner,
                source_range,
                &destination.inner,
                destination_index,
            )
            .map_err(Error::from_ffi)
    }

    /// Optimizes a checked indirect-command range.
    pub fn optimize_indirect_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .optimize_indirect_commands(&buffer.inner, range)
            .map_err(Error::from_ffi)
    }

    /// Samples counters at a checked index.
    pub fn sample_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        self.inner
            .sample_counters(&sample_buffer.inner, sample_index, barrier)
            .map_err(Error::from_ffi)
    }

    /// Resolves checked counters into staging bound to this submission.
    pub fn read_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<BufferReadback, Error> {
        self.inner
            .read_counters(&sample_buffer.inner, range)
            .map(|inner| BufferReadback { inner })
            .map_err(Error::from_ffi)
    }

    /// Resets sparse texture access counters for a checked region.
    pub fn reset_texture_access_counters(
        &self,
        texture: &Texture,
        region: Region,
        level: usize,
        slice: usize,
    ) -> Result<(), Error> {
        self.inner
            .reset_texture_access_counters(&texture.inner, region, level, slice)
            .map_err(Error::from_ffi)
    }

    /// Copies sparse-tile access counters into submission-bound staging.
    pub fn read_texture_access_counters(
        &self,
        texture: &Texture,
        region: Region,
        level: usize,
        slice: usize,
        reset_counters: bool,
    ) -> Result<BufferReadback, Error> {
        self.inner
            .read_texture_access_counters(&texture.inner, region, level, slice, reset_counters)
            .map(|inner| BufferReadback { inner })
            .map_err(Error::from_ffi)
    }

    /// Updates a fence after prior blit commands.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .update_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Waits for a fence before subsequent blit commands.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .wait_for_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Optimizes texture contents for CPU access when supported.
    pub fn optimize_texture_for_cpu(&self, texture: &Texture) -> Result<(), Error> {
        self.inner
            .optimize_texture_for_cpu(&texture.inner)
            .map_err(Error::from_ffi)
    }

    /// Optimizes texture contents for GPU access when supported.
    pub fn optimize_texture_for_gpu(&self, texture: &Texture) -> Result<(), Error> {
        self.inner
            .optimize_texture_for_gpu(&texture.inner)
            .map_err(Error::from_ffi)
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(self) {
        self.inner.end_encoding();
    }
}

fn ffi_tensor_layout(layout: &TensorLayout) -> Result<metal_rust_ffi::TensorLayout, Error> {
    metal_rust_ffi::TensorLayout::strided(layout.dimensions(), layout.strides(), layout.data_type())
        .map_err(Error::from_ffi)
}
