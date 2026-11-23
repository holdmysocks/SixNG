//! Audited Metal blit encoder operations.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    CounterSampleBuffer, Fence, IndirectCommandBuffer, Tensor,
};
use crate::metal::generated_value_types::{BlitOption, TensorPlaneType};
use crate::metal::{
    Buffer, BufferReadback, CommandBuffer, PixelFormat, Region, ResourceOptions, StorageMode,
    TensorLayout, Texture, TextureReadback, TextureType,
};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSData, NSRange};
use objc2_metal::{MTLBlitCommandEncoder, MTLCommandEncoder, MTLDevice, MTLTexture as _};
use std::marker::PhantomData;

/// A blit encoder borrowing its command buffer until `end_encoding`.
pub struct BlitCommandEncoder<'a> {
    inner: Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>,
    _command_buffer: PhantomData<&'a mut CommandBuffer>,
    submission_id: u64,
    ended: bool,
    _thread_bound: ThreadBound,
}

impl<'a> BlitCommandEncoder<'a> {
    pub(super) fn new(
        inner: Retained<ProtocolObject<dyn MTLBlitCommandEncoder>>,
        submission_id: u64,
        _command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            inner,
            _command_buffer: PhantomData,
            submission_id,
            ended: false,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Encodes a copy into an internal shared staging buffer.
    pub fn read_buffer(
        &self,
        source: &Buffer,
        source_offset: usize,
        size: usize,
    ) -> Result<BufferReadback, Error> {
        let source_end = source_offset
            .checked_add(size)
            .ok_or_else(|| Error::invalid_argument("readback range overflow"))?;
        if source_end > source.length() {
            return Err(Error::invalid_argument("readback range is out of bounds"));
        }
        let inner = self
            .inner
            .device()
            .newBufferWithLength_options(size, ResourceOptions::SHARED.as_objc())
            .ok_or_else(|| Error::unsupported("Metal could not allocate a staging buffer"))?;
        let buffer = Buffer::new(inner, StorageMode::Shared);
        self.copy_buffer(source, source_offset, &buffer, 0, size)?;
        Ok(BufferReadback {
            buffer,
            submission_id: self.submission_id,
            length: size,
        })
    }

    /// Encodes a checked level-zero 2D texture readback through staging.
    pub fn read_texture(&self, source: &Texture, region: Region) -> Result<TextureReadback, Error> {
        if region.size.depth != 1 || region.origin.z != 0 {
            return Err(Error::invalid_argument(
                "2D texture readback requires z=0 and depth=1",
            ));
        }
        let end_x = region
            .origin
            .x
            .checked_add(region.size.width)
            .ok_or_else(|| Error::invalid_argument("texture readback x range overflow"))?;
        let end_y = region
            .origin
            .y
            .checked_add(region.size.height)
            .ok_or_else(|| Error::invalid_argument("texture readback y range overflow"))?;
        if region.size.width == 0
            || region.size.height == 0
            || end_x > source.width()
            || end_y > source.height()
        {
            return Err(Error::invalid_argument(
                "texture readback region is empty or out of bounds",
            ));
        }
        let pixel_format = source.pixel_format();
        let bytes_per_pixel = match pixel_format {
            PixelFormat::RGBA8_UNORM
            | PixelFormat::BGRA8_UNORM
            | PixelFormat::RGBA8_UNORM_SRGB
            | PixelFormat::BGRA8_UNORM_SRGB
            | PixelFormat::RG16_FLOAT
            | PixelFormat::DEPTH32_FLOAT => 4_usize,
            PixelFormat::R16_FLOAT => 2,
            PixelFormat::RGBA16_FLOAT => 8,
            _ => {
                return Err(Error::unsupported(
                    "texture readback format has no audited byte layout",
                ));
            }
        };
        let unaligned_row = region
            .size
            .width
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| Error::invalid_argument("texture row size overflow"))?;
        let alignment = self
            .inner
            .device()
            .minimumTextureBufferAlignmentForPixelFormat(pixel_format.as_objc())
            .max(1);
        let bytes_per_row = unaligned_row
            .checked_add(alignment - 1)
            .map(|value| value / alignment * alignment)
            .ok_or_else(|| Error::invalid_argument("aligned texture row size overflow"))?;
        let length = bytes_per_row
            .checked_mul(region.size.height)
            .ok_or_else(|| Error::invalid_argument("texture staging size overflow"))?;
        let inner = self
            .inner
            .device()
            .newBufferWithLength_options(length, ResourceOptions::SHARED.as_objc())
            .ok_or_else(|| Error::unsupported("Metal could not allocate texture staging"))?;
        let buffer = Buffer::new(inner, StorageMode::Shared);
        // SAFETY: source bounds, staging length, pixel byte layout, row
        // alignment, origin, and size were all checked above; both retained
        // resources remain alive through command encoding and submission.
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &source.inner,
                0,
                0,
                region.origin.into(),
                region.size.into(),
                &buffer.inner,
                0,
                bytes_per_row,
                length,
            );
        }
        Ok(TextureReadback {
            buffer,
            submission_id: self.submission_id,
            length,
            bytes_per_row,
            width: region.size.width,
            height: region.size.height,
        })
    }

    /// Copies a checked byte range between two buffers.
    pub fn copy_buffer(
        &self,
        source: &Buffer,
        source_offset: usize,
        destination: &Buffer,
        destination_offset: usize,
        size: usize,
    ) -> Result<(), Error> {
        let source_end = source_offset
            .checked_add(size)
            .ok_or_else(|| Error::invalid_argument("source copy range overflow"))?;
        let destination_end = destination_offset
            .checked_add(size)
            .ok_or_else(|| Error::invalid_argument("destination copy range overflow"))?;
        if source_end > source.length() || destination_end > destination.length() {
            return Err(Error::invalid_argument(
                "buffer copy range is out of bounds",
            ));
        }
        if size == 0 {
            return Ok(());
        }
        // SAFETY: both buffers are retained for the encoder lifetime, and the
        // checked offsets and size are inside their Metal allocations.
        unsafe {
            self.inner
                .copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                    &source.inner,
                    source_offset,
                    &destination.inner,
                    destination_offset,
                    size,
                );
        }
        Ok(())
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
        destination_origin: crate::metal::Origin,
    ) -> Result<(), Error> {
        validate_texture_region(source, source_slice, source_level, source_region)?;
        validate_texture_region(
            destination,
            destination_slice,
            destination_level,
            Region::new(destination_origin, source_region.size),
        )?;
        if source.pixel_format() != destination.pixel_format()
            || source.layout().3 != destination.layout().3
        {
            return Err(Error::invalid_argument(
                "texture-region copy requires matching pixel formats and sample counts",
            ));
        }
        // SAFETY: source and destination mip/slice regions, formats, and
        // sample counts were checked and both retained textures outlive this
        // command encoding operation.
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &source.inner,
                source_slice,
                source_level,
                source_region.origin.into(),
                source_region.size.into(),
                &destination.inner,
                destination_slice,
                destination_level,
                destination_origin.into(),
            );
        }
        Ok(())
    }

    /// Copies a checked linear buffer layout into a texture region.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture(
        &self,
        source: &Buffer,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: crate::metal::Size,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: crate::metal::Origin,
    ) -> Result<(), Error> {
        let region = Region::new(destination_origin, source_size);
        validate_texture_region(destination, destination_slice, destination_level, region)?;
        validate_linear_texture_layout(
            &self.inner,
            source,
            source_offset,
            source_bytes_per_row,
            source_bytes_per_image,
            source_size,
            destination.pixel_format(),
        )?;
        // SAFETY: the destination region and complete source buffer byte
        // footprint, including row/image strides and required alignment, were
        // checked above. Both retained resources remain alive for submission.
        unsafe {
            self.inner.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &source.inner,
                source_offset,
                source_bytes_per_row,
                source_bytes_per_image,
                source_size.into(),
                &destination.inner,
                destination_slice,
                destination_level,
                destination_origin.into(),
            );
        }
        Ok(())
    }

    /// Safe substitute for the option-bearing buffer-to-texture overload.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_buffer_to_texture_with_options(
        &self,
        source: &Buffer,
        source_offset: usize,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        source_size: crate::metal::Size,
        destination: &Texture,
        destination_slice: usize,
        destination_level: usize,
        destination_origin: crate::metal::Origin,
        options: BlitOption,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("blit options contain unknown bits"));
        }
        if options.as_raw() != BlitOption::BlitOptionNone.as_raw() {
            return Err(Error::unsupported(
                "depth/stencil and row-linear PVRTC copies require an unaudited byte layout",
            ));
        }
        self.copy_buffer_to_texture(
            source,
            source_offset,
            source_bytes_per_row,
            source_bytes_per_image,
            source_size,
            destination,
            destination_slice,
            destination_level,
            destination_origin,
        )
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
        validate_texture_region(source, source_slice, source_level, source_region)?;
        validate_linear_texture_layout(
            &self.inner,
            destination,
            destination_offset,
            destination_bytes_per_row,
            destination_bytes_per_image,
            source_region.size,
            source.pixel_format(),
        )?;
        // SAFETY: the source region and complete destination buffer byte
        // footprint, including row/image strides and required alignment, were
        // checked above. Both retained resources remain alive for submission.
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                &source.inner,
                source_slice,
                source_level,
                source_region.origin.into(),
                source_region.size.into(),
                &destination.inner,
                destination_offset,
                destination_bytes_per_row,
                destination_bytes_per_image,
            );
        }
        Ok(())
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
        if !options.is_valid() {
            return Err(Error::invalid_argument("blit options contain unknown bits"));
        }
        if options.as_raw() != BlitOption::BlitOptionNone.as_raw() {
            return Err(Error::unsupported(
                "depth/stencil and row-linear PVRTC copies require an unaudited byte layout",
            ));
        }
        self.copy_texture_to_buffer(
            source,
            source_slice,
            source_level,
            source_region,
            destination,
            destination_offset,
            destination_bytes_per_row,
            destination_bytes_per_image,
        )
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
        validate_tensor_layout(source, source_layout)?;
        validate_tensor_layout(destination, destination_layout)?;
        validate_tensor_slice(source_layout, source_origin, source_dimensions)?;
        validate_tensor_slice(
            destination_layout,
            destination_origin,
            destination_dimensions,
        )?;
        if source_layout.data_type() != destination_layout.data_type()
            || checked_element_count(source_dimensions)?
                != checked_element_count(destination_dimensions)?
        {
            return Err(Error::invalid_argument(
                "tensor copy requires matching data types and element counts",
            ));
        }
        let source_origin = ObjectiveCTensorExtents::new(source_origin)?;
        let source_dimensions = ObjectiveCTensorExtents::new(source_dimensions)?;
        let destination_origin = ObjectiveCTensorExtents::new(destination_origin)?;
        let destination_dimensions = ObjectiveCTensorExtents::new(destination_dimensions)?;
        if !self.inner.respondsToSelector(
            sel!(copyFromTensor:sourceOrigin:sourceDimensions:toTensor:destinationOrigin:destinationDimensions:),
        ) {
            return Err(Error::unsupported("tensor blit copies are unavailable"));
        }
        // SAFETY: runtime layouts match checked Rust layout proofs; origins,
        // extents, types, element counts, and all retained arguments were checked.
        unsafe {
            let _: () = msg_send![
                &*self.inner,
                copyFromTensor: source.as_inner(),
                sourceOrigin: &*source_origin.inner,
                sourceDimensions: &*source_dimensions.inner,
                toTensor: destination.as_inner(),
                destinationOrigin: &*destination_origin.inner,
                destinationDimensions: &*destination_dimensions.inner
            ];
        }
        Ok(())
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
        if !source_plane.is_valid() || !destination_plane.is_valid() {
            return Err(Error::invalid_argument("tensor plane is invalid"));
        }
        let data = TensorPlaneType::TensorPlaneTypeData.as_raw();
        if source_plane.as_raw() != data || destination_plane.as_raw() != data {
            return Err(Error::unsupported(
                "scale-plane blits require an audited auxiliary-plane layout",
            ));
        }
        self.copy_tensor(
            source,
            source_layout,
            source_origin,
            source_dimensions,
            destination,
            destination_layout,
            destination_origin,
            destination_dimensions,
        )
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
        if slice_count == 0 || level_count == 0 {
            return Err(Error::invalid_argument(
                "texture slice and level counts must be non-zero",
            ));
        }
        let source_slice_end = source_slice
            .checked_add(slice_count)
            .ok_or_else(|| Error::invalid_argument("source texture slice range overflow"))?;
        let destination_slice_end = destination_slice
            .checked_add(slice_count)
            .ok_or_else(|| Error::invalid_argument("destination texture slice range overflow"))?;
        let source_level_end = source_level
            .checked_add(level_count)
            .ok_or_else(|| Error::invalid_argument("source texture level range overflow"))?;
        let destination_level_end = destination_level
            .checked_add(level_count)
            .ok_or_else(|| Error::invalid_argument("destination texture level range overflow"))?;
        let (_, _, source_levels, source_samples) = source.layout();
        let (_, _, destination_levels, destination_samples) = destination.layout();
        if source_slice_end > texture_slice_count(source)?
            || destination_slice_end > texture_slice_count(destination)?
            || source_level_end > source_levels
            || destination_level_end > destination_levels
            || source_samples != 1
            || destination_samples != 1
            || source.pixel_format() != destination.pixel_format()
            || source.texture_type()? != destination.texture_type()?
        {
            return Err(Error::invalid_argument(
                "texture subresource ranges or layouts are incompatible",
            ));
        }
        for relative_level in 0..level_count {
            if mip_extent(source, source_level + relative_level)?
                != mip_extent(destination, destination_level + relative_level)?
            {
                return Err(Error::invalid_argument(
                    "texture mip dimensions are incompatible",
                ));
            }
        }
        // SAFETY: non-empty slice/level ranges, every mip extent, texture type,
        // sample count, and pixel format were checked for compatibility.
        unsafe {
            self.inner.copyFromTexture_sourceSlice_sourceLevel_toTexture_destinationSlice_destinationLevel_sliceCount_levelCount(
                &source.inner,
                source_slice,
                source_level,
                &destination.inner,
                destination_slice,
                destination_level,
                slice_count,
                level_count,
            );
        }
        Ok(())
    }

    /// Fills a checked buffer range with a byte value.
    pub fn fill_buffer(
        &self,
        buffer: &Buffer,
        range: std::ops::Range<usize>,
        value: u8,
    ) -> Result<(), Error> {
        if range.start > range.end || range.end > buffer.length() {
            return Err(Error::invalid_argument(
                "buffer fill range is out of bounds",
            ));
        }
        if !range.is_empty() {
            self.inner.fillBuffer_range_value(
                &buffer.inner,
                NSRange::new(range.start, range.len()),
                value,
            );
        }
        Ok(())
    }

    /// Copies an entire compatible texture.
    pub fn copy_texture(&self, source: &Texture, destination: &Texture) -> Result<(), Error> {
        if source.width() != destination.width()
            || source.height() != destination.height()
            || source.layout() != destination.layout()
            || source.texture_type()? != destination.texture_type()?
            || source.pixel_format() != destination.pixel_format()
        {
            return Err(Error::invalid_argument(
                "texture copy requires identical type, layout, dimensions, and pixel format",
            ));
        }
        // SAFETY: both textures are retained and their dimensions and pixel
        // formats were checked for whole-texture copy compatibility.
        unsafe {
            self.inner
                .copyFromTexture_toTexture(&source.inner, &destination.inner)
        };
        Ok(())
    }

    /// Generates mipmaps for a texture with more than one mip level.
    pub fn generate_mipmaps(&self, texture: &Texture) -> Result<(), Error> {
        if texture.inner.mipmapLevelCount() <= 1 {
            return Err(Error::invalid_argument(
                "mipmap generation requires more than one mip level",
            ));
        }
        self.inner.generateMipmapsForTexture(&texture.inner);
        Ok(())
    }

    /// Synchronizes one texture slice and mip level for managed CPU access.
    pub fn synchronize_texture(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        if texture.storage_mode() != StorageMode::Managed {
            return Err(Error::invalid_argument(
                "only managed textures require explicit synchronization",
            ));
        }
        validate_texture_subresource(texture, slice, level)?;
        // SAFETY: the texture is retained and both the slice and mip level are
        // within its declared ranges.
        unsafe {
            self.inner
                .synchronizeTexture_slice_level(&texture.inner, slice, level)
        };
        Ok(())
    }

    /// Synchronizes a managed buffer for subsequent CPU access.
    pub fn synchronize_buffer(&self, buffer: &Buffer) -> Result<(), Error> {
        if buffer.storage_mode() != StorageMode::Managed {
            return Err(Error::invalid_argument(
                "only managed buffers require explicit synchronization",
            ));
        }
        // SAFETY: MTLBuffer inherits MTLResource, the retained buffer remains
        // live, and its managed storage mode was checked above.
        unsafe {
            let _: () = objc2::msg_send![&*self.inner, synchronizeResource: buffer.as_any_object()];
        }
        Ok(())
    }

    /// Synchronizes an entire managed texture through MTLResource semantics.
    pub fn synchronize_texture_resource(&self, texture: &Texture) -> Result<(), Error> {
        if texture.storage_mode() != StorageMode::Managed {
            return Err(Error::invalid_argument(
                "only managed textures require explicit synchronization",
            ));
        }
        // SAFETY: MTLTexture inherits MTLResource, the retained texture remains
        // live, and its managed storage mode was checked above.
        unsafe {
            let _: () = msg_send![&*self.inner, synchronizeResource: texture.as_any_object()];
        }
        Ok(())
    }

    /// Optimizes a checked texture slice and mip level for CPU access.
    pub fn optimize_texture_slice_for_cpu(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        validate_texture_subresource(texture, slice, level)?;
        if !self
            .inner
            .respondsToSelector(sel!(optimizeContentsForCPUAccess:slice:level:))
        {
            return Err(Error::unsupported(
                "per-subresource CPU texture optimization is unavailable",
            ));
        }
        // SAFETY: selector availability and subresource bounds were checked.
        unsafe {
            self.inner
                .optimizeContentsForCPUAccess_slice_level(&texture.inner, slice, level)
        };
        Ok(())
    }

    /// Optimizes a checked texture slice and mip level for GPU access.
    pub fn optimize_texture_slice_for_gpu(
        &self,
        texture: &Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        validate_texture_subresource(texture, slice, level)?;
        if !self
            .inner
            .respondsToSelector(sel!(optimizeContentsForGPUAccess:slice:level:))
        {
            return Err(Error::unsupported(
                "per-subresource GPU texture optimization is unavailable",
            ));
        }
        // SAFETY: selector availability and subresource bounds were checked.
        unsafe {
            self.inner
                .optimizeContentsForGPUAccess_slice_level(&texture.inner, slice, level)
        };
        Ok(())
    }

    /// Resets a checked range in an indirect command buffer.
    pub fn reset_indirect_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        validate_indirect_range(buffer, &range)?;
        if !self
            .inner
            .respondsToSelector(sel!(resetCommandsInBuffer:withRange:))
        {
            return Err(Error::unsupported("indirect command reset is unavailable"));
        }
        // SAFETY: the wrapper is type-checked as MTLIndirectCommandBuffer, the
        // selector is present, and the range is within its reported size.
        unsafe {
            let _: () = objc2::msg_send![
                &*self.inner,
                resetCommandsInBuffer: buffer.as_inner(),
                withRange: NSRange::new(range.start, range.len())
            ];
        }
        Ok(())
    }

    /// Copies a checked range between indirect command buffers.
    pub fn copy_indirect_commands(
        &self,
        source: &IndirectCommandBuffer,
        source_range: std::ops::Range<usize>,
        destination: &IndirectCommandBuffer,
        destination_index: usize,
    ) -> Result<(), Error> {
        validate_indirect_range(source, &source_range)?;
        let destination_end = destination_index
            .checked_add(source_range.len())
            .ok_or_else(|| Error::invalid_argument("indirect command destination overflow"))?;
        if destination_end > indirect_size(destination)? {
            return Err(Error::invalid_argument(
                "indirect command destination range is out of bounds",
            ));
        }
        if !self.inner.respondsToSelector(
            sel!(copyIndirectCommandBuffer:sourceRange:destination:destinationIndex:),
        ) {
            return Err(Error::unsupported(
                "indirect command buffer copy is unavailable",
            ));
        }
        // SAFETY: both wrappers are type-checked as indirect command buffers,
        // selector availability was checked, and both ranges are in bounds.
        unsafe {
            let _: () = objc2::msg_send![
                &*self.inner,
                copyIndirectCommandBuffer: source.as_inner(),
                sourceRange: NSRange::new(source_range.start, source_range.len()),
                destination: destination.as_inner(),
                destinationIndex: destination_index
            ];
        }
        Ok(())
    }

    /// Optimizes a checked indirect command range for execution.
    pub fn optimize_indirect_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        validate_indirect_range(buffer, &range)?;
        if !self
            .inner
            .respondsToSelector(sel!(optimizeIndirectCommandBuffer:withRange:))
        {
            return Err(Error::unsupported(
                "indirect command optimization is unavailable",
            ));
        }
        // SAFETY: the wrapper type, selector availability, and range were checked.
        unsafe {
            let _: () = objc2::msg_send![
                &*self.inner,
                optimizeIndirectCommandBuffer: buffer.as_inner(),
                withRange: NSRange::new(range.start, range.len())
            ];
        }
        Ok(())
    }

    /// Samples counters at a checked index.
    pub fn sample_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        let sample_count = sample_buffer.sample_count()?;
        if sample_index >= sample_count {
            return Err(Error::invalid_argument(
                "counter sample index is out of bounds",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(sampleCountersInBuffer:atSampleIndex:withBarrier:))
        {
            return Err(Error::unsupported("blit counter sampling is unavailable"));
        }
        // SAFETY: the wrapper is type-checked as MTLCounterSampleBuffer, the
        // selector exists, and sample_index is inside sampleCount.
        unsafe {
            let _: () = objc2::msg_send![
                &*self.inner,
                sampleCountersInBuffer: sample_buffer.as_inner(),
                atSampleIndex: sample_index,
                withBarrier: barrier
            ];
        }
        Ok(())
    }

    /// Resolves a checked counter range into submission-bound staging.
    ///
    /// The shared sample buffer's CPU resolve result is used only to obtain the
    /// framework-defined exact byte length. Counter contents are returned only
    /// through `CompletedCommandBuffer::resolve_buffer` for this submission.
    pub fn read_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<BufferReadback, Error> {
        let sample_count = sample_buffer.sample_count()?;
        if range.start >= range.end || range.end > sample_count {
            return Err(Error::invalid_argument(
                "counter resolve range is empty or out of bounds",
            ));
        }
        let object = sample_buffer.as_inner();
        // SAFETY: NSObject respondsToSelector: uses the selector/bool ABI.
        let can_measure: bool =
            unsafe { msg_send![object, respondsToSelector: sel!(resolveCounterRange:)] };
        if !can_measure
            || !self.inner.respondsToSelector(
                sel!(resolveCounters:inRange:destinationBuffer:destinationOffset:),
            )
        {
            return Err(Error::unsupported("counter resolving is unavailable"));
        }
        // SAFETY: range is inside sampleCount; this API returns owned NSData
        // for shared counter buffers. Its contents are discarded and only its
        // exact framework-defined byte length is used for staging allocation.
        let measured: Option<Retained<NSData>> = unsafe {
            msg_send![object, resolveCounterRange: NSRange::new(range.start, range.len())]
        };
        let length = measured
            .map(|data| data.length())
            .filter(|&length| length != 0)
            .ok_or_else(|| {
                Error::unsupported("counter byte layout cannot be measured for this sample buffer")
            })?;
        let inner = self
            .inner
            .device()
            .newBufferWithLength_options(length, ResourceOptions::SHARED.as_objc())
            .ok_or_else(|| Error::unsupported("Metal could not allocate counter staging"))?;
        let buffer = Buffer::new(inner, StorageMode::Shared);
        // SAFETY: sample range bounds and exact resolved byte length were
        // obtained from Metal; destination offset is zero and the staging
        // buffer has exactly that retained allocation.
        unsafe {
            let _: () = msg_send![
                &*self.inner,
                resolveCounters: object,
                inRange: NSRange::new(range.start, range.len()),
                destinationBuffer: buffer.as_any_object(),
                destinationOffset: 0usize
            ];
        }
        Ok(BufferReadback {
            buffer,
            submission_id: self.submission_id,
            length,
        })
    }

    /// Resets sparse texture access counters for a checked region.
    pub fn reset_texture_access_counters(
        &self,
        texture: &Texture,
        region: Region,
        level: usize,
        slice: usize,
    ) -> Result<(), Error> {
        if !texture.is_sparse() {
            return Err(Error::invalid_argument(
                "texture access counters require a sparse texture",
            ));
        }
        validate_sparse_tile_region(&self.inner, texture, slice, level, region)?;
        if !self
            .inner
            .respondsToSelector(sel!(resetTextureAccessCounters:region:mipLevel:slice:))
        {
            return Err(Error::unsupported(
                "sparse texture access counters are unavailable",
            ));
        }
        // SAFETY: selector availability, sparse-texture requirement, and the
        // complete mip/slice region were checked above.
        unsafe {
            self.inner.resetTextureAccessCounters_region_mipLevel_slice(
                &texture.inner,
                region.into(),
                level,
                slice,
            );
        }
        Ok(())
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
        if !texture.is_sparse() {
            return Err(Error::invalid_argument(
                "texture access counters require a sparse texture",
            ));
        }
        validate_sparse_tile_region(&self.inner, texture, slice, level, region)?;
        if !self.inner.respondsToSelector(sel!(getTextureAccessCounters:region:mipLevel:slice:resetCounters:countersBuffer:countersBufferOffset:)) {
            return Err(Error::unsupported(
                "sparse texture access counters are unavailable",
            ));
        }
        // Apple defines one row-major uint32 counter per requested sparse tile.
        let length = region
            .size
            .width
            .checked_mul(region.size.height)
            .and_then(|value| value.checked_mul(region.size.depth))
            .and_then(|value| value.checked_mul(std::mem::size_of::<u32>()))
            .ok_or_else(|| Error::invalid_argument("texture counter staging size overflow"))?;
        let inner = self
            .inner
            .device()
            .newBufferWithLength_options(length, ResourceOptions::SHARED.as_objc())
            .ok_or_else(|| Error::unsupported("Metal could not allocate counter staging"))?;
        let buffer = Buffer::new(inner, StorageMode::Shared);
        // SAFETY: sparse tile-region bounds were validated, the selector is
        // present, and staging contains exactly one u32 per requested tile.
        unsafe {
            self.inner.getTextureAccessCounters_region_mipLevel_slice_resetCounters_countersBuffer_countersBufferOffset(
                &texture.inner,
                region.into(),
                level,
                slice,
                reset_counters,
                &buffer.inner,
                0,
            );
        }
        Ok(BufferReadback {
            buffer,
            submission_id: self.submission_id,
            length,
        })
    }

    /// Updates a fence after all prior blit commands.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(updateFence:)) {
            return Err(Error::unsupported("blit fences are unavailable"));
        }
        // SAFETY: Fence is a type-checked retained MTLFence wrapper and the
        // selector was checked before sending it.
        unsafe {
            let _: () = objc2::msg_send![&*self.inner, updateFence: fence.as_inner()];
        }
        Ok(())
    }

    /// Waits for a fence before subsequent blit commands.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(waitForFence:)) {
            return Err(Error::unsupported("blit fences are unavailable"));
        }
        // SAFETY: Fence is a type-checked retained MTLFence wrapper and the
        // selector was checked before sending it.
        unsafe {
            let _: () = objc2::msg_send![&*self.inner, waitForFence: fence.as_inner()];
        }
        Ok(())
    }

    /// Optimizes an entire texture for CPU access when supported.
    pub fn optimize_texture_for_cpu(&self, texture: &Texture) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(optimizeContentsForCPUAccess:))
        {
            return Err(Error::unsupported(
                "CPU texture-content optimization is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the retained texture is
        // valid for the duration of encoding.
        unsafe { self.inner.optimizeContentsForCPUAccess(&texture.inner) };
        Ok(())
    }

    /// Optimizes an entire texture for GPU access when supported.
    pub fn optimize_texture_for_gpu(&self, texture: &Texture) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(optimizeContentsForGPUAccess:))
        {
            return Err(Error::unsupported(
                "GPU texture-content optimization is unavailable",
            ));
        }
        self.inner.optimizeContentsForGPUAccess(&texture.inner);
        Ok(())
    }

    /// Ends encoding and releases the borrow of the command buffer.
    pub fn end_encoding(mut self) {
        self.inner.endEncoding();
        self.ended = true;
    }
}

fn texture_slice_count(texture: &Texture) -> Result<usize, Error> {
    let (_, array_length, _, _) = texture.layout();
    match texture.texture_type()? {
        TextureType::D1Array | TextureType::D2Array | TextureType::D2MultisampleArray => {
            Ok(array_length)
        }
        TextureType::Cube => Ok(6),
        TextureType::CubeArray => array_length
            .checked_mul(6)
            .ok_or_else(|| Error::invalid_argument("cube-array slice count overflow")),
        _ => Ok(1),
    }
}

fn validate_texture_subresource(
    texture: &Texture,
    slice: usize,
    level: usize,
) -> Result<(), Error> {
    let (_, _, levels, samples) = texture.layout();
    if level >= levels || slice >= texture_slice_count(texture)? {
        return Err(Error::invalid_argument(
            "texture slice or mip level is out of bounds",
        ));
    }
    if samples > 1 {
        return Err(Error::unsupported(
            "multisample texture blits are not exposed by this safe path",
        ));
    }
    Ok(())
}

fn validate_texture_region(
    texture: &Texture,
    slice: usize,
    level: usize,
    region: Region,
) -> Result<(), Error> {
    validate_texture_subresource(texture, slice, level)?;
    if region.size.width == 0 || region.size.height == 0 || region.size.depth == 0 {
        return Err(Error::invalid_argument(
            "texture copy region must be non-empty",
        ));
    }
    let (width, height, depth) = mip_extent(texture, level)?;
    let end_x = region
        .origin
        .x
        .checked_add(region.size.width)
        .ok_or_else(|| Error::invalid_argument("texture x range overflow"))?;
    let end_y = region
        .origin
        .y
        .checked_add(region.size.height)
        .ok_or_else(|| Error::invalid_argument("texture y range overflow"))?;
    let end_z = region
        .origin
        .z
        .checked_add(region.size.depth)
        .ok_or_else(|| Error::invalid_argument("texture z range overflow"))?;
    if end_x > width || end_y > height || end_z > depth {
        return Err(Error::invalid_argument(
            "texture copy region is out of mip bounds",
        ));
    }
    Ok(())
}

fn validate_sparse_tile_region(
    encoder: &ProtocolObject<dyn MTLBlitCommandEncoder>,
    texture: &Texture,
    slice: usize,
    level: usize,
    region: Region,
) -> Result<(), Error> {
    validate_texture_subresource(texture, slice, level)?;
    if region.size.width == 0 || region.size.height == 0 || region.size.depth == 0 {
        return Err(Error::invalid_argument(
            "sparse tile region must be non-empty",
        ));
    }
    if !encoder
        .device()
        .respondsToSelector(sel!(sparseTileSizeWithTextureType:pixelFormat:sampleCount:))
    {
        return Err(Error::unsupported("sparse tile geometry is unavailable"));
    }
    // SAFETY: selector availability was checked and all value arguments come
    // directly from a retained texture with checked enum representations.
    let tile = unsafe {
        encoder
            .device()
            .sparseTileSizeWithTextureType_pixelFormat_sampleCount(
                texture.texture_type()?.as_objc(),
                texture.pixel_format().as_objc(),
                texture.layout().3,
            )
    };
    if tile.width == 0 || tile.height == 0 || tile.depth == 0 {
        return Err(Error::unsupported(
            "Metal returned invalid sparse tile geometry",
        ));
    }
    let (width, height, depth) = mip_extent(texture, level)?;
    let tiles = (
        width.div_ceil(tile.width),
        height.div_ceil(tile.height),
        depth.div_ceil(tile.depth),
    );
    let end_x = region.origin.x.checked_add(region.size.width);
    let end_y = region.origin.y.checked_add(region.size.height);
    let end_z = region.origin.z.checked_add(region.size.depth);
    if end_x.is_none_or(|end| end > tiles.0)
        || end_y.is_none_or(|end| end > tiles.1)
        || end_z.is_none_or(|end| end > tiles.2)
    {
        return Err(Error::invalid_argument(
            "sparse tile region is out of mip bounds",
        ));
    }
    Ok(())
}

fn mip_extent(texture: &Texture, level: usize) -> Result<(usize, usize, usize), Error> {
    let (depth, _, _, _) = texture.layout();
    let shift = u32::try_from(level)
        .map_err(|_| Error::invalid_argument("texture mip level cannot be represented"))?;
    let width = texture.width().checked_shr(shift).unwrap_or(0).max(1);
    let height = texture.height().checked_shr(shift).unwrap_or(0).max(1);
    let depth = if texture.texture_type()? == TextureType::D3 {
        depth.checked_shr(shift).unwrap_or(0).max(1)
    } else {
        1
    };
    Ok((width, height, depth))
}

fn validate_linear_texture_layout(
    encoder: &ProtocolObject<dyn MTLBlitCommandEncoder>,
    buffer: &Buffer,
    offset: usize,
    bytes_per_row: usize,
    bytes_per_image: usize,
    size: crate::metal::Size,
    pixel_format: PixelFormat,
) -> Result<(), Error> {
    let bytes_per_pixel = super::mtl_texture::audited_bytes_per_pixel(pixel_format)?;
    let active_row_bytes = size
        .width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Error::invalid_argument("active texture row size overflow"))?;
    let minimum_image_bytes = bytes_per_row
        .checked_mul(size.height)
        .ok_or_else(|| Error::invalid_argument("texture image stride overflow"))?;
    if bytes_per_row < active_row_bytes || bytes_per_image < minimum_image_bytes {
        return Err(Error::invalid_argument(
            "texture row or image stride is too small",
        ));
    }
    let alignment = encoder
        .device()
        .minimumLinearTextureAlignmentForPixelFormat(pixel_format.as_objc())
        .max(1);
    if !offset.is_multiple_of(alignment) || !bytes_per_row.is_multiple_of(alignment) {
        return Err(Error::invalid_argument(
            "texture buffer offset and row stride do not meet Metal alignment",
        ));
    }
    let required = offset
        .checked_add(
            bytes_per_image
                .checked_mul(size.depth.saturating_sub(1))
                .ok_or_else(|| Error::invalid_argument("texture depth footprint overflow"))?,
        )
        .and_then(|value| {
            value.checked_add(bytes_per_row.checked_mul(size.height.saturating_sub(1))?)
        })
        .and_then(|value| value.checked_add(active_row_bytes))
        .ok_or_else(|| Error::invalid_argument("texture buffer footprint overflow"))?;
    if required > buffer.length() {
        return Err(Error::invalid_argument(
            "texture buffer footprint is out of bounds",
        ));
    }
    Ok(())
}

fn indirect_size(buffer: &IndirectCommandBuffer) -> Result<usize, Error> {
    buffer.size()
}

fn validate_indirect_range(
    buffer: &IndirectCommandBuffer,
    range: &std::ops::Range<usize>,
) -> Result<(), Error> {
    if range.start > range.end || range.end > indirect_size(buffer)? {
        return Err(Error::invalid_argument(
            "indirect command range is out of bounds",
        ));
    }
    Ok(())
}

struct ObjectiveCTensorExtents {
    inner: Retained<AnyObject>,
}

impl ObjectiveCTensorExtents {
    fn new(values: &[usize]) -> Result<Self, Error> {
        if values.len() > crate::metal::MAX_TENSOR_RANK
            || values.iter().any(|&value| value > isize::MAX as usize)
        {
            return Err(Error::invalid_argument(
                "tensor extents exceed Metal's rank or integer limits",
            ));
        }
        let class = AnyClass::get(c"MTLTensorExtents")
            .ok_or_else(|| Error::unsupported("MTLTensorExtents is unavailable"))?;
        // SAFETY: the class identity is checked before querying its initializer.
        let available: bool =
            unsafe { msg_send![class, instancesRespondToSelector: sel!(initWithRank:values:)] };
        if !available {
            return Err(Error::unsupported(
                "MTLTensorExtents initializer is unavailable",
            ));
        }
        let signed: Vec<isize> = values.iter().map(|&value| value as isize).collect();
        // SAFETY: alloc is sent to the checked MTLTensorExtents class.
        let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let pointer = if signed.is_empty() {
            std::ptr::null()
        } else {
            signed.as_ptr()
        };
        // SAFETY: pointer covers exactly rank NSInteger elements for this call;
        // MTLTensorExtents copies the input during initialization.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![allocated, initWithRank: signed.len(), values: pointer] };
        inner
            .map(|inner| Self { inner })
            .ok_or_else(|| Error::invalid_argument("Metal rejected tensor extents"))
    }
}

fn read_tensor_extents(object: &AnyObject) -> Result<Vec<usize>, Error> {
    // SAFETY: NSObject respondsToSelector: uses the declared selector/bool ABI.
    let has_rank: bool = unsafe { msg_send![object, respondsToSelector: sel!(rank)] };
    // SAFETY: NSObject respondsToSelector: uses the declared selector/bool ABI.
    let has_extent: bool =
        unsafe { msg_send![object, respondsToSelector: sel!(extentAtDimensionIndex:)] };
    if !has_rank || !has_extent {
        return Err(Error::unsupported(
            "tensor extent inspection is unavailable",
        ));
    }
    // SAFETY: rank selector and NSUInteger ABI were checked above.
    let rank: usize = unsafe { msg_send![object, rank] };
    if rank > crate::metal::MAX_TENSOR_RANK {
        return Err(Error::unsupported("Metal returned an invalid tensor rank"));
    }
    let mut values = Vec::with_capacity(rank);
    for index in 0..rank {
        // SAFETY: extent selector exists and index is strictly below rank.
        let value: isize = unsafe { msg_send![object, extentAtDimensionIndex: index] };
        values.push(
            usize::try_from(value)
                .map_err(|_| Error::unsupported("Metal returned a negative tensor extent"))?,
        );
    }
    Ok(values)
}

fn validate_tensor_layout(tensor: &Tensor, layout: &TensorLayout) -> Result<(), Error> {
    if tensor.data_type()? != layout.data_type() {
        return Err(Error::invalid_argument(
            "tensor data type does not match its layout proof",
        ));
    }
    let dimensions = tensor
        .dimensions()?
        .ok_or_else(|| Error::unsupported("tensor dimensions are unavailable"))?;
    let strides = tensor
        .strides()?
        .ok_or_else(|| Error::unsupported("tensor strides are unavailable"))?;
    if read_tensor_extents(dimensions.as_inner())? != layout.dimensions().as_slice()
        || read_tensor_extents(strides.as_inner())? != layout.strides().as_slice()
    {
        return Err(Error::invalid_argument(
            "tensor runtime layout does not match its Rust proof",
        ));
    }
    Ok(())
}

fn validate_tensor_slice(
    layout: &TensorLayout,
    origin: &[usize],
    dimensions: &[usize],
) -> Result<(), Error> {
    let rank = layout.dimensions().rank();
    if origin.len() != rank || dimensions.len() != rank || dimensions.contains(&0) {
        return Err(Error::invalid_argument(
            "tensor slice origin and dimensions must match rank and be non-zero",
        ));
    }
    for ((&origin, &size), &extent) in origin
        .iter()
        .zip(dimensions)
        .zip(layout.dimensions().as_slice())
    {
        let end = origin
            .checked_add(size)
            .ok_or_else(|| Error::invalid_argument("tensor slice range overflow"))?;
        if end > extent {
            return Err(Error::invalid_argument(
                "tensor slice is out of layout bounds",
            ));
        }
    }
    Ok(())
}

fn checked_element_count(dimensions: &[usize]) -> Result<usize, Error> {
    dimensions.iter().try_fold(1_usize, |count, &dimension| {
        count
            .checked_mul(dimension)
            .ok_or_else(|| Error::invalid_argument("tensor slice element count overflow"))
    })
}

impl Drop for BlitCommandEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.inner.endEncoding();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metal::generated_value_types::TensorDataType;

    #[test]
    fn tensor_slice_bounds_are_checked() {
        let layout = TensorLayout::dense(&[4, 3], TensorDataType::TensorDataTypeFloat32).unwrap();
        assert!(validate_tensor_slice(&layout, &[1, 1], &[3, 2]).is_ok());
        assert!(validate_tensor_slice(&layout, &[2, 1], &[3, 2]).is_err());
        assert!(validate_tensor_slice(&layout, &[0], &[1]).is_err());
        assert!(validate_tensor_slice(&layout, &[0, 0], &[4, 0]).is_err());
    }

    #[test]
    fn tensor_element_count_rejects_overflow() {
        assert_eq!(checked_element_count(&[2, 3, 4]).unwrap(), 24);
        assert!(checked_element_count(&[usize::MAX, 2]).is_err());
    }
}
