//! Audited parallel-render and resource-state command encoders.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    BlitPassSampleBufferAttachmentDescriptor, BlitPassSampleBufferAttachmentDescriptorArray,
    CommandEncoder, Fence, ResourceStatePassDescriptor,
    ResourceStatePassSampleBufferAttachmentDescriptor,
    ResourceStatePassSampleBufferAttachmentDescriptorArray,
};
use crate::metal::generated_value_types::{
    SparseTextureMappingMode, Stages, StoreAction, StoreActionOptions,
};
use crate::metal::{
    CommandBuffer, Device, Origin, Region, RenderCommandEncoder, RenderPassDescriptor, Texture,
    TextureType,
};
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLOrigin, MTLParallelRenderCommandEncoder, MTLRegion,
    MTLResourceStateCommandEncoder, MTLSparseTextureMappingMode, MTLStoreAction,
    MTLStoreActionOptions, MTLTexture,
};
use std::cell::Cell;
use std::ptr::NonNull;

const MAX_COLOR_ATTACHMENTS: usize = 8;
const MAX_PASS_SAMPLE_BUFFER_ATTACHMENTS: usize = 4;

fn object_responds(object: &objc2::runtime::AnyObject, selector: objc2::runtime::Sel) -> bool {
    // SAFETY: every Objective-C object implements respondsToSelector:, and the
    // selector and bool use their runtime-declared ABI representations.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

/// A checked index into a pass sample-buffer attachment array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassSampleBufferAttachmentIndex(u8);

impl PassSampleBufferAttachmentIndex {
    /// First pass sample-buffer attachment slot.
    pub const ZERO: Self = Self(0);
    /// Second pass sample-buffer attachment slot.
    pub const ONE: Self = Self(1);
    /// Third pass sample-buffer attachment slot.
    pub const TWO: Self = Self(2);
    /// Fourth pass sample-buffer attachment slot.
    pub const THREE: Self = Self(3);

    const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<usize> for PassSampleBufferAttachmentIndex {
    type Error = Error;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        if value >= MAX_PASS_SAMPLE_BUFFER_ATTACHMENTS {
            return Err(Error::invalid_argument(
                "pass sample-buffer attachment index must be below 4",
            ));
        }
        Ok(Self(value as u8))
    }
}

impl BlitPassSampleBufferAttachmentDescriptorArray {
    /// Returns one typed attachment from a checked slot.
    pub fn attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
    ) -> Result<BlitPassSampleBufferAttachmentDescriptor, Error> {
        if !object_responds(self.as_inner(), sel!(objectAtIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTLBlitPassSampleBufferAttachmentDescriptorArray::object is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the newtype limits the
        // index to Metal's four pass sample-buffer attachment slots.
        let value: Option<Retained<objc2::runtime::AnyObject>> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index.as_usize()] };
        value
            .map(BlitPassSampleBufferAttachmentDescriptor::from_inner)
            .ok_or_else(|| Error::unsupported("Metal returned no blit-pass attachment"))
    }

    /// Replaces or clears one typed attachment in a checked slot.
    pub fn set_attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
        attachment: Option<&BlitPassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        if !object_responds(self.as_inner(), sel!(setObject:atIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTLBlitPassSampleBufferAttachmentDescriptorArray::setObject is unavailable",
            ));
        }
        // SAFETY: selector availability was checked, the index is constrained
        // to the four valid slots, and the typed wrapper owns the attachment.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                setObject: attachment.map(BlitPassSampleBufferAttachmentDescriptor::as_inner),
                atIndexedSubscript: index.as_usize()
            ];
        }
        Ok(())
    }
}

impl ResourceStatePassSampleBufferAttachmentDescriptorArray {
    /// Returns one typed attachment from a checked slot.
    pub fn attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
    ) -> Result<ResourceStatePassSampleBufferAttachmentDescriptor, Error> {
        if !object_responds(self.as_inner(), sel!(objectAtIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTLResourceStatePassSampleBufferAttachmentDescriptorArray::object is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the newtype limits the
        // index to Metal's four pass sample-buffer attachment slots.
        let value: Option<Retained<objc2::runtime::AnyObject>> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index.as_usize()] };
        value
            .map(ResourceStatePassSampleBufferAttachmentDescriptor::from_inner)
            .ok_or_else(|| Error::unsupported("Metal returned no resource-state attachment"))
    }

    /// Replaces or clears one typed attachment in a checked slot.
    pub fn set_attachment(
        &self,
        index: PassSampleBufferAttachmentIndex,
        attachment: Option<&ResourceStatePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        if !object_responds(self.as_inner(), sel!(setObject:atIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTLResourceStatePassSampleBufferAttachmentDescriptorArray::setObject is unavailable",
            ));
        }
        // SAFETY: selector availability was checked, the index is constrained
        // to the four valid slots, and the typed wrapper owns the attachment.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                setObject: attachment.map(ResourceStatePassSampleBufferAttachmentDescriptor::as_inner),
                atIndexedSubscript: index.as_usize()
            ];
        }
        Ok(())
    }
}

impl CommandEncoder {
    /// Encodes a cross-pass queue-stage barrier using validated stage masks.
    pub fn barrier_after_queue_stages(
        &self,
        after_queue_stages: Stages,
        before_stages: Stages,
    ) -> Result<(), Error> {
        if !after_queue_stages.is_valid() || !before_stages.is_valid() {
            return Err(Error::invalid_argument(
                "queue-stage barrier masks contain undeclared bits",
            ));
        }
        if !object_responds(self.as_inner(), sel!(barrierAfterQueueStages:beforeStages:)) {
            return Err(Error::unsupported(
                "MTLCommandEncoder::barrierAfterQueueStages is unavailable",
            ));
        }
        // SAFETY: selector availability and both option masks were checked;
        // Stages has Metal's NSUInteger representation.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                barrierAfterQueueStages: after_queue_stages.as_raw(),
                beforeStages: before_stages.as_raw()
            ];
        }
        Ok(())
    }
}

fn validate_store_action(value: StoreAction) -> Result<(), Error> {
    if !value.is_valid() || value.as_raw() == StoreAction::StoreActionUnknown.as_raw() {
        return Err(Error::invalid_argument(
            "parallel render store action must be a declared non-unknown value",
        ));
    }
    Ok(())
}

fn validate_store_action_options(value: StoreActionOptions) -> Result<(), Error> {
    if !value.is_valid() {
        return Err(Error::invalid_argument(
            "parallel render store-action options contain undeclared bits",
        ));
    }
    Ok(())
}

fn texture_slice_count(texture: &Texture) -> Result<usize, Error> {
    let (_, array_length, _, _) = texture.layout();
    let count = match texture.texture_type()? {
        TextureType::Cube => 6,
        TextureType::CubeArray => array_length
            .checked_mul(6)
            .ok_or_else(|| Error::invalid_argument("cube-array slice count overflows"))?,
        TextureType::D1Array | TextureType::D2Array | TextureType::D2MultisampleArray => {
            array_length
        }
        _ => 1,
    };
    Ok(count.max(1))
}

fn validate_sparse_region(
    texture: &Texture,
    region: Region,
    mip_level: usize,
    slice: usize,
) -> Result<(), Error> {
    if !texture.is_sparse() {
        return Err(Error::invalid_argument(
            "sparse mapping operations require a sparse texture",
        ));
    }
    let (_, _, mip_count, _) = texture.layout();
    if mip_level >= mip_count || slice >= texture_slice_count(texture)? {
        return Err(Error::invalid_argument(
            "sparse texture mip level or slice is out of bounds",
        ));
    }
    if region.size.width == 0 || region.size.height == 0 || region.size.depth == 0 {
        return Err(Error::invalid_argument(
            "sparse texture mapping region must be non-empty",
        ));
    }
    let width = (texture.width() >> mip_level.min(usize::BITS as usize - 1)).max(1);
    let height = (texture.height() >> mip_level.min(usize::BITS as usize - 1)).max(1);
    let depth = (texture.inner.depth() >> mip_level.min(usize::BITS as usize - 1)).max(1);
    let end_x = region.origin.x.checked_add(region.size.width);
    let end_y = region.origin.y.checked_add(region.size.height);
    let end_z = region.origin.z.checked_add(region.size.depth);
    if end_x.is_none_or(|end| end > width)
        || end_y.is_none_or(|end| end > height)
        || end_z.is_none_or(|end| end > depth)
    {
        return Err(Error::invalid_argument(
            "sparse texture mapping region is out of bounds",
        ));
    }
    Ok(())
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

/// A parallel render encoder borrowing its command buffer until encoding ends.
pub struct ParallelRenderCommandEncoder<'a> {
    inner: Retained<ProtocolObject<dyn MTLParallelRenderCommandEncoder>>,
    command_buffer: &'a mut CommandBuffer,
    ended: bool,
    _thread_bound: ThreadBound,
}

impl<'a> ParallelRenderCommandEncoder<'a> {
    fn new(
        inner: Retained<ProtocolObject<dyn MTLParallelRenderCommandEncoder>>,
        command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            inner,
            command_buffer,
            ended: false,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Creates one render encoder whose borrow prevents ending the parallel pass.
    pub fn render_encoder<'encoder>(
        &'encoder mut self,
    ) -> Result<RenderCommandEncoder<'encoder>, Error> {
        self.inner
            .renderCommandEncoder()
            .map(|inner| RenderCommandEncoder::new(inner, &mut *self.command_buffer))
            .ok_or_else(|| Error::unsupported("Metal could not create a parallel render encoder"))
    }

    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insertDebugSignpost(&NSString::from_str(value));
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.pushDebugGroup(&NSString::from_str(value));
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.popDebugGroup();
    }

    /// Finalizes the store action for a checked color attachment.
    pub fn set_color_store_action(&self, index: usize, action: StoreAction) -> Result<(), Error> {
        if index >= MAX_COLOR_ATTACHMENTS {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        validate_store_action(action)?;
        if !self
            .inner
            .respondsToSelector(sel!(setColorStoreAction:atIndex:))
        {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setColorStoreAction is unavailable",
            ));
        }
        // SAFETY: the selector was checked, the attachment index is below
        // Metal's fixed color-attachment limit, and the action is declared.
        unsafe {
            self.inner
                .setColorStoreAction_atIndex(MTLStoreAction(action.as_raw()), index);
        }
        Ok(())
    }

    /// Finalizes the depth store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        validate_store_action(action)?;
        if !self.inner.respondsToSelector(sel!(setDepthStoreAction:)) {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setDepthStoreAction is unavailable",
            ));
        }
        self.inner
            .setDepthStoreAction(MTLStoreAction(action.as_raw()));
        Ok(())
    }

    /// Finalizes the stencil store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        validate_store_action(action)?;
        if !self.inner.respondsToSelector(sel!(setStencilStoreAction:)) {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setStencilStoreAction is unavailable",
            ));
        }
        self.inner
            .setStencilStoreAction(MTLStoreAction(action.as_raw()));
        Ok(())
    }

    /// Sets options for a checked color attachment's store action.
    pub fn set_color_store_action_options(
        &self,
        index: usize,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        if index >= MAX_COLOR_ATTACHMENTS {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        validate_store_action_options(options)?;
        if !self
            .inner
            .respondsToSelector(sel!(setColorStoreActionOptions:atIndex:))
        {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setColorStoreActionOptions is unavailable",
            ));
        }
        // SAFETY: the selector was checked, the attachment index is below
        // Metal's fixed limit, and only declared option bits are passed.
        unsafe {
            self.inner
                .setColorStoreActionOptions_atIndex(MTLStoreActionOptions(options.as_raw()), index);
        }
        Ok(())
    }

    /// Sets depth store-action options.
    pub fn set_depth_store_action_options(&self, options: StoreActionOptions) -> Result<(), Error> {
        validate_store_action_options(options)?;
        if !self
            .inner
            .respondsToSelector(sel!(setDepthStoreActionOptions:))
        {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setDepthStoreActionOptions is unavailable",
            ));
        }
        self.inner
            .setDepthStoreActionOptions(MTLStoreActionOptions(options.as_raw()));
        Ok(())
    }

    /// Sets stencil store-action options.
    pub fn set_stencil_store_action_options(
        &self,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        validate_store_action_options(options)?;
        if !self
            .inner
            .respondsToSelector(sel!(setStencilStoreActionOptions:))
        {
            return Err(Error::unsupported(
                "MTLParallelRenderCommandEncoder::setStencilStoreActionOptions is unavailable",
            ));
        }
        self.inner
            .setStencilStoreActionOptions(MTLStoreActionOptions(options.as_raw()));
        Ok(())
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(mut self) {
        if !self.ended {
            self.inner.endEncoding();
            self.ended = true;
        }
    }
}

impl Drop for ParallelRenderCommandEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.inner.endEncoding();
            self.ended = true;
        }
    }
}

/// A resource-state encoder borrowing its command buffer until encoding ends.
pub struct ResourceStateCommandEncoder<'a> {
    inner: Retained<ProtocolObject<dyn MTLResourceStateCommandEncoder>>,
    _command_buffer: &'a mut CommandBuffer,
    fence_updated: Cell<bool>,
    ended: bool,
    _thread_bound: ThreadBound,
}

impl<'a> ResourceStateCommandEncoder<'a> {
    fn new(
        inner: Retained<ProtocolObject<dyn MTLResourceStateCommandEncoder>>,
        command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            inner,
            _command_buffer: command_buffer,
            fence_updated: Cell::new(false),
            ended: false,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insertDebugSignpost(&NSString::from_str(value));
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.pushDebugGroup(&NSString::from_str(value));
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.popDebugGroup();
    }

    /// Updates one checked sparse-texture mapping.
    pub fn update_texture_mapping(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mapping: SparseTextureMapping,
    ) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument(
                "sparse texture mapping mode is not declared by Metal",
            ));
        }
        validate_sparse_region(texture, mapping.region, mapping.mip_level, mapping.slice)?;
        if !self
            .inner
            .respondsToSelector(sel!(updateTextureMapping:mode:region:mipLevel:slice:))
        {
            return Err(Error::unsupported(
                "MTLResourceStateCommandEncoder::updateTextureMapping is unavailable",
            ));
        }
        // SAFETY: selector availability and all texture coordinates were
        // checked; the safe wrappers keep the texture and encoder retained.
        unsafe {
            self.inner.updateTextureMapping_mode_region_mipLevel_slice(
                &texture.inner,
                MTLSparseTextureMappingMode(mode.as_raw()),
                MTLRegion {
                    origin: mapping.region.origin.into(),
                    size: mapping.region.size.into(),
                },
                mapping.mip_level,
                mapping.slice,
            );
        }
        Ok(())
    }

    /// Updates checked sparse-texture mappings from a Rust slice.
    pub fn update_texture_mappings(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mappings: &[SparseTextureMapping],
    ) -> Result<(), Error> {
        if mappings.is_empty() {
            return Ok(());
        }
        if !mode.is_valid() {
            return Err(Error::invalid_argument(
                "sparse texture mapping mode is not declared by Metal",
            ));
        }
        for mapping in mappings {
            validate_sparse_region(texture, mapping.region, mapping.mip_level, mapping.slice)?;
        }
        if !self.inner.respondsToSelector(
            sel!(updateTextureMappings:mode:regions:mipLevels:slices:numRegions:),
        ) {
            return Err(Error::unsupported(
                "MTLResourceStateCommandEncoder::updateTextureMappings is unavailable",
            ));
        }
        let mut regions = mappings
            .iter()
            .map(|mapping| MTLRegion {
                origin: mapping.region.origin.into(),
                size: mapping.region.size.into(),
            })
            .collect::<Vec<_>>();
        let mut mip_levels = mappings
            .iter()
            .map(|mapping| mapping.mip_level)
            .collect::<Vec<_>>();
        let mut slices = mappings
            .iter()
            .map(|mapping| mapping.slice)
            .collect::<Vec<_>>();
        // SAFETY: all three non-empty vectors have exactly `mappings.len()`
        // initialized elements and remain alive throughout the synchronous
        // Objective-C call. Every coordinate was validated above.
        unsafe {
            self.inner
                .updateTextureMappings_mode_regions_mipLevels_slices_numRegions(
                    &texture.inner,
                    MTLSparseTextureMappingMode(mode.as_raw()),
                    NonNull::new_unchecked(regions.as_mut_ptr()),
                    NonNull::new_unchecked(mip_levels.as_mut_ptr()),
                    NonNull::new_unchecked(slices.as_mut_ptr()),
                    mappings.len(),
                );
        }
        Ok(())
    }

    /// Safely substitutes native indirect-buffer mappings with checked direct
    /// mapping records.
    ///
    /// An arbitrary GPU buffer can contain an unbounded mapping count that a
    /// safe caller cannot validate at encoding time. This substitute preserves
    /// the mapping result without exposing that native unsafe precondition.
    pub fn update_texture_mapping_indirect(
        &self,
        texture: &Texture,
        mode: SparseTextureMappingMode,
        mappings: &[SparseTextureMapping],
    ) -> Result<(), Error> {
        self.update_texture_mappings(texture, mode, mappings)
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
        validate_sparse_region(source, source_region, source_level, source_slice)?;
        let destination_region = Region::new(destination_origin, source_region.size);
        validate_sparse_region(
            destination,
            destination_region,
            destination_level,
            destination_slice,
        )?;
        if source.texture_type()? != destination.texture_type()?
            || source.layout().3 != destination.layout().3
            || source.usage() != destination.usage()
        {
            return Err(Error::invalid_argument(
                "sparse mapping move requires compatible texture types, samples, and usage",
            ));
        }
        if !self.inner.respondsToSelector(sel!(moveTextureMappingsFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:)) {
            return Err(Error::unsupported(
                "MTLResourceStateCommandEncoder::moveTextureMappings is unavailable",
            ));
        }
        // SAFETY: selector availability and both source and destination
        // coordinate ranges were checked; all resource wrappers remain alive.
        unsafe {
            self.inner.moveTextureMappingsFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &source.inner,
                source_slice,
                source_level,
                MTLOrigin::from(source_region.origin),
                source_region.size.into(),
                &destination.inner,
                destination_slice,
                destination_level,
                MTLOrigin::from(destination_origin),
            );
        }
        Ok(())
    }

    /// Updates a fence after all resource-state work encoded so far.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(updateFence:)) {
            return Err(Error::unsupported(
                "MTLResourceStateCommandEncoder::updateFence is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the generated Fence
        // wrapper owns an object declared to conform to MTLFence.
        unsafe {
            let _: () = msg_send![&*self.inner, updateFence: fence.as_inner()];
        }
        self.fence_updated.set(true);
        Ok(())
    }

    /// Waits for a fence before subsequent resource-state work.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        if self.fence_updated.get() {
            return Err(Error::invalid_argument(
                "a resource-state encoder cannot wait after updating a fence",
            ));
        }
        if !self.inner.respondsToSelector(sel!(waitForFence:)) {
            return Err(Error::unsupported(
                "MTLResourceStateCommandEncoder::waitForFence is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the generated Fence
        // wrapper owns an object declared to conform to MTLFence.
        unsafe {
            let _: () = msg_send![&*self.inner, waitForFence: fence.as_inner()];
        }
        Ok(())
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(mut self) {
        if !self.ended {
            self.inner.endEncoding();
            self.ended = true;
        }
    }
}

impl Drop for ResourceStateCommandEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.inner.endEncoding();
            self.ended = true;
        }
    }
}

impl CommandBuffer {
    /// Begins a parallel render encoder with an exclusive command-buffer borrow.
    pub fn parallel_render_encoder<'a>(
        &'a mut self,
        descriptor: &RenderPassDescriptor,
    ) -> Result<ParallelRenderCommandEncoder<'a>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(parallelRenderCommandEncoderWithDescriptor:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::parallelRenderCommandEncoder is unavailable",
            ));
        }
        self.inner
            .parallelRenderCommandEncoderWithDescriptor(&descriptor.inner)
            .map(|inner| ParallelRenderCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a parallel render encoder"))
    }

    /// Begins a resource-state encoder with an exclusive command-buffer borrow.
    pub fn resource_state_encoder<'a>(
        &'a mut self,
    ) -> Result<ResourceStateCommandEncoder<'a>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(resourceStateCommandEncoder))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::resourceStateCommandEncoder is unavailable",
            ));
        }
        self.inner
            .resourceStateCommandEncoder()
            .map(|inner| ResourceStateCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a resource-state encoder"))
    }

    /// Begins a descriptor-configured resource-state encoder.
    pub fn resource_state_encoder_with_descriptor<'a>(
        &'a mut self,
        descriptor: &ResourceStatePassDescriptor,
    ) -> Result<ResourceStateCommandEncoder<'a>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(resourceStateCommandEncoderWithDescriptor:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::resourceStateCommandEncoderWithDescriptor is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the generated descriptor
        // owns an MTLResourceStatePassDescriptor, and the returned protocol
        // object remains inside an exclusive command-buffer borrow.
        let inner: Option<Retained<ProtocolObject<dyn MTLResourceStateCommandEncoder>>> = unsafe {
            msg_send![&*self.inner, resourceStateCommandEncoderWithDescriptor: descriptor.as_inner()]
        };
        inner
            .map(|inner| ResourceStateCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a resource-state encoder"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_store_action_rejects_unknown() {
        assert!(validate_store_action(StoreAction::StoreActionStore).is_ok());
        assert!(validate_store_action(StoreAction::StoreActionUnknown).is_err());
    }

    #[test]
    fn parallel_store_options_accept_declared_bits() {
        assert!(validate_store_action_options(StoreActionOptions::StoreActionOptionNone).is_ok());
        assert!(
            validate_store_action_options(
                StoreActionOptions::StoreActionOptionCustomSamplePositions
            )
            .is_ok()
        );
    }

    #[test]
    fn pass_attachment_indices_are_bounded() {
        assert_eq!(
            PassSampleBufferAttachmentIndex::try_from(3).expect("fourth slot should be valid"),
            PassSampleBufferAttachmentIndex::THREE
        );
        assert!(PassSampleBufferAttachmentIndex::try_from(4).is_err());
    }
}
