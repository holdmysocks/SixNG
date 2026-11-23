//! Safe parallel-render and resource-state command encoders.

use super::{
    CommandBuffer, Device, Fence, Origin, Region, RenderCommandEncoder, RenderPassDescriptor,
    ResourceStatePassDescriptor, SparseTextureMappingMode, StoreAction, StoreActionOptions,
    Texture,
};
use crate::Error;

/// A checked index into a pass sample-buffer attachment array.
pub use metal_rust_ffi::PassSampleBufferAttachmentIndex;

impl super::BlitPassSampleBufferAttachmentDescriptorArray {
    /// Returns one typed attachment from a checked slot.
    pub fn attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
    ) -> Result<super::BlitPassSampleBufferAttachmentDescriptor, Error> {
        self.inner
            .attachment(index)
            .map(super::BlitPassSampleBufferAttachmentDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Replaces or clears one typed attachment in a checked slot.
    pub fn set_attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
        attachment: Option<&super::BlitPassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_attachment(index, attachment.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl super::ResourceStatePassSampleBufferAttachmentDescriptorArray {
    /// Returns one typed attachment from a checked slot.
    pub fn attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
    ) -> Result<super::ResourceStatePassSampleBufferAttachmentDescriptor, Error> {
        self.inner
            .attachment(index)
            .map(super::ResourceStatePassSampleBufferAttachmentDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Replaces or clears one typed attachment in a checked slot.
    pub fn set_attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
        attachment: Option<&super::ResourceStatePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_attachment(index, attachment.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl super::CommandEncoder {
    /// Encodes a cross-pass queue-stage barrier using validated stage masks.
    pub fn barrier_after_queue_stages(
        &self,
        after_queue_stages: super::Stages,
        before_stages: super::Stages,
    ) -> Result<(), Error> {
        self.inner
            .barrier_after_queue_stages(after_queue_stages, before_stages)
            .map_err(Error::from_ffi)
    }
}

/// One checked region in a batched sparse-texture mapping operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SparseTextureMapping {
    /// Texture region whose tiles are mapped or unmapped.
    pub region: Region,
    /// Mipmap level containing the region.
    pub mip_level: usize,
    /// Array or cube-face slice containing the region.
    pub slice: usize,
}

impl SparseTextureMapping {
    const fn as_ffi(self) -> metal_rust_ffi::SparseTextureMapping {
        metal_rust_ffi::SparseTextureMapping {
            region: self.region,
            mip_level: self.mip_level,
            slice: self.slice,
        }
    }
}

/// A parallel render encoder that exclusively borrows its command buffer.
pub struct ParallelRenderCommandEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::ParallelRenderCommandEncoder<'a>,
}

impl ParallelRenderCommandEncoder<'_> {
    /// Creates one render encoder whose borrow prevents ending the parallel pass.
    pub fn render_encoder(&mut self) -> Result<RenderCommandEncoder<'_>, Error> {
        self.inner
            .render_encoder()
            .map(|inner| RenderCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insert_debug_signpost(value);
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.push_debug_group(value);
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.pop_debug_group();
    }

    /// Finalizes the store action for a checked color attachment.
    pub fn set_color_store_action(&self, index: usize, action: StoreAction) -> Result<(), Error> {
        self.inner
            .set_color_store_action(index, action)
            .map_err(Error::from_ffi)
    }

    /// Finalizes the depth store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner
            .set_depth_store_action(action)
            .map_err(Error::from_ffi)
    }

    /// Finalizes the stencil store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner
            .set_stencil_store_action(action)
            .map_err(Error::from_ffi)
    }

    /// Sets options for a checked color attachment's store action.
    pub fn set_color_store_action_options(
        &self,
        index: usize,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        self.inner
            .set_color_store_action_options(index, options)
            .map_err(Error::from_ffi)
    }

    /// Sets depth store-action options.
    pub fn set_depth_store_action_options(&self, options: StoreActionOptions) -> Result<(), Error> {
        self.inner
            .set_depth_store_action_options(options)
            .map_err(Error::from_ffi)
    }

    /// Sets stencil store-action options.
    pub fn set_stencil_store_action_options(
        &self,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        self.inner
            .set_stencil_store_action_options(options)
            .map_err(Error::from_ffi)
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(self) {
        self.inner.end_encoding();
    }
}

/// A resource-state encoder that exclusively borrows its command buffer.
pub struct ResourceStateCommandEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::ResourceStateCommandEncoder<'a>,
}

impl ResourceStateCommandEncoder<'_> {
    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insert_debug_signpost(value);
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.push_debug_group(value);
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.pop_debug_group();
    }

    /// Updates one checked sparse-texture mapping.
    pub fn update_texture_mapping(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mapping: SparseTextureMapping,
    ) -> Result<(), Error> {
        self.inner
            .update_texture_mapping(&texture.inner, mode, mapping.as_ffi())
            .map_err(Error::from_ffi)
    }

    /// Updates checked sparse-texture mappings from a Rust slice.
    pub fn update_texture_mappings(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mappings: &[SparseTextureMapping],
    ) -> Result<(), Error> {
        let mappings = mappings
            .iter()
            .copied()
            .map(SparseTextureMapping::as_ffi)
            .collect::<Vec<_>>();
        self.inner
            .update_texture_mappings(&texture.inner, mode, &mappings)
            .map_err(Error::from_ffi)
    }

    /// Safely substitutes native indirect-buffer mappings with checked Rust
    /// mapping records.
    pub fn update_texture_mapping_indirect(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mappings: &[SparseTextureMapping],
    ) -> Result<(), Error> {
        let mappings = mappings
            .iter()
            .copied()
            .map(SparseTextureMapping::as_ffi)
            .collect::<Vec<_>>();
        self.inner
            .update_texture_mapping_indirect(&texture.inner, mode, &mappings)
            .map_err(Error::from_ffi)
    }

    /// Moves checked sparse mappings between compatible sparse textures.
    #[allow(clippy::too_many_arguments)]
    pub fn move_texture_mappings(
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
            .move_texture_mappings(
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

    /// Updates a fence after all resource-state work encoded so far.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .update_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Waits for a fence before subsequent resource-state work.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .wait_for_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(self) {
        self.inner.end_encoding();
    }
}

impl CommandBuffer {
    /// Begins a parallel render encoder with an exclusive command-buffer borrow.
    pub fn parallel_render_encoder<'a>(
        &'a mut self,
        descriptor: &RenderPassDescriptor,
    ) -> Result<ParallelRenderCommandEncoder<'a>, Error> {
        self.inner
            .parallel_render_encoder(&descriptor.inner)
            .map(|inner| ParallelRenderCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a resource-state encoder with an exclusive command-buffer borrow.
    pub fn resource_state_encoder<'a>(
        &'a mut self,
    ) -> Result<ResourceStateCommandEncoder<'a>, Error> {
        self.inner
            .resource_state_encoder()
            .map(|inner| ResourceStateCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a descriptor-configured resource-state encoder.
    pub fn resource_state_encoder_with_descriptor<'a>(
        &'a mut self,
        descriptor: &ResourceStatePassDescriptor,
    ) -> Result<ResourceStateCommandEncoder<'a>, Error> {
        self.inner
            .resource_state_encoder_with_descriptor(&descriptor.inner)
            .map(|inner| ResourceStateCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }
}
