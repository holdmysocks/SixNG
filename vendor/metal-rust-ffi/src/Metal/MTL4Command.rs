//! Audited dynamic dispatch for Metal 4 command objects.
//!
//! Metal 4 is weak-linked and may be absent even when the crate was built with
//! a newer SDK. Every operation in this module checks its selector before
//! crossing the Objective-C boundary.

use crate::foundation::{Error, metal_error};
use crate::quartz_core::Drawable as LayerDrawable;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{
    MTL4BufferRange, MTL4CopySparseBufferMappingOperation, MTL4CopySparseTextureMappingOperation,
    MTL4UpdateSparseBufferMappingOperation, MTL4UpdateSparseTextureMappingOperation, MTLBlitOption,
    MTLBuffer, MTLDevice, MTLDrawable, MTLIndexType, MTLScissorRect, MTLSize,
    MTLSparseTextureMappingMode, MTLVertexAmplificationViewMapping, MTLViewport,
};

use super::generated_object_types::{metal, metal4, metal4_fx};
use super::generated_struct_types::{
    CopySparseBufferMappingOperation, CopySparseTextureMappingOperation,
    UpdateSparseBufferMappingOperation, UpdateSparseTextureMappingOperation,
};
use super::generated_struct_types::{ScissorRect, VertexAmplificationViewMapping};
use super::generated_value_types::{
    AccelerationStructureRefitOptions, BlitOption, CullMode, DepthClipMode, IndexType,
    RenderEncoderOptions, RenderStages, Stages, StoreAction, TensorPlaneType, TimestampGranularity,
    TriangleFillMode, VisibilityOptions, VisibilityResultMode, Winding,
};
use super::{Origin, PrimitiveType, Region, Size, Viewport};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

static NEXT_MTL4_SUBMISSION_ID: AtomicU64 = AtomicU64::new(1);
type CompletionState = Arc<(Mutex<Option<Result<(), Error>>>, Condvar)>;

/// A checked span of texture slices and mip levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureSubresourceSpan {
    /// First array slice.
    pub slice: usize,
    /// First mip level.
    pub level: usize,
    /// Number of array slices.
    pub slice_count: usize,
    /// Number of mip levels.
    pub level_count: usize,
}

/// A checked texture source region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRegionSelection {
    /// Array slice.
    pub slice: usize,
    /// Mip level.
    pub level: usize,
    /// Region within the subresource.
    pub region: Region,
}
/// A checked texture destination origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureOriginSelection {
    /// Array slice.
    pub slice: usize,
    /// Mip level.
    pub level: usize,
    /// Origin within the subresource.
    pub origin: Origin,
}
/// A checked linear image layout within a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferImageLayout {
    /// Byte offset to the first image byte.
    pub offset: usize,
    /// Bytes between adjacent rows.
    pub bytes_per_row: usize,
    /// Bytes between adjacent depth images.
    pub bytes_per_image: usize,
}

/// Parameters for an indexed draw with explicit base values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedDraw {
    /// Number of indices.
    pub index_count: usize,
    /// Index element type.
    pub index_type: IndexType,
    /// Checked index-buffer byte range.
    pub index_range: std::ops::Range<usize>,
    /// Number of instances.
    pub instance_count: usize,
    /// Signed base vertex.
    pub base_vertex: isize,
    /// Base instance.
    pub base_instance: usize,
}

/// Borrowed checked tensor regions for one tensor copy.
pub struct TensorCopyRegion<'a> {
    /// Source origin.
    pub source_origin: &'a super::CheckedTensorExtents,
    /// Source dimensions.
    pub source_dimensions: &'a super::CheckedTensorExtents,
    /// Destination origin.
    pub destination_origin: &'a super::CheckedTensorExtents,
    /// Destination dimensions.
    pub destination_dimensions: &'a super::CheckedTensorExtents,
}

fn checked_buffer_address(
    buffer: &super::Buffer,
    offset: usize,
    required: usize,
    alignment: usize,
    operation: &str,
) -> Result<u64, Error> {
    let end = offset
        .checked_add(required)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} buffer range overflows")))?;
    if end > buffer.length() || alignment == 0 || !offset.is_multiple_of(alignment) {
        return Err(Error::invalid_argument(format!(
            "{operation} requires an in-bounds {alignment}-byte aligned buffer range"
        )));
    }
    buffer
        .inner
        .gpuAddress()
        .checked_add(offset as u64)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} GPU address overflows")))
}

fn checked_mtl4_buffer_range(
    buffer: &super::Buffer,
    range: std::ops::Range<usize>,
    operation: &str,
) -> Result<MTL4BufferRange, Error> {
    let range = checked_range(buffer.length(), range, operation)?;
    let address = checked_buffer_address(buffer, range.location, range.length, 1, operation)?;
    Ok(MTL4BufferRange {
        bufferAddress: address,
        length: range.length as u64,
    })
}

fn checked_range(
    total: usize,
    range: std::ops::Range<usize>,
    operation: &str,
) -> Result<NSRange, Error> {
    if range.start > range.end || range.end > total {
        return Err(Error::invalid_argument(format!(
            "{operation} range is out of bounds"
        )));
    }
    Ok(NSRange::new(range.start, range.len()))
}

fn submission_contains(submission_ids: &[u64], readback_id: u64) -> bool {
    submission_ids.contains(&readback_id)
}

fn checked_index_type(value: IndexType, operation: &str) -> Result<(MTLIndexType, usize), Error> {
    match value.as_raw() {
        0 => Ok((MTLIndexType::UInt16, 2)),
        1 => Ok((MTLIndexType::UInt32, 4)),
        _ => Err(Error::invalid_argument(format!(
            "{operation} received an invalid index type"
        ))),
    }
}

fn validate_texture_region(
    texture: &super::Texture,
    slice: usize,
    level: usize,
    region: Region,
    operation: &str,
) -> Result<(objc2_metal::MTLOrigin, MTLSize), Error> {
    let (depth, slices, levels, _) = texture.layout();
    let end_x = region.origin.x.checked_add(region.size.width);
    let end_y = region.origin.y.checked_add(region.size.height);
    let end_z = region.origin.z.checked_add(region.size.depth);
    if region.size.width == 0
        || region.size.height == 0
        || region.size.depth == 0
        || slice >= slices
        || level >= levels
        || end_x.filter(|end| *end <= texture.width()).is_none()
        || end_y.filter(|end| *end <= texture.height()).is_none()
        || end_z.filter(|end| *end <= depth).is_none()
    {
        return Err(Error::invalid_argument(format!(
            "{operation} texture region is out of bounds"
        )));
    }
    Ok((region.origin.into(), region.size.into()))
}

fn validate_image_buffer_range(
    buffer: &super::Buffer,
    offset: usize,
    bytes_per_row: usize,
    bytes_per_image: usize,
    size: Size,
    operation: &str,
) -> Result<(), Error> {
    if bytes_per_row == 0 || (size.depth > 1 && bytes_per_image == 0) {
        return Err(Error::invalid_argument(format!(
            "{operation} requires non-zero image strides"
        )));
    }
    let rows = size
        .height
        .saturating_sub(1)
        .checked_mul(bytes_per_row)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} row stride overflows")))?;
    let images = size
        .depth
        .saturating_sub(1)
        .checked_mul(bytes_per_image)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} image stride overflows")))?;
    let minimum = rows
        .checked_add(images)
        .and_then(|value| value.checked_add(bytes_per_row))
        .ok_or_else(|| Error::invalid_argument(format!("{operation} buffer range overflows")))?;
    offset
        .checked_add(minimum)
        .filter(|end| *end <= buffer.length())
        .ok_or_else(|| {
            Error::invalid_argument(format!("{operation} buffer range is out of bounds"))
        })?;
    Ok(())
}

fn require_selector(object: &AnyObject, selector: Sel, operation: &str) -> Result<(), Error> {
    // SAFETY: NSObject protocol objects implement respondsToSelector:, and the
    // selector and bool return use the Objective-C runtime ABI.
    let supported: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if supported {
        Ok(())
    } else {
        Err(Error::unsupported(format!(
            "{operation} is unavailable on this system"
        )))
    }
}

fn required_object(
    value: Option<Retained<AnyObject>>,
    operation: &str,
) -> Result<Retained<AnyObject>, Error> {
    value.ok_or_else(|| Error::unsupported(format!("{operation} returned nil")))
}

fn validate_size(size: Size, operation: &str) -> Result<MTLSize, Error> {
    if size.width == 0 || size.height == 0 || size.depth == 0 {
        return Err(Error::invalid_argument(format!(
            "{operation} requires non-zero dimensions"
        )));
    }
    Ok(MTLSize {
        width: size.width,
        height: size.height,
        depth: size.depth,
    })
}

fn validate_viewport(viewport: Viewport) -> Result<MTLViewport, Error> {
    let values = [
        viewport.origin_x,
        viewport.origin_y,
        viewport.width,
        viewport.height,
        viewport.z_near,
        viewport.z_far,
    ];
    if !values.iter().all(|value| value.is_finite()) {
        return Err(Error::invalid_argument(
            "MTL4::RenderCommandEncoder::setViewport requires finite values",
        ));
    }
    if viewport.width < 0.0 || viewport.height < 0.0 || viewport.z_near > viewport.z_far {
        return Err(Error::invalid_argument(
            "MTL4::RenderCommandEncoder::setViewport received invalid bounds",
        ));
    }
    Ok(MTLViewport {
        originX: viewport.origin_x,
        originY: viewport.origin_y,
        width: viewport.width,
        height: viewport.height,
        znear: viewport.z_near,
        zfar: viewport.z_far,
    })
}

fn push_debug_group(object: &AnyObject, value: &str, operation: &str) -> Result<(), Error> {
    require_selector(object, sel!(pushDebugGroup:), operation)?;
    let value = NSString::from_str(value);
    // SAFETY: Selector availability is checked and the NSString remains alive
    // for the duration of the synchronous message.
    unsafe { msg_send![object, pushDebugGroup: &*value] }
    Ok(())
}

fn object_label(object: &AnyObject, operation: &str) -> Result<Option<String>, Error> {
    require_selector(object, sel!(label), operation)?;
    // SAFETY: Selector availability and NSString object return ABI are checked.
    let value: Option<Retained<NSString>> = unsafe { msg_send![object, label] };
    Ok(value.map(|value| value.to_string()))
}

fn set_object_label(object: &AnyObject, value: Option<&str>, operation: &str) -> Result<(), Error> {
    require_selector(object, sel!(setLabel:), operation)?;
    let value = value.map(NSString::from_str);
    // SAFETY: Selector availability is checked and any NSString argument
    // remains alive for the synchronous message.
    unsafe { msg_send![object, setLabel: value.as_deref()] }
    Ok(())
}

fn end_encoding(object: &AnyObject, operation: &str) -> Result<(), Error> {
    require_selector(object, sel!(endEncoding), operation)?;
    // SAFETY: Selector availability and its zero-argument void ABI are checked.
    unsafe { msg_send![object, endEncoding] }
    Ok(())
}

impl super::Device {
    /// Creates a non-cloneable Metal 4 command buffer in the available state.
    pub fn new_mtl4_command_buffer(&self) -> Result<AvailableCommandBuffer, Error> {
        require_selector(
            self.as_any_object(),
            sel!(newCommandBuffer),
            "MTL::Device::newCommandBuffer(Metal4)",
        )?;
        let inner = self.inner.newCommandBuffer().ok_or_else(|| {
            Error::unsupported("MTL::Device::newCommandBuffer(Metal4) returned nil")
        })?;
        // SAFETY: The checked SDK method declares an MTL4CommandBuffer result;
        // type erasure preserves the retained Objective-C object identity.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(AvailableCommandBuffer::from_generated(
            metal4::CommandBuffer::from_inner(inner),
            NEXT_MTL4_SUBMISSION_ID.fetch_add(1, Ordering::Relaxed),
        ))
    }

    /// Creates a reusable Metal 4 command queue.
    pub fn new_mtl4_command_queue(&self) -> Result<Metal4CommandQueue, Error> {
        require_selector(
            self.as_any_object(),
            sel!(newMTL4CommandQueue),
            "MTL::Device::newMTL4CommandQueue",
        )?;
        let inner = self
            .inner
            .newMTL4CommandQueue()
            .ok_or_else(|| Error::unsupported("MTL::Device::newMTL4CommandQueue returned nil"))?;
        // SAFETY: The checked SDK method declares an MTL4CommandQueue result;
        // type erasure preserves the retained Objective-C object identity.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Metal4CommandQueue {
            inner: metal4::CommandQueue::from_inner(inner),
        })
    }
}

impl metal4::CommandBuffer {
    /// Begins recording with the supplied allocator and optional options.
    pub(crate) fn begin(
        &self,
        allocator: &metal4::CommandAllocator,
        options: Option<&metal4::CommandBufferOptions>,
    ) -> Result<(), Error> {
        let object = self.as_inner();
        if let Some(options) = options {
            require_selector(
                object,
                sel!(beginCommandBufferWithAllocator:options:),
                "MTL4::CommandBuffer::beginCommandBuffer(options)",
            )?;
            // SAFETY: The selector is checked and both generated wrappers hold
            // live objects with the protocol identities required by Metal 4.
            unsafe {
                msg_send![object,
                    beginCommandBufferWithAllocator: allocator.as_inner(),
                    options: options.as_inner()
                ]
            }
        } else {
            require_selector(
                object,
                sel!(beginCommandBufferWithAllocator:),
                "MTL4::CommandBuffer::beginCommandBuffer",
            )?;
            // SAFETY: The selector is checked and the allocator wrapper holds a
            // live object with the required Metal 4 protocol identity.
            unsafe { msg_send![object, beginCommandBufferWithAllocator: allocator.as_inner()] }
        }
        Ok(())
    }

    /// Creates a compute encoder for the active recording scope.
    pub(crate) fn compute_encoder(&self) -> Result<metal4::ComputeCommandEncoder, Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(computeCommandEncoder),
            "MTL4::CommandBuffer::computeCommandEncoder",
        )?;
        // SAFETY: The checked selector returns an Objective-C object. Retained
        // ownership is captured before wrapping it.
        let value = unsafe { msg_send![object, computeCommandEncoder] };
        Ok(metal4::ComputeCommandEncoder::from_inner(required_object(
            value,
            "MTL4::CommandBuffer::computeCommandEncoder",
        )?))
    }

    /// Creates a machine-learning encoder for the active recording scope.
    pub(crate) fn machine_learning_encoder(
        &self,
    ) -> Result<metal4::MachineLearningCommandEncoder, Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(machineLearningCommandEncoder),
            "MTL4::CommandBuffer::machineLearningCommandEncoder",
        )?;
        // SAFETY: The checked selector returns an Objective-C object. Retained
        // ownership is captured before wrapping it.
        let value = unsafe { msg_send![object, machineLearningCommandEncoder] };
        Ok(metal4::MachineLearningCommandEncoder::from_inner(
            required_object(value, "MTL4::CommandBuffer::machineLearningCommandEncoder")?,
        ))
    }

    /// Creates a render encoder using the supplied pass descriptor.
    pub(crate) fn render_encoder(
        &self,
        descriptor: &metal4::RenderPassDescriptor,
    ) -> Result<metal4::RenderCommandEncoder, Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(renderCommandEncoderWithDescriptor:),
            "MTL4::CommandBuffer::renderCommandEncoder",
        )?;
        // SAFETY: Selector availability and descriptor identity are checked;
        // retained ownership of the returned encoder is captured.
        let value =
            unsafe { msg_send![object, renderCommandEncoderWithDescriptor: descriptor.as_inner()] };
        Ok(metal4::RenderCommandEncoder::from_inner(required_object(
            value,
            "MTL4::CommandBuffer::renderCommandEncoder",
        )?))
    }

    /// Creates a render encoder using pass descriptor options.
    pub(crate) fn render_encoder_with_options(
        &self,
        descriptor: &metal4::RenderPassDescriptor,
        options: RenderEncoderOptions,
    ) -> Result<metal4::RenderCommandEncoder, Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument(
                "invalid render encoder option bits",
            ));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(renderCommandEncoderWithDescriptor:options:),
            "MTL4::CommandBuffer::renderCommandEncoder(options)",
        )?;
        // SAFETY: Selector availability, descriptor identity, and option bits
        // are checked; retained ownership of the encoder is captured.
        let value = unsafe {
            msg_send![object,
                renderCommandEncoderWithDescriptor: descriptor.as_inner(),
                options: options.as_raw()
            ]
        };
        Ok(metal4::RenderCommandEncoder::from_inner(required_object(
            value,
            "MTL4::CommandBuffer::renderCommandEncoder(options)",
        )?))
    }

    /// Makes one residency set available to this command buffer.
    pub(crate) fn use_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(useResidencySet:),
            "MTL4::CommandBuffer::useResidencySet",
        )?;
        // SAFETY: Selector availability is checked and the generated wrapper
        // retains an object with MTLResidencySet identity.
        unsafe { msg_send![object, useResidencySet: set.as_inner()] }
        Ok(())
    }

    /// Makes a non-empty slice of residency sets available to this command buffer.
    pub(crate) fn use_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        if sets.is_empty() {
            return Err(Error::invalid_argument(
                "residency set slice must not be empty",
            ));
        }
        let objects: Vec<*const AnyObject> = sets
            .iter()
            .map(|set| set.as_inner() as *const AnyObject)
            .collect();
        let object = self.as_inner();
        require_selector(
            object,
            sel!(useResidencySets:count:),
            "MTL4::CommandBuffer::useResidencySets",
        )?;
        // SAFETY: The pointer array is derived from live retained wrappers,
        // remains allocated for the synchronous call, and count matches it.
        unsafe { msg_send![object, useResidencySets: objects.as_ptr(), count: objects.len()] }
        Ok(())
    }

    /// Pushes a command-buffer debug group.
    pub(crate) fn push_debug_group(&self, name: &str) -> Result<(), Error> {
        push_debug_group(self.as_inner(), name, "MTL4::CommandBuffer::pushDebugGroup")
    }

    /// Pops a command-buffer debug group.
    pub(crate) fn pop_debug_group(&self) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(popDebugGroup),
            "MTL4::CommandBuffer::popDebugGroup",
        )?;
        // SAFETY: Selector availability and its zero-argument void ABI are checked.
        unsafe { msg_send![object, popDebugGroup] }
        Ok(())
    }

    /// Ends the active command-buffer recording scope.
    pub(crate) fn end(&self) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(endCommandBuffer),
            "MTL4::CommandBuffer::endCommandBuffer",
        )?;
        // SAFETY: Selector availability and its zero-argument void ABI are checked.
        unsafe { msg_send![object, endCommandBuffer] }
        Ok(())
    }
}

/// A Metal 4 command buffer that has not begun recording.
pub struct AvailableCommandBuffer {
    inner: metal4::CommandBuffer,
    submission_id: u64,
}

impl AvailableCommandBuffer {
    /// Wraps a newly-created Metal 4 command buffer at the audited factory boundary.
    pub(crate) const fn from_generated(inner: metal4::CommandBuffer, submission_id: u64) -> Self {
        Self {
            inner,
            submission_id,
        }
    }

    /// Begins a single recording scope and consumes the available state.
    pub fn begin(
        self,
        allocator: &metal4::CommandAllocator,
        options: Option<&metal4::CommandBufferOptions>,
    ) -> Result<RecordingCommandBuffer, Error> {
        self.inner.begin(allocator, options)?;
        Ok(RecordingCommandBuffer {
            inner: self.inner,
            submission_id: self.submission_id,
            active_encoder: false,
        })
    }

    /// Returns the owning device when the runtime property is available.
    pub fn device(&self) -> Result<crate::Device, Error> {
        self.inner.device()?.ok_or_else(|| {
            Error::unsupported("MTL4::CommandBuffer::device unexpectedly returned nil")
        })
    }

    /// Returns the optional command-buffer label.
    pub fn label(&self) -> Result<Option<String>, Error> {
        object_label(self.inner.as_inner(), "MTL4::CommandBuffer::label")
    }

    /// Sets or clears the command-buffer label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), Error> {
        set_object_label(
            self.inner.as_inner(),
            label,
            "MTL4::CommandBuffer::setLabel",
        )
    }
}

/// A Metal 4 command buffer in its exclusive recording state.
pub struct RecordingCommandBuffer {
    pub(crate) inner: metal4::CommandBuffer,
    submission_id: u64,
    active_encoder: bool,
}

impl RecordingCommandBuffer {
    /// Encodes Metal4FX frame interpolation while this buffer is recording.
    #[doc(hidden)]
    pub fn encode_frame_interpolator(
        &mut self,
        effect: &metal4_fx::FrameInterpolator,
    ) -> Result<(), Error> {
        effect.encode_to_metal4_command_buffer(self)
    }

    /// Encodes Metal4FX spatial scaling while this buffer is recording.
    #[doc(hidden)]
    pub fn encode_spatial_scaler(
        &mut self,
        effect: &metal4_fx::SpatialScaler,
    ) -> Result<(), Error> {
        effect.encode_to_metal4_command_buffer(self)
    }

    /// Encodes Metal4FX temporal denoising and scaling while recording.
    #[doc(hidden)]
    pub fn encode_temporal_denoised_scaler(
        &mut self,
        effect: &metal4_fx::TemporalDenoisedScaler,
    ) -> Result<(), Error> {
        effect.encode_to_metal4_command_buffer(self)
    }

    /// Encodes Metal4FX temporal scaling while this buffer is recording.
    #[doc(hidden)]
    pub fn encode_temporal_scaler(
        &mut self,
        effect: &metal4_fx::TemporalScaler,
    ) -> Result<(), Error> {
        effect.encode_to_metal4_command_buffer(self)
    }

    /// Returns the owning device.
    pub fn device(&self) -> Result<crate::Device, Error> {
        self.inner.device()?.ok_or_else(|| {
            Error::unsupported("MTL4::CommandBuffer::device unexpectedly returned nil")
        })
    }

    /// Returns the optional command-buffer label.
    pub fn label(&self) -> Result<Option<String>, Error> {
        object_label(self.inner.as_inner(), "MTL4::CommandBuffer::label")
    }

    /// Sets or clears the command-buffer label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), Error> {
        set_object_label(
            self.inner.as_inner(),
            label,
            "MTL4::CommandBuffer::setLabel",
        )
    }

    /// Creates an exclusively borrowed compute encoder.
    pub fn compute_encoder(&mut self) -> Result<RecordingComputeEncoder<'_>, Error> {
        if self.active_encoder {
            return Err(Error::invalid_argument(
                "a Metal 4 encoder is already active",
            ));
        }
        let inner = self.inner.compute_encoder()?;
        require_selector(
            inner.as_inner(),
            sel!(endEncoding),
            "MTL4::ComputeCommandEncoder::endEncoding",
        )?;
        self.active_encoder = true;
        Ok(RecordingComputeEncoder {
            inner,
            recording: self,
            ended: false,
        })
    }

    /// Creates an exclusively borrowed render encoder.
    pub fn render_encoder(
        &mut self,
        descriptor: &metal4::RenderPassDescriptor,
        options: Option<RenderEncoderOptions>,
    ) -> Result<RecordingRenderEncoder<'_>, Error> {
        if self.active_encoder {
            return Err(Error::invalid_argument(
                "a Metal 4 encoder is already active",
            ));
        }
        let inner = if let Some(options) = options {
            self.inner
                .render_encoder_with_options(descriptor, options)?
        } else {
            self.inner.render_encoder(descriptor)?
        };
        require_selector(
            inner.as_inner(),
            sel!(endEncoding),
            "MTL4::RenderCommandEncoder::endEncoding",
        )?;
        self.active_encoder = true;
        Ok(RecordingRenderEncoder {
            inner,
            recording: self,
            ended: false,
        })
    }

    /// Creates an exclusively borrowed machine-learning encoder.
    pub fn machine_learning_encoder(
        &mut self,
    ) -> Result<RecordingMachineLearningEncoder<'_>, Error> {
        if self.active_encoder {
            return Err(Error::invalid_argument(
                "a Metal 4 encoder is already active",
            ));
        }
        let inner = self.inner.machine_learning_encoder()?;
        require_selector(
            inner.as_inner(),
            sel!(endEncoding),
            "MTL4::MachineLearningCommandEncoder::endEncoding",
        )?;
        self.active_encoder = true;
        Ok(RecordingMachineLearningEncoder {
            inner,
            recording: self,
            ended: false,
        })
    }

    /// Makes one residency set available to this recording.
    pub fn use_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        self.inner.use_residency_set(set)
    }

    /// Makes a non-empty residency-set slice available to this recording.
    pub fn use_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        self.inner.use_residency_sets(sets)
    }

    /// Pushes a command-buffer debug group.
    pub fn push_debug_group(&self, name: &str) -> Result<(), Error> {
        self.inner.push_debug_group(name)
    }

    /// Pops a command-buffer debug group.
    pub fn pop_debug_group(&self) -> Result<(), Error> {
        self.inner.pop_debug_group()
    }

    /// Encodes a counter-heap resolve into an internal CPU-visible staging buffer.
    ///
    /// Consuming the heap prevents CPU invalidation or reuse until the matching
    /// completed submission resolves the returned handle.
    pub fn resolve_counter_heap(
        &self,
        heap: super::TimestampCounterHeap,
        range: std::ops::Range<usize>,
    ) -> Result<super::mtl4_counters::CounterReadback, Error> {
        let range = super::mtl4_counters::checked_range(
            range,
            heap.count(),
            "MTL4::CommandBuffer::resolveCounterHeap",
        )?;
        if range.length == 0 {
            return Err(Error::invalid_argument(
                "counter readback range must not be empty",
            ));
        }
        let byte_length = range
            .length
            .checked_mul(std::mem::size_of::<u64>())
            .ok_or_else(|| Error::invalid_argument("counter readback size overflows"))?;
        let staging = self
            .device()?
            .new_buffer(byte_length, super::ResourceOptions::SHARED)?;
        let buffer_range = MTL4BufferRange {
            bufferAddress: staging.inner.gpuAddress(),
            length: byte_length as u64,
        };
        let operation = "MTL4::CommandBuffer::resolveCounterHeap";
        require_selector(
            self.inner.as_inner(),
            sel!(resolveCounterHeap:withRange:intoBuffer:waitFence:updateFence:),
            operation,
        )?;
        // SAFETY: The selector and heap identity are checked. The heap range is
        // bounded, and MTL4BufferRange is derived from an owned staging buffer
        // large enough for every tightly packed timestamp entry.
        unsafe {
            msg_send![self.inner.as_inner(), resolveCounterHeap: heap.as_any_object(), withRange: range, intoBuffer: buffer_range, waitFence: Option::<&AnyObject>::None, updateFence: Option::<&AnyObject>::None]
        }
        Ok(super::mtl4_counters::CounterReadback {
            heap,
            buffer: staging,
            submission_id: self.submission_id,
            count: range.length,
        })
    }

    /// Writes a command-buffer boundary timestamp into a checked heap entry.
    pub fn write_timestamp_into_heap(
        &self,
        heap: &super::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::CommandBuffer::writeTimestampIntoHeap";
        if index >= heap.count() {
            return Err(Error::invalid_argument(format!(
                "{operation} index is out of bounds"
            )));
        }
        require_selector(
            self.inner.as_inner(),
            sel!(writeTimestampIntoHeap:atIndex:),
            operation,
        )?;
        // SAFETY: Selector and counter-heap identity are checked and the index
        // is bounded by the immutable heap count.
        unsafe {
            msg_send![self.inner.as_inner(), writeTimestampIntoHeap: heap.as_any_object(), atIndex: index]
        }
        Ok(())
    }

    /// Ends recording and consumes the recording state.
    pub fn end(self) -> Result<EndedCommandBuffer, Error> {
        if self.active_encoder {
            return Err(Error::invalid_argument(
                "cannot end a Metal 4 command buffer while an encoder is active",
            ));
        }
        self.inner.end()?;
        Ok(EndedCommandBuffer {
            inner: self.inner,
            submission_id: self.submission_id,
        })
    }
}

/// A Metal 4 command buffer that is ended and ready for one submission.
pub struct EndedCommandBuffer {
    inner: metal4::CommandBuffer,
    submission_id: u64,
}

/// An exclusively borrowed Metal 4 compute encoder.
pub struct RecordingComputeEncoder<'a> {
    inner: metal4::ComputeCommandEncoder,
    recording: &'a mut RecordingCommandBuffer,
    ended: bool,
}

impl RecordingComputeEncoder<'_> {
    /// Dispatches a grid expressed in threadgroups.
    pub fn dispatch_threadgroups(&self, groups: Size, threads: Size) -> Result<(), Error> {
        self.inner.dispatch_threadgroups(groups, threads)
    }

    /// Dispatches a grid expressed as individual threads.
    pub fn dispatch_threads(&self, grid: Size, threads: Size) -> Result<(), Error> {
        self.inner.dispatch_threads(grid, threads)
    }

    /// Dispatches threadgroups using arguments stored in a checked buffer range.
    pub fn dispatch_threadgroups_indirect(
        &self,
        buffer: &super::Buffer,
        offset: usize,
        threads: Size,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::dispatchThreadgroups(indirect)";
        let address = checked_buffer_address(buffer, offset, 12, 4, operation)?;
        let threads = validate_size(threads, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(dispatchThreadgroupsWithIndirectBuffer:threadsPerThreadgroup:),
            operation,
        )?;
        // SAFETY: The selector is checked; the address is derived from a live
        // MTLBuffer and validated for the documented indirect argument layout.
        unsafe {
            msg_send![self.inner.as_inner(), dispatchThreadgroupsWithIndirectBuffer: address, threadsPerThreadgroup: threads]
        }
        Ok(())
    }

    /// Dispatches threads using arguments stored in a checked buffer range.
    pub fn dispatch_threads_indirect(
        &self,
        buffer: &super::Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::dispatchThreads(indirect)";
        let address = checked_buffer_address(buffer, offset, 12, 4, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(dispatchThreadsWithIndirectBuffer:),
            operation,
        )?;
        // SAFETY: The checked address is derived from a retained MTLBuffer.
        unsafe { msg_send![self.inner.as_inner(), dispatchThreadsWithIndirectBuffer: address] }
        Ok(())
    }

    /// Executes a checked CPU-selected range of an indirect command buffer.
    pub fn execute_commands(
        &self,
        commands: &metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::executeCommandsInBuffer(range)";
        let range = checked_range(commands.size()?, range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(executeCommandsInBuffer:withRange:),
            operation,
        )?;
        // SAFETY: Selector and object identity are checked, and NSRange is bounded.
        unsafe {
            msg_send![self.inner.as_inner(), executeCommandsInBuffer: commands.as_inner(), withRange: range]
        }
        Ok(())
    }

    /// Executes an indirect-command range read from a checked buffer offset.
    pub fn execute_commands_indirect(
        &self,
        commands: &metal::IndirectCommandBuffer,
        range_buffer: &super::Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::executeCommandsInBuffer(indirect)";
        let address = checked_buffer_address(range_buffer, offset, 16, 4, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(executeCommandsInBuffer:indirectBuffer:),
            operation,
        )?;
        // SAFETY: Selector and object identity are checked; the indirect address
        // comes from a retained buffer and covers the execution-range payload.
        unsafe {
            msg_send![self.inner.as_inner(), executeCommandsInBuffer: commands.as_inner(), indirectBuffer: address]
        }
        Ok(())
    }

    /// Fills an in-bounds buffer range with one byte value.
    pub fn fill_buffer(
        &self,
        buffer: &super::Buffer,
        range: std::ops::Range<usize>,
        value: u8,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::fillBuffer";
        let range = checked_range(buffer.length(), range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(fillBuffer:range:value:),
            operation,
        )?;
        // SAFETY: The selector is checked and the range is bounded by the buffer.
        unsafe {
            msg_send![self.inner.as_inner(), fillBuffer: buffer.as_any_object(), range: range, value: value]
        }
        Ok(())
    }

    /// Copies one checked buffer range into another buffer.
    pub fn copy_buffer(
        &self,
        source: &super::Buffer,
        source_range: std::ops::Range<usize>,
        destination: &super::Buffer,
        destination_offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromBuffer(toBuffer)";
        let source_range = checked_range(source.length(), source_range, operation)?;
        destination_offset
            .checked_add(source_range.length)
            .filter(|end| *end <= destination.length())
            .ok_or_else(|| {
                Error::invalid_argument(format!("{operation} destination range is out of bounds"))
            })?;
        require_selector(
            self.inner.as_inner(),
            sel!(copyFromBuffer:sourceOffset:toBuffer:destinationOffset:size:),
            operation,
        )?;
        // SAFETY: The selector is checked and both byte ranges are bounded.
        unsafe {
            msg_send![self.inner.as_inner(), copyFromBuffer: source.as_any_object(), sourceOffset: source_range.location, toBuffer: destination.as_any_object(), destinationOffset: destination_offset, size: source_range.length]
        }
        Ok(())
    }

    /// Generates all mip levels for a texture.
    pub fn generate_mipmaps(&self, texture: &super::Texture) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::generateMipmapsForTexture";
        require_selector(
            self.inner.as_inner(),
            sel!(generateMipmapsForTexture:),
            operation,
        )?;
        // SAFETY: Selector availability and MTLTexture identity are checked.
        unsafe {
            msg_send![self.inner.as_inner(), generateMipmapsForTexture: texture.as_any_object()]
        }
        Ok(())
    }

    /// Copies all compatible subresources between two textures.
    pub fn copy_texture(
        &self,
        source: &super::Texture,
        destination: &super::Texture,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromTexture(toTexture)";
        require_selector(
            self.inner.as_inner(),
            sel!(copyFromTexture:toTexture:),
            operation,
        )?;
        // SAFETY: Selector and both retained MTLTexture identities are checked.
        unsafe {
            msg_send![self.inner.as_inner(), copyFromTexture: source.as_any_object(), toTexture: destination.as_any_object()]
        }
        Ok(())
    }

    /// Copies checked texture slice and mip-level spans.
    pub fn copy_texture_levels(
        &self,
        source: &super::Texture,
        source_span: TextureSubresourceSpan,
        destination: &super::Texture,
        destination_slice: usize,
        destination_level: usize,
    ) -> Result<(), Error> {
        let (_, source_slices, source_levels, _) = source.layout();
        let (_, destination_slices, destination_levels, _) = destination.layout();
        if source_span.slice_count == 0
            || source_span.level_count == 0
            || source_span
                .slice
                .checked_add(source_span.slice_count)
                .filter(|end| *end <= source_slices)
                .is_none()
            || destination_slice
                .checked_add(source_span.slice_count)
                .filter(|end| *end <= destination_slices)
                .is_none()
            || source_span
                .level
                .checked_add(source_span.level_count)
                .filter(|end| *end <= source_levels)
                .is_none()
            || destination_level
                .checked_add(source_span.level_count)
                .filter(|end| *end <= destination_levels)
                .is_none()
        {
            return Err(Error::invalid_argument(
                "texture copy slice or level span is out of bounds",
            ));
        }
        let operation = "MTL4::ComputeCommandEncoder::copyFromTexture(slices)";
        require_selector(
            self.inner.as_inner(),
            sel!(copyFromTexture:sourceSlice:sourceLevel:toTexture:destinationSlice:destinationLevel:sliceCount:levelCount:),
            operation,
        )?;
        // SAFETY: Selector/resource identities are checked and all spans are bounded.
        unsafe {
            msg_send![self.inner.as_inner(), copyFromTexture: source.as_any_object(), sourceSlice: source_span.slice, sourceLevel: source_span.level, toTexture: destination.as_any_object(), destinationSlice: destination_slice, destinationLevel: destination_level, sliceCount: source_span.slice_count, levelCount: source_span.level_count]
        }
        Ok(())
    }

    /// Copies a checked texture region into another texture.
    pub fn copy_texture_region(
        &self,
        source: &super::Texture,
        source_region: TextureRegionSelection,
        destination: &super::Texture,
        destination_origin: TextureOriginSelection,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromTexture(region)";
        let (source_origin, source_size) = validate_texture_region(
            source,
            source_region.slice,
            source_region.level,
            source_region.region,
            operation,
        )?;
        let destination_region = Region {
            origin: destination_origin.origin,
            size: source_region.region.size,
        };
        validate_texture_region(
            destination,
            destination_origin.slice,
            destination_origin.level,
            destination_region,
            operation,
        )?;
        require_selector(
            self.inner.as_inner(),
            sel!(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:),
            operation,
        )?;
        // SAFETY: Selector/resource identities and both texture regions are checked.
        unsafe {
            msg_send![self.inner.as_inner(), copyFromTexture: source.as_any_object(), sourceSlice: source_region.slice, sourceLevel: source_region.level, sourceOrigin: source_origin, sourceSize: source_size, toTexture: destination.as_any_object(), destinationSlice: destination_origin.slice, destinationLevel: destination_origin.level, destinationOrigin: objc2_metal::MTLOrigin::from(destination_origin.origin)]
        }
        Ok(())
    }

    /// Copies a checked texture region into a checked buffer layout.
    pub fn copy_texture_to_buffer(
        &self,
        source: &super::Texture,
        source_region: TextureRegionSelection,
        destination: &super::Buffer,
        layout: BufferImageLayout,
        options: Option<BlitOption>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromTexture(toBuffer)";
        let (origin, size) = validate_texture_region(
            source,
            source_region.slice,
            source_region.level,
            source_region.region,
            operation,
        )?;
        validate_image_buffer_range(
            destination,
            layout.offset,
            layout.bytes_per_row,
            layout.bytes_per_image,
            source_region.region.size,
            operation,
        )?;
        if let Some(options) = options {
            if !options.is_valid() {
                return Err(Error::invalid_argument("invalid blit option bits"));
            }
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:options:),
                operation,
            )?;
            // SAFETY: Selector, resources, region, buffer layout, and option bits are checked.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromTexture: source.as_any_object(), sourceSlice: source_region.slice, sourceLevel: source_region.level, sourceOrigin: origin, sourceSize: size, toBuffer: destination.as_any_object(), destinationOffset: layout.offset, destinationBytesPerRow: layout.bytes_per_row, destinationBytesPerImage: layout.bytes_per_image, options: MTLBlitOption(options.as_raw())]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromTexture:sourceSlice:sourceLevel:sourceOrigin:sourceSize:toBuffer:destinationOffset:destinationBytesPerRow:destinationBytesPerImage:),
                operation,
            )?;
            // SAFETY: Selector, resources, region, and buffer layout are checked.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromTexture: source.as_any_object(), sourceSlice: source_region.slice, sourceLevel: source_region.level, sourceOrigin: origin, sourceSize: size, toBuffer: destination.as_any_object(), destinationOffset: layout.offset, destinationBytesPerRow: layout.bytes_per_row, destinationBytesPerImage: layout.bytes_per_image]
            }
        }
        Ok(())
    }

    /// Copies a checked buffer image layout into a checked texture region.
    pub fn copy_buffer_to_texture(
        &self,
        source: &super::Buffer,
        layout: BufferImageLayout,
        source_size: Size,
        destination: &super::Texture,
        destination_origin: TextureOriginSelection,
        options: Option<BlitOption>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromBuffer(toTexture)";
        validate_size(source_size, operation)?;
        validate_image_buffer_range(
            source,
            layout.offset,
            layout.bytes_per_row,
            layout.bytes_per_image,
            source_size,
            operation,
        )?;
        validate_texture_region(
            destination,
            destination_origin.slice,
            destination_origin.level,
            Region {
                origin: destination_origin.origin,
                size: source_size,
            },
            operation,
        )?;
        let size: MTLSize = source_size.into();
        let origin: objc2_metal::MTLOrigin = destination_origin.origin.into();
        if let Some(options) = options {
            if !options.is_valid() {
                return Err(Error::invalid_argument("invalid blit option bits"));
            }
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:options:),
                operation,
            )?;
            // SAFETY: Selector, resources, layouts, destination region, and option bits are checked.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromBuffer: source.as_any_object(), sourceOffset: layout.offset, sourceBytesPerRow: layout.bytes_per_row, sourceBytesPerImage: layout.bytes_per_image, sourceSize: size, toTexture: destination.as_any_object(), destinationSlice: destination_origin.slice, destinationLevel: destination_origin.level, destinationOrigin: origin, options: MTLBlitOption(options.as_raw())]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromBuffer:sourceOffset:sourceBytesPerRow:sourceBytesPerImage:sourceSize:toTexture:destinationSlice:destinationLevel:destinationOrigin:),
                operation,
            )?;
            // SAFETY: Selector, resources, layouts, and destination region are checked.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromBuffer: source.as_any_object(), sourceOffset: layout.offset, sourceBytesPerRow: layout.bytes_per_row, sourceBytesPerImage: layout.bytes_per_image, sourceSize: size, toTexture: destination.as_any_object(), destinationSlice: destination_origin.slice, destinationLevel: destination_origin.level, destinationOrigin: origin]
            }
        }
        Ok(())
    }

    /// Optimizes all texture contents for GPU access.
    pub fn optimize_texture_for_gpu(&self, texture: &super::Texture) -> Result<(), Error> {
        self.optimize_texture(texture, None, true)
    }

    /// Optimizes one checked texture slice/level for GPU access.
    pub fn optimize_texture_level_for_gpu(
        &self,
        texture: &super::Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        self.optimize_texture(texture, Some((slice, level)), true)
    }

    /// Optimizes all texture contents for CPU access.
    pub fn optimize_texture_for_cpu(&self, texture: &super::Texture) -> Result<(), Error> {
        self.optimize_texture(texture, None, false)
    }

    /// Optimizes one checked texture slice/level for CPU access.
    pub fn optimize_texture_level_for_cpu(
        &self,
        texture: &super::Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), Error> {
        self.optimize_texture(texture, Some((slice, level)), false)
    }

    fn optimize_texture(
        &self,
        texture: &super::Texture,
        subresource: Option<(usize, usize)>,
        gpu: bool,
    ) -> Result<(), Error> {
        let operation = if gpu {
            "MTL4::ComputeCommandEncoder::optimizeContentsForGPUAccess"
        } else {
            "MTL4::ComputeCommandEncoder::optimizeContentsForCPUAccess"
        };
        if let Some((slice, level)) = subresource {
            let (_, slices, levels, _) = texture.layout();
            if slice >= slices || level >= levels {
                return Err(Error::invalid_argument(format!(
                    "{operation} subresource is out of bounds"
                )));
            }
            if gpu {
                require_selector(
                    self.inner.as_inner(),
                    sel!(optimizeContentsForGPUAccess:slice:level:),
                    operation,
                )?;
                // SAFETY: Selector availability, texture identity, and the
                // requested slice and mip level are checked above.
                unsafe {
                    msg_send![self.inner.as_inner(), optimizeContentsForGPUAccess: texture.as_any_object(), slice: slice, level: level]
                }
            } else {
                require_selector(
                    self.inner.as_inner(),
                    sel!(optimizeContentsForCPUAccess:slice:level:),
                    operation,
                )?;
                // SAFETY: Selector availability, texture identity, and the
                // requested slice and mip level are checked above.
                unsafe {
                    msg_send![self.inner.as_inner(), optimizeContentsForCPUAccess: texture.as_any_object(), slice: slice, level: level]
                }
            }
        } else if gpu {
            require_selector(
                self.inner.as_inner(),
                sel!(optimizeContentsForGPUAccess:),
                operation,
            )?;
            // SAFETY: Selector availability and retained texture identity are checked.
            unsafe {
                msg_send![self.inner.as_inner(), optimizeContentsForGPUAccess: texture.as_any_object()]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(optimizeContentsForCPUAccess:),
                operation,
            )?;
            // SAFETY: Selector availability and retained texture identity are checked.
            unsafe {
                msg_send![self.inner.as_inner(), optimizeContentsForCPUAccess: texture.as_any_object()]
            }
        }
        Ok(())
    }

    /// Resets a checked range of an indirect command buffer.
    pub fn reset_commands(
        &self,
        commands: &metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::resetCommandsInBuffer";
        let range = checked_range(commands.size()?, range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(resetCommandsInBuffer:withRange:),
            operation,
        )?;
        // SAFETY: Selector, object identity, and range have been checked.
        unsafe {
            msg_send![self.inner.as_inner(), resetCommandsInBuffer: commands.as_inner(), withRange: range]
        }
        Ok(())
    }

    /// Optimizes a checked range of an indirect command buffer.
    pub fn optimize_indirect_commands(
        &self,
        commands: &metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::optimizeIndirectCommandBuffer";
        let range = checked_range(commands.size()?, range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(optimizeIndirectCommandBuffer:withRange:),
            operation,
        )?;
        // SAFETY: Selector, object identity, and range have been checked.
        unsafe {
            msg_send![self.inner.as_inner(), optimizeIndirectCommandBuffer: commands.as_inner(), withRange: range]
        }
        Ok(())
    }

    /// Copies a checked indirect-command range into a destination index.
    pub fn copy_indirect_commands(
        &self,
        source: &metal::IndirectCommandBuffer,
        source_range: std::ops::Range<usize>,
        destination: &metal::IndirectCommandBuffer,
        destination_index: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyIndirectCommandBuffer";
        let range = checked_range(source.size()?, source_range, operation)?;
        let destination_size = destination.size()?;
        if range.length == 0
            || destination_index
                .checked_add(range.length)
                .filter(|end| *end <= destination_size)
                .is_none()
        {
            return Err(Error::invalid_argument(format!(
                "{operation} destination span is out of bounds"
            )));
        }
        require_selector(
            self.inner.as_inner(),
            sel!(copyIndirectCommandBuffer:sourceRange:destination:destinationIndex:),
            operation,
        )?;
        // SAFETY: Selector/object identities are checked and both command spans are bounded.
        unsafe {
            msg_send![self.inner.as_inner(), copyIndirectCommandBuffer: source.as_inner(), sourceRange: range, destination: destination.as_inner(), destinationIndex: destination_index]
        }
        Ok(())
    }

    /// Writes a timestamp into a checked counter-heap entry.
    pub fn write_timestamp(
        &self,
        granularity: TimestampGranularity,
        heap: &super::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::writeTimestamp";
        if !granularity.is_valid() || index >= heap.count() {
            return Err(Error::invalid_argument(format!(
                "{operation} received invalid granularity or index"
            )));
        }
        require_selector(
            self.inner.as_inner(),
            sel!(writeTimestampWithGranularity:intoHeap:atIndex:),
            operation,
        )?;
        // SAFETY: Selector, enum value, heap identity, and index are checked.
        unsafe {
            msg_send![self.inner.as_inner(), writeTimestampWithGranularity: granularity.as_raw(), intoHeap: heap.as_any_object(), atIndex: index]
        }
        Ok(())
    }

    /// Builds an acceleration structure using a checked scratch-buffer range.
    pub fn build_acceleration_structure(
        &self,
        acceleration_structure: &metal::AccelerationStructure,
        descriptor: &metal4::AccelerationStructureDescriptor,
        scratch: &super::Buffer,
        scratch_range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::buildAccelerationStructure";
        let scratch = checked_mtl4_buffer_range(scratch, scratch_range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(buildAccelerationStructure:descriptor:scratchBuffer:),
            operation,
        )?;
        // SAFETY: Selector/object identities are checked and scratch range is derived from a retained buffer.
        unsafe {
            msg_send![self.inner.as_inner(), buildAccelerationStructure: acceleration_structure.as_inner(), descriptor: descriptor.as_inner(), scratchBuffer: scratch]
        }
        Ok(())
    }

    /// Refits an acceleration structure using a checked scratch-buffer range.
    pub fn refit_acceleration_structure(
        &self,
        source: &metal::AccelerationStructure,
        descriptor: &metal4::AccelerationStructureDescriptor,
        destination: Option<&metal::AccelerationStructure>,
        scratch: &super::Buffer,
        scratch_range: std::ops::Range<usize>,
        options: Option<AccelerationStructureRefitOptions>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::refitAccelerationStructure";
        let scratch = checked_mtl4_buffer_range(scratch, scratch_range, operation)?;
        if let Some(options) = options {
            if !options.is_valid() {
                return Err(Error::invalid_argument(
                    "invalid acceleration-structure refit option bits",
                ));
            }
            require_selector(
                self.inner.as_inner(),
                sel!(refitAccelerationStructure:descriptor:destination:scratchBuffer:options:),
                operation,
            )?;
            // SAFETY: Selector/object identities, option bits, and scratch range are checked.
            unsafe {
                msg_send![self.inner.as_inner(), refitAccelerationStructure: source.as_inner(), descriptor: descriptor.as_inner(), destination: destination.map(metal::AccelerationStructure::as_inner), scratchBuffer: scratch, options: options.as_raw()]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(refitAccelerationStructure:descriptor:destination:scratchBuffer:),
                operation,
            )?;
            // SAFETY: Selector/object identities and scratch range are checked.
            unsafe {
                msg_send![self.inner.as_inner(), refitAccelerationStructure: source.as_inner(), descriptor: descriptor.as_inner(), destination: destination.map(metal::AccelerationStructure::as_inner), scratchBuffer: scratch]
            }
        }
        Ok(())
    }

    /// Copies one acceleration structure into another.
    pub fn copy_acceleration_structure(
        &self,
        source: &metal::AccelerationStructure,
        destination: &metal::AccelerationStructure,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyAccelerationStructure";
        require_selector(
            self.inner.as_inner(),
            sel!(copyAccelerationStructure:toAccelerationStructure:),
            operation,
        )?;
        // SAFETY: Selector availability and both retained object identities are checked.
        unsafe {
            msg_send![self.inner.as_inner(), copyAccelerationStructure: source.as_inner(), toAccelerationStructure: destination.as_inner()]
        }
        Ok(())
    }

    /// Copies and compacts one acceleration structure into another.
    pub fn copy_and_compact_acceleration_structure(
        &self,
        source: &metal::AccelerationStructure,
        destination: &metal::AccelerationStructure,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyAndCompactAccelerationStructure";
        require_selector(
            self.inner.as_inner(),
            sel!(copyAndCompactAccelerationStructure:toAccelerationStructure:),
            operation,
        )?;
        // SAFETY: Selector availability and both retained object identities are checked.
        unsafe {
            msg_send![self.inner.as_inner(), copyAndCompactAccelerationStructure: source.as_inner(), toAccelerationStructure: destination.as_inner()]
        }
        Ok(())
    }

    /// Writes the compacted size into a checked buffer range.
    pub fn write_compacted_acceleration_structure_size(
        &self,
        acceleration_structure: &metal::AccelerationStructure,
        destination: &super::Buffer,
        destination_range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::writeCompactedAccelerationStructureSize";
        let destination = checked_mtl4_buffer_range(destination, destination_range, operation)?;
        if destination.length < std::mem::size_of::<u64>() as u64 {
            return Err(Error::invalid_argument(
                "compacted-size destination must contain at least eight bytes",
            ));
        }
        require_selector(
            self.inner.as_inner(),
            sel!(writeCompactedAccelerationStructureSize:toBuffer:),
            operation,
        )?;
        // SAFETY: Selector/object identity and typed destination range are checked.
        unsafe {
            msg_send![self.inner.as_inner(), writeCompactedAccelerationStructureSize: acceleration_structure.as_inner(), toBuffer: destination]
        }
        Ok(())
    }

    /// Copies a checked tensor region, optionally selecting auxiliary planes.
    pub fn copy_tensor(
        &self,
        source: &metal::Tensor,
        destination: &metal::Tensor,
        region: TensorCopyRegion<'_>,
        planes: Option<(TensorPlaneType, TensorPlaneType)>,
    ) -> Result<(), Error> {
        let operation = "MTL4::ComputeCommandEncoder::copyFromTensor";
        let source_origin_values = region.source_origin.as_slice();
        let source_dimension_values = region.source_dimensions.as_slice();
        let destination_origin_values = region.destination_origin.as_slice();
        let destination_dimension_values = region.destination_dimensions.as_slice();
        if source_origin_values.len() != source_dimension_values.len()
            || destination_origin_values.len() != destination_dimension_values.len()
            || source_dimension_values != destination_dimension_values
            || source_dimension_values.contains(&0)
            || source_origin_values
                .iter()
                .zip(source_dimension_values)
                .any(|(origin, size)| origin.checked_add(*size).is_none())
            || destination_origin_values
                .iter()
                .zip(destination_dimension_values)
                .any(|(origin, size)| origin.checked_add(*size).is_none())
        {
            return Err(Error::invalid_argument(format!(
                "{operation} received mismatched or overflowing extents"
            )));
        }
        let source_bounds_object = source
            .dimensions()?
            .ok_or_else(|| Error::unsupported("source tensor dimensions returned nil"))?;
        let destination_bounds_object = destination
            .dimensions()?
            .ok_or_else(|| Error::unsupported("destination tensor dimensions returned nil"))?;
        let source_bounds = super::mtl_tensor_safe::read_extents(
            source_bounds_object.as_inner(),
            "source tensor dimensions",
        )?;
        let destination_bounds = super::mtl_tensor_safe::read_extents(
            destination_bounds_object.as_inner(),
            "destination tensor dimensions",
        )?;
        let in_bounds = |origins: &[usize], dimensions: &[usize], bounds: &[usize]| {
            origins.len() == bounds.len()
                && origins
                    .iter()
                    .zip(dimensions)
                    .zip(bounds)
                    .all(|((origin, size), bound)| {
                        origin.checked_add(*size).is_some_and(|end| end <= *bound)
                    })
        };
        if !in_bounds(
            source_origin_values,
            source_dimension_values,
            source_bounds.as_slice(),
        ) || !in_bounds(
            destination_origin_values,
            destination_dimension_values,
            destination_bounds.as_slice(),
        ) {
            return Err(Error::invalid_argument(format!(
                "{operation} tensor region is out of bounds"
            )));
        }
        let source_origin = super::mtl_tensor_safe::ObjectiveCTensorExtents::from_values(
            region.source_origin.as_slice(),
        )?;
        let source_dimensions = super::mtl_tensor_safe::ObjectiveCTensorExtents::from_values(
            region.source_dimensions.as_slice(),
        )?;
        let destination_origin = super::mtl_tensor_safe::ObjectiveCTensorExtents::from_values(
            region.destination_origin.as_slice(),
        )?;
        let destination_dimensions = super::mtl_tensor_safe::ObjectiveCTensorExtents::from_values(
            region.destination_dimensions.as_slice(),
        )?;
        if let Some((source_plane, destination_plane)) = planes {
            if !source_plane.is_valid() || !destination_plane.is_valid() {
                return Err(Error::invalid_argument(
                    "tensor copy received invalid plane values",
                ));
            }
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromTensor:sourceOrigin:sourceDimensions:sourcePlane:toTensor:destinationOrigin:destinationDimensions:destinationPlane:),
                operation,
            )?;
            // SAFETY: Selector/tensor identities and both plane values are checked;
            // all extent objects are owned for the synchronous invocation.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromTensor: source.as_inner(), sourceOrigin: &*source_origin.inner, sourceDimensions: &*source_dimensions.inner, sourcePlane: source_plane.as_raw(), toTensor: destination.as_inner(), destinationOrigin: &*destination_origin.inner, destinationDimensions: &*destination_dimensions.inner, destinationPlane: destination_plane.as_raw()]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(copyFromTensor:sourceOrigin:sourceDimensions:toTensor:destinationOrigin:destinationDimensions:),
                operation,
            )?;
            // SAFETY: Selector/tensor identities are checked and all extent
            // objects are owned for the synchronous invocation.
            unsafe {
                msg_send![self.inner.as_inner(), copyFromTensor: source.as_inner(), sourceOrigin: &*source_origin.inner, sourceDimensions: &*source_dimensions.inner, toTensor: destination.as_inner(), destinationOrigin: &*destination_origin.inner, destinationDimensions: &*destination_dimensions.inner]
            }
        }
        Ok(())
    }

    /// Binds the optional compute argument table.
    pub fn set_argument_table(
        &self,
        table: Option<&super::Mtl4ArgumentTable>,
    ) -> Result<(), Error> {
        self.inner
            .set_argument_table(table.map(super::Mtl4ArgumentTable::as_generated))
    }

    /// Binds the compute pipeline state.
    pub fn set_compute_pipeline_state(
        &self,
        state: &super::ComputePipelineState,
    ) -> Result<(), Error> {
        self.inner.set_compute_pipeline_state(state)
    }

    /// Configures compute threadgroup memory.
    pub fn set_threadgroup_memory_length(&self, length: usize, index: usize) -> Result<(), Error> {
        self.inner.set_threadgroup_memory_length(length, index)
    }

    /// Sets imageblock dimensions.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner.set_imageblock_size(width, height)
    }

    /// Returns the encoder stage mask.
    pub fn stages(&self) -> Result<Stages, Error> {
        self.inner.stages()
    }

    /// Explicitly ends encoding and releases the exclusive recording borrow.
    pub fn end(mut self) -> Result<(), Error> {
        self.inner.finish()?;
        self.ended = true;
        self.recording.active_encoder = false;
        Ok(())
    }
}

impl Drop for RecordingComputeEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.inner.finish();
            self.recording.active_encoder = false;
        }
    }
}

/// An exclusively borrowed Metal 4 render encoder.
pub struct RecordingRenderEncoder<'a> {
    inner: metal4::RenderCommandEncoder,
    recording: &'a mut RecordingCommandBuffer,
    ended: bool,
}

impl RecordingRenderEncoder<'_> {
    /// Returns the tile width.
    pub fn tile_width(&self) -> Result<usize, Error> {
        self.inner.tile_width()
    }

    /// Returns the tile height.
    pub fn tile_height(&self) -> Result<usize, Error> {
        self.inner.tile_height()
    }

    /// Dispatches one tile grid with validated dimensions.
    pub fn dispatch_threads_per_tile(&self, threads: Size) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::dispatchThreadsPerTile";
        let threads = validate_size(threads, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(dispatchThreadsPerTile:),
            operation,
        )?;
        // SAFETY: Selector availability and the MTLSize ABI are checked.
        unsafe { msg_send![self.inner.as_inner(), dispatchThreadsPerTile: threads] }
        Ok(())
    }

    /// Draws indexed primitives from a checked typed buffer range.
    pub fn draw_indexed_primitives(
        &self,
        primitive: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        index_buffer: &super::Buffer,
        index_range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.draw_indexed_primitives_instanced(
            primitive,
            index_count,
            index_type,
            index_buffer,
            index_range,
            1,
        )
    }

    /// Draws instanced indexed primitives from a checked typed buffer range.
    pub fn draw_indexed_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        index_buffer: &super::Buffer,
        index_range: std::ops::Range<usize>,
        instance_count: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawIndexedPrimitives";
        let (index_type, stride) = checked_index_type(index_type, operation)?;
        let range = checked_range(index_buffer.length(), index_range, operation)?;
        let required = index_count
            .checked_mul(stride)
            .ok_or_else(|| Error::invalid_argument(format!("{operation} index size overflows")))?;
        if range.length < required
            || !range.length.is_multiple_of(stride)
            || index_count == 0
            || instance_count == 0
        {
            return Err(Error::invalid_argument(format!(
                "{operation} received an invalid index range or count"
            )));
        }
        let address = checked_buffer_address(
            index_buffer,
            range.location,
            range.length,
            stride,
            operation,
        )?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferLength:instanceCount:),
            operation,
        )?;
        // SAFETY: The selector is checked; the address and length derive from a
        // retained buffer and satisfy the index-type alignment and size rules.
        unsafe {
            msg_send![self.inner.as_inner(), drawIndexedPrimitives: primitive.as_raw(), indexCount: index_count, indexType: index_type, indexBuffer: address, indexBufferLength: range.length, instanceCount: instance_count]
        }
        Ok(())
    }

    /// Draws instanced indexed primitives with explicit base values.
    pub fn draw_indexed_primitives_base(
        &self,
        primitive: PrimitiveType,
        index_buffer: &super::Buffer,
        draw: IndexedDraw,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawIndexedPrimitives(base)";
        let (index_type, stride) = checked_index_type(draw.index_type, operation)?;
        let range = checked_range(index_buffer.length(), draw.index_range, operation)?;
        let required = draw
            .index_count
            .checked_mul(stride)
            .ok_or_else(|| Error::invalid_argument(format!("{operation} index size overflows")))?;
        if range.length < required
            || !range.length.is_multiple_of(stride)
            || draw.index_count == 0
            || draw.instance_count == 0
        {
            return Err(Error::invalid_argument(format!(
                "{operation} received an invalid index range or count"
            )));
        }
        let address = checked_buffer_address(
            index_buffer,
            range.location,
            range.length,
            stride,
            operation,
        )?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferLength:instanceCount:baseVertex:baseInstance:),
            operation,
        )?;
        // SAFETY: Selector and all documented buffer invariants are checked.
        unsafe {
            msg_send![self.inner.as_inner(), drawIndexedPrimitives: primitive.as_raw(), indexCount: draw.index_count, indexType: index_type, indexBuffer: address, indexBufferLength: range.length, instanceCount: draw.instance_count, baseVertex: draw.base_vertex, baseInstance: draw.base_instance]
        }
        Ok(())
    }

    /// Issues a non-indexed indirect draw from a checked buffer offset.
    pub fn draw_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        buffer: &super::Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawPrimitives(indirect)";
        let address = checked_buffer_address(buffer, offset, 16, 4, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawPrimitives:indirectBuffer:),
            operation,
        )?;
        // SAFETY: Selector is checked and address is a validated live-buffer offset.
        unsafe {
            msg_send![self.inner.as_inner(), drawPrimitives: primitive.as_raw(), indirectBuffer: address]
        }
        Ok(())
    }

    /// Issues an indexed indirect draw using two checked buffer ranges.
    pub fn draw_indexed_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        index_type: IndexType,
        index_buffer: &super::Buffer,
        index_range: std::ops::Range<usize>,
        arguments: &super::Buffer,
        arguments_offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawIndexedPrimitives(indirect)";
        let (index_type, stride) = checked_index_type(index_type, operation)?;
        let range = checked_range(index_buffer.length(), index_range, operation)?;
        if range.is_empty() || !range.length.is_multiple_of(stride) {
            return Err(Error::invalid_argument(format!(
                "{operation} index range is invalid"
            )));
        }
        let index_address = checked_buffer_address(
            index_buffer,
            range.location,
            range.length,
            stride,
            operation,
        )?;
        let indirect_address =
            checked_buffer_address(arguments, arguments_offset, 20, 4, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawIndexedPrimitives:indexType:indexBuffer:indexBufferLength:indirectBuffer:),
            operation,
        )?;
        // SAFETY: Selector and both retained-buffer address ranges are checked.
        unsafe {
            msg_send![self.inner.as_inner(), drawIndexedPrimitives: primitive.as_raw(), indexType: index_type, indexBuffer: index_address, indexBufferLength: range.length, indirectBuffer: indirect_address]
        }
        Ok(())
    }

    /// Draws mesh threadgroups directly.
    pub fn draw_mesh_threadgroups(
        &self,
        groups: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawMeshThreadgroups";
        let groups = validate_size(groups, operation)?;
        let object_threads = validate_size(object_threads, operation)?;
        let mesh_threads = validate_size(mesh_threads, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawMeshThreadgroups:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:),
            operation,
        )?;
        // SAFETY: Selector and all MTLSize values are checked.
        unsafe {
            msg_send![self.inner.as_inner(), drawMeshThreadgroups: groups, threadsPerObjectThreadgroup: object_threads, threadsPerMeshThreadgroup: mesh_threads]
        }
        Ok(())
    }

    /// Draws mesh threads directly.
    pub fn draw_mesh_threads(
        &self,
        grid: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawMeshThreads";
        let grid = validate_size(grid, operation)?;
        let object_threads = validate_size(object_threads, operation)?;
        let mesh_threads = validate_size(mesh_threads, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawMeshThreads:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:),
            operation,
        )?;
        // SAFETY: Selector and all MTLSize values are checked.
        unsafe {
            msg_send![self.inner.as_inner(), drawMeshThreads: grid, threadsPerObjectThreadgroup: object_threads, threadsPerMeshThreadgroup: mesh_threads]
        }
        Ok(())
    }

    /// Draws mesh threadgroups using checked indirect arguments.
    pub fn draw_mesh_threadgroups_indirect(
        &self,
        buffer: &super::Buffer,
        offset: usize,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::drawMeshThreadgroups(indirect)";
        let address = checked_buffer_address(buffer, offset, 12, 4, operation)?;
        let object_threads = validate_size(object_threads, operation)?;
        let mesh_threads = validate_size(mesh_threads, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(drawMeshThreadgroupsWithIndirectBuffer:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:),
            operation,
        )?;
        // SAFETY: Selector, live-buffer address, and MTLSize values are checked.
        unsafe {
            msg_send![self.inner.as_inner(), drawMeshThreadgroupsWithIndirectBuffer: address, threadsPerObjectThreadgroup: object_threads, threadsPerMeshThreadgroup: mesh_threads]
        }
        Ok(())
    }

    /// Executes a checked CPU-selected indirect-command range.
    pub fn execute_commands(
        &self,
        commands: &metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::executeCommandsInBuffer(range)";
        let range = checked_range(commands.size()?, range, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(executeCommandsInBuffer:withRange:),
            operation,
        )?;
        // SAFETY: Selector, object identity, and range are checked.
        unsafe {
            msg_send![self.inner.as_inner(), executeCommandsInBuffer: commands.as_inner(), withRange: range]
        }
        Ok(())
    }

    /// Executes an indirect-command range read from a checked buffer offset.
    pub fn execute_commands_indirect(
        &self,
        commands: &metal::IndirectCommandBuffer,
        buffer: &super::Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::executeCommandsInBuffer(indirect)";
        let address = checked_buffer_address(buffer, offset, 16, 4, operation)?;
        require_selector(
            self.inner.as_inner(),
            sel!(executeCommandsInBuffer:indirectBuffer:),
            operation,
        )?;
        // SAFETY: Selector and object identity are checked and address is derived from a retained buffer.
        unsafe {
            msg_send![self.inner.as_inner(), executeCommandsInBuffer: commands.as_inner(), indirectBuffer: address]
        }
        Ok(())
    }

    /// Sets one scissor rectangle.
    pub fn set_scissor_rect(&self, rect: ScissorRect) -> Result<(), Error> {
        if rect.width == 0 || rect.height == 0 {
            return Err(Error::invalid_argument(
                "scissor rectangle must be non-empty",
            ));
        }
        let raw = MTLScissorRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        };
        let operation = "MTL4::RenderCommandEncoder::setScissorRect";
        require_selector(self.inner.as_inner(), sel!(setScissorRect:), operation)?;
        // SAFETY: Selector availability and MTLScissorRect ABI are checked.
        unsafe { msg_send![self.inner.as_inner(), setScissorRect: raw] }
        Ok(())
    }

    /// Sets a non-empty array of validated scissor rectangles.
    pub fn set_scissor_rects(&self, rects: &[ScissorRect]) -> Result<(), Error> {
        if rects.is_empty() {
            return Err(Error::invalid_argument(
                "scissor rectangle array must not be empty",
            ));
        }
        let raw: Result<Vec<_>, Error> = rects
            .iter()
            .map(|rect| {
                if rect.width == 0 || rect.height == 0 {
                    Err(Error::invalid_argument(
                        "scissor rectangle must be non-empty",
                    ))
                } else {
                    Ok(MTLScissorRect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    })
                }
            })
            .collect();
        let raw = raw?;
        let operation = "MTL4::RenderCommandEncoder::setScissorRects";
        require_selector(
            self.inner.as_inner(),
            sel!(setScissorRects:count:),
            operation,
        )?;
        // SAFETY: Selector is checked and the contiguous slice lives through the synchronous call.
        unsafe { msg_send![self.inner.as_inner(), setScissorRects: raw.as_ptr(), count: raw.len()] }
        Ok(())
    }

    /// Sets a non-empty viewport array.
    pub fn set_viewports(&self, viewports: &[Viewport]) -> Result<(), Error> {
        if viewports.is_empty() {
            return Err(Error::invalid_argument("viewport array must not be empty"));
        }
        let raw: Result<Vec<_>, _> = viewports.iter().copied().map(validate_viewport).collect();
        let raw = raw?;
        let operation = "MTL4::RenderCommandEncoder::setViewports";
        require_selector(self.inner.as_inner(), sel!(setViewports:count:), operation)?;
        // SAFETY: Selector is checked and the contiguous slice lives through the synchronous call.
        unsafe { msg_send![self.inner.as_inner(), setViewports: raw.as_ptr(), count: raw.len()] }
        Ok(())
    }

    /// Sets or clears logical-to-physical color attachment mapping.
    pub fn set_color_attachment_map(
        &self,
        mapping: Option<&metal::LogicalToPhysicalColorAttachmentMap>,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::setColorAttachmentMap";
        require_selector(
            self.inner.as_inner(),
            sel!(setColorAttachmentMap:),
            operation,
        )?;
        // SAFETY: Selector availability and optional wrapper identity are checked.
        unsafe {
            msg_send![self.inner.as_inner(), setColorAttachmentMap: mapping.map(metal::LogicalToPhysicalColorAttachmentMap::as_inner)]
        }
        Ok(())
    }

    /// Configures vertex amplification using a validated mapping array.
    pub fn set_vertex_amplification(
        &self,
        mappings: &[VertexAmplificationViewMapping],
    ) -> Result<(), Error> {
        if mappings.is_empty() {
            return Err(Error::invalid_argument(
                "vertex amplification mappings must not be empty",
            ));
        }
        let raw: Vec<_> = mappings
            .iter()
            .map(|value| MTLVertexAmplificationViewMapping {
                viewportArrayIndexOffset: value.viewport_array_index_offset,
                renderTargetArrayIndexOffset: value.render_target_array_index_offset,
            })
            .collect();
        let operation = "MTL4::RenderCommandEncoder::setVertexAmplificationCount";
        require_selector(
            self.inner.as_inner(),
            sel!(setVertexAmplificationCount:viewMappings:),
            operation,
        )?;
        // SAFETY: Selector is checked and the contiguous ABI array remains live synchronously.
        unsafe {
            msg_send![self.inner.as_inner(), setVertexAmplificationCount: raw.len(), viewMappings: raw.as_ptr()]
        }
        Ok(())
    }

    /// Writes a timestamp after a checked render stage.
    pub fn write_timestamp(
        &self,
        granularity: TimestampGranularity,
        stage: RenderStages,
        heap: &super::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), Error> {
        let operation = "MTL4::RenderCommandEncoder::writeTimestamp";
        if !granularity.is_valid() || !stage.is_valid() || index >= heap.count() {
            return Err(Error::invalid_argument(format!(
                "{operation} received invalid granularity, stage, or index"
            )));
        }
        require_selector(
            self.inner.as_inner(),
            sel!(writeTimestampWithGranularity:afterStage:intoHeap:atIndex:),
            operation,
        )?;
        // SAFETY: Selector, enum/option values, heap identity, and index are checked.
        unsafe {
            msg_send![self.inner.as_inner(), writeTimestampWithGranularity: granularity.as_raw(), afterStage: stage.as_raw(), intoHeap: heap.as_any_object(), atIndex: index]
        }
        Ok(())
    }

    /// Draws a non-indexed primitive range.
    pub fn draw_primitives(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives(primitive, vertex_start, vertex_count)
    }

    /// Draws instanced non-indexed primitives.
    pub fn draw_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives_instanced(primitive, vertex_start, vertex_count, instance_count)
    }

    /// Draws instanced non-indexed primitives with a base instance.
    pub fn draw_primitives_instanced_base_instance(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner.draw_primitives_instanced_base_instance(
            primitive,
            vertex_start,
            vertex_count,
            instance_count,
            base_instance,
        )
    }

    /// Sets the constant blend color.
    pub fn set_blend_color(
        &self,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> Result<(), Error> {
        self.inner.set_blend_color(red, green, blue, alpha)
    }

    /// Binds an optional argument table to selected render stages.
    pub fn set_argument_table(
        &self,
        table: Option<&super::Mtl4ArgumentTable>,
        stages: RenderStages,
    ) -> Result<(), Error> {
        self.inner
            .set_argument_table(table.map(super::Mtl4ArgumentTable::as_generated), stages)
    }

    /// Sets face culling mode.
    pub fn set_cull_mode(&self, mode: CullMode) -> Result<(), Error> {
        self.inner.set_cull_mode_checked(mode)
    }

    /// Sets front-face winding.
    pub fn set_front_facing_winding(&self, winding: Winding) -> Result<(), Error> {
        self.inner.set_front_facing_winding_checked(winding)
    }

    /// Sets triangle fill mode.
    pub fn set_triangle_fill_mode(&self, mode: TriangleFillMode) -> Result<(), Error> {
        self.inner.set_triangle_fill_mode_checked(mode)
    }

    /// Sets depth clipping mode.
    pub fn set_depth_clip_mode(&self, mode: DepthClipMode) -> Result<(), Error> {
        self.inner.set_depth_clip_mode_checked(mode)
    }

    /// Sets or clears the depth-stencil state.
    pub fn set_depth_stencil_state(
        &self,
        state: Option<&metal::DepthStencilState>,
    ) -> Result<(), Error> {
        self.inner.set_depth_stencil_state(state)
    }

    /// Binds the render pipeline state.
    pub fn set_render_pipeline_state(
        &self,
        state: &super::RenderPipelineState,
    ) -> Result<(), Error> {
        self.inner.set_render_pipeline_state(state)
    }

    /// Sets the raster viewport.
    pub fn set_viewport(&self, viewport: Viewport) -> Result<(), Error> {
        self.inner.set_viewport_checked(viewport)
    }

    /// Sets depth bias parameters.
    pub fn set_depth_bias(&self, bias: f32, slope: f32, clamp: f32) -> Result<(), Error> {
        self.inner.set_depth_bias(bias, slope, clamp)
    }

    /// Sets inclusive depth-test bounds.
    pub fn set_depth_test_bounds(&self, minimum: f32, maximum: f32) -> Result<(), Error> {
        self.inner.set_depth_test_bounds(minimum, maximum)
    }

    /// Sets one stencil reference value for both faces.
    pub fn set_stencil_reference_value(&self, value: u32) -> Result<(), Error> {
        self.inner.set_stencil_reference_value(value)
    }

    /// Sets distinct front and back stencil reference values.
    pub fn set_stencil_reference_values(&self, front: u32, back: u32) -> Result<(), Error> {
        self.inner.set_stencil_reference_values(front, back)
    }

    /// Configures visibility-result writes.
    pub fn set_visibility_result_mode(
        &self,
        mode: VisibilityResultMode,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner.set_visibility_result_mode(mode, offset)
    }

    /// Sets a color attachment store action.
    pub fn set_color_store_action(&self, action: StoreAction, index: usize) -> Result<(), Error> {
        self.inner.set_color_store_action(action, index)
    }

    /// Sets the depth attachment store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner.set_depth_store_action(action)
    }

    /// Sets the stencil attachment store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner.set_stencil_store_action(action)
    }

    /// Configures render threadgroup memory.
    pub fn set_threadgroup_memory_length(
        &self,
        length: usize,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_threadgroup_memory_length(length, offset, index)
    }

    /// Configures object-stage threadgroup memory.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_threadgroup_memory_length(length, index)
    }

    /// Explicitly ends encoding and releases the exclusive recording borrow.
    pub fn end(mut self) -> Result<(), Error> {
        self.inner.finish()?;
        self.ended = true;
        self.recording.active_encoder = false;
        Ok(())
    }
}

impl Drop for RecordingRenderEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.inner.finish();
            self.recording.active_encoder = false;
        }
    }
}

/// An exclusively borrowed Metal 4 machine-learning encoder.
pub struct RecordingMachineLearningEncoder<'a> {
    inner: metal4::MachineLearningCommandEncoder,
    recording: &'a mut RecordingCommandBuffer,
    ended: bool,
}

impl RecordingMachineLearningEncoder<'_> {
    /// Binds a machine-learning pipeline state.
    pub fn set_pipeline_state(
        &self,
        state: &metal4::MachineLearningPipelineState,
    ) -> Result<(), Error> {
        let operation = "MTL4::MachineLearningCommandEncoder::setPipelineState";
        require_selector(self.inner.as_inner(), sel!(setPipelineState:), operation)?;
        // SAFETY: Selector availability and retained pipeline-state identity are checked.
        unsafe { msg_send![self.inner.as_inner(), setPipelineState: state.as_inner()] }
        Ok(())
    }

    /// Binds or clears the machine-learning argument table.
    pub fn set_argument_table(
        &self,
        table: Option<&super::Mtl4ArgumentTable>,
    ) -> Result<(), Error> {
        let operation = "MTL4::MachineLearningCommandEncoder::setArgumentTable";
        require_selector(self.inner.as_inner(), sel!(setArgumentTable:), operation)?;
        // SAFETY: Selector availability and optional argument-table identity are checked.
        unsafe {
            msg_send![self.inner.as_inner(), setArgumentTable: table.map(super::Mtl4ArgumentTable::as_generated).map(metal4::ArgumentTable::as_inner)]
        }
        Ok(())
    }

    /// Dispatches the configured network using a retained intermediate heap.
    pub fn dispatch_network(&self, intermediates_heap: &metal::Heap) -> Result<(), Error> {
        let operation = "MTL4::MachineLearningCommandEncoder::dispatchNetwork";
        require_selector(
            self.inner.as_inner(),
            sel!(dispatchNetworkWithIntermediatesHeap:),
            operation,
        )?;
        // SAFETY: Selector availability and retained MTLHeap identity are checked.
        unsafe {
            msg_send![self.inner.as_inner(), dispatchNetworkWithIntermediatesHeap: intermediates_heap.as_inner()]
        }
        Ok(())
    }

    /// Explicitly ends encoding and releases the exclusive recording borrow.
    pub fn end(mut self) -> Result<(), Error> {
        end_encoding(
            self.inner.as_inner(),
            "MTL4::MachineLearningCommandEncoder::endEncoding",
        )?;
        self.ended = true;
        self.recording.active_encoder = false;
        Ok(())
    }
}

impl Drop for RecordingMachineLearningEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            let _ = end_encoding(
                self.inner.as_inner(),
                "MTL4::MachineLearningCommandEncoder::endEncoding",
            );
            self.recording.active_encoder = false;
        }
    }
}

macro_rules! impl_recording_encoder_common {
    ($type:ident) => {
        impl $type<'_> {
            /// Returns the optional encoder label.
            pub fn label(&self) -> Result<Option<String>, Error> {
                let object = self.inner.as_inner();
                require_selector(object, sel!(label), "MTL4::CommandEncoder::label")?;
                // SAFETY: Selector availability and NSString object return ABI are checked.
                let value: Option<Retained<NSString>> = unsafe { msg_send![object, label] };
                Ok(value.map(|value| value.to_string()))
            }

            /// Sets the optional encoder label from an owned Rust string view.
            pub fn set_label(&self, value: Option<&str>) -> Result<(), Error> {
                let object = self.inner.as_inner();
                require_selector(object, sel!(setLabel:), "MTL4::CommandEncoder::setLabel")?;
                let value = value.map(NSString::from_str);
                // SAFETY: Selector availability is checked and any NSString
                // argument remains alive for the synchronous message.
                unsafe { msg_send![object, setLabel: value.as_deref()] }
                Ok(())
            }

            /// Inserts a debug signpost into the encoder stream.
            pub fn insert_debug_signpost(&self, value: &str) -> Result<(), Error> {
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(insertDebugSignpost:),
                    "MTL4::CommandEncoder::insertDebugSignpost",
                )?;
                let value = NSString::from_str(value);
                // SAFETY: Selector availability is checked and NSString remains live.
                unsafe { msg_send![object, insertDebugSignpost: &*value] }
                Ok(())
            }

            /// Pushes an encoder debug group.
            pub fn push_debug_group(&self, value: &str) -> Result<(), Error> {
                push_debug_group(
                    self.inner.as_inner(),
                    value,
                    "MTL4::CommandEncoder::pushDebugGroup",
                )
            }

            /// Pops the current encoder debug group.
            pub fn pop_debug_group(&self) -> Result<(), Error> {
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(popDebugGroup),
                    "MTL4::CommandEncoder::popDebugGroup",
                )?;
                // SAFETY: Selector availability and its zero-argument void ABI are checked.
                unsafe { msg_send![object, popDebugGroup] }
                Ok(())
            }

            /// Adds an encoder-to-encoder stage barrier.
            pub fn barrier_after_encoder_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), Error> {
                if !after.is_valid() || !before.is_valid() || !visibility.is_valid() {
                    return Err(Error::invalid_argument("invalid Metal 4 barrier option bits"));
                }
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(barrierAfterEncoderStages:beforeEncoderStages:visibilityOptions:),
                    "MTL4::CommandEncoder::barrierAfterEncoderStages",
                )?;
                // SAFETY: Selector and scalar option values use checked NSUInteger ABIs.
                unsafe {
                    msg_send![object,
                        barrierAfterEncoderStages: after.as_raw(),
                        beforeEncoderStages: before.as_raw(),
                        visibilityOptions: visibility.as_raw()
                    ]
                }
                Ok(())
            }

            /// Adds a barrier from earlier queue stages into this encoder.
            pub fn barrier_after_queue_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), Error> {
                if !after.is_valid() || !before.is_valid() || !visibility.is_valid() {
                    return Err(Error::invalid_argument("invalid Metal 4 barrier option bits"));
                }
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(barrierAfterQueueStages:beforeStages:visibilityOptions:),
                    "MTL4::CommandEncoder::barrierAfterQueueStages",
                )?;
                // SAFETY: Selector and scalar option values use checked NSUInteger ABIs.
                unsafe {
                    msg_send![object,
                        barrierAfterQueueStages: after.as_raw(),
                        beforeStages: before.as_raw(),
                        visibilityOptions: visibility.as_raw()
                    ]
                }
                Ok(())
            }

            /// Adds a barrier from this encoder into later queue stages.
            pub fn barrier_after_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), Error> {
                if !after.is_valid() || !before.is_valid() || !visibility.is_valid() {
                    return Err(Error::invalid_argument("invalid Metal 4 barrier option bits"));
                }
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(barrierAfterStages:beforeQueueStages:visibilityOptions:),
                    "MTL4::CommandEncoder::barrierAfterStages",
                )?;
                // SAFETY: Selector and scalar option values use checked NSUInteger ABIs.
                unsafe {
                    msg_send![object,
                        barrierAfterStages: after.as_raw(),
                        beforeQueueStages: before.as_raw(),
                        visibilityOptions: visibility.as_raw()
                    ]
                }
                Ok(())
            }

            /// Updates a fence after selected encoder stages.
            pub fn update_fence(&self, fence: &metal::Fence, after: Stages) -> Result<(), Error> {
                if !after.is_valid() {
                    return Err(Error::invalid_argument("invalid Metal stage bits"));
                }
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(updateFence:afterEncoderStages:),
                    "MTL4::CommandEncoder::updateFence",
                )?;
                // SAFETY: Selector availability, fence identity, and option bits are checked.
                unsafe {
                    msg_send![object,
                        updateFence: fence.as_inner(),
                        afterEncoderStages: after.as_raw()
                    ]
                }
                Ok(())
            }

            /// Waits for a fence before selected encoder stages.
            pub fn wait_for_fence(&self, fence: &metal::Fence, before: Stages) -> Result<(), Error> {
                if !before.is_valid() {
                    return Err(Error::invalid_argument("invalid Metal stage bits"));
                }
                let object = self.inner.as_inner();
                require_selector(
                    object,
                    sel!(waitForFence:beforeEncoderStages:),
                    "MTL4::CommandEncoder::waitForFence",
                )?;
                // SAFETY: Selector availability, fence identity, and option bits are checked.
                unsafe {
                    msg_send![object,
                        waitForFence: fence.as_inner(),
                        beforeEncoderStages: before.as_raw()
                    ]
                }
                Ok(())
            }
        }
    };
}

impl_recording_encoder_common!(RecordingComputeEncoder);
impl_recording_encoder_common!(RecordingRenderEncoder);
impl_recording_encoder_common!(RecordingMachineLearningEncoder);

/// Opaque proof that a batch of Metal 4 command buffers was submitted once.
pub struct SubmittedCommandBuffers {
    count: usize,
    submission_ids: Vec<u64>,
    completion: CompletionState,
}

/// Proof that a Metal 4 submission batch completed successfully.
pub struct CompletedCommandBuffers {
    count: usize,
    submission_ids: Vec<u64>,
}

/// A safe reusable Metal 4 command queue facade over the generated shell.
#[derive(Clone)]
pub struct Metal4CommandQueue {
    inner: metal4::CommandQueue,
}

impl Metal4CommandQueue {
    pub(crate) const fn from_generated(inner: metal4::CommandQueue) -> Self {
        Self { inner }
    }

    #[allow(dead_code)]
    pub(crate) const fn as_generated(&self) -> &metal4::CommandQueue {
        &self.inner
    }
    /// Returns the owning device.
    pub fn device(&self) -> Result<crate::Device, Error> {
        self.inner.device()?.ok_or_else(|| {
            Error::unsupported("MTL4::CommandQueue::device unexpectedly returned nil")
        })
    }

    /// Returns the optional queue label.
    pub fn label(&self) -> Result<Option<String>, Error> {
        self.inner.label()
    }

    /// Submits a non-empty batch of ended command buffers exactly once.
    pub fn submit(
        &self,
        buffers: Vec<EndedCommandBuffer>,
        options: Option<&metal4::CommitOptions>,
    ) -> Result<SubmittedCommandBuffers, Error> {
        self.inner.submit(buffers, options)
    }

    /// Adds one residency set to the queue.
    pub fn add_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        self.inner.add_residency_set(set)
    }

    /// Adds a non-empty slice of residency sets to the queue.
    pub fn add_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        self.inner.add_residency_sets(sets)
    }

    /// Removes one residency set from the queue.
    pub fn remove_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        self.inner.remove_residency_set(set)
    }

    /// Removes a non-empty slice of residency sets from the queue.
    pub fn remove_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        self.inner.remove_residency_sets(sets)
    }

    /// Schedules an event signal after earlier queue work.
    pub fn signal_event(&self, event: &metal::Event, value: u64) -> Result<(), Error> {
        self.inner.signal_event(event, value)
    }

    /// Schedules drawable signaling after earlier queue work.
    pub fn signal_drawable(&self, drawable: &LayerDrawable) -> Result<(), Error> {
        self.inner.signal_drawable(drawable)
    }

    /// Schedules a wait for an event value before later queue work.
    pub fn wait_for_event(&self, event: &metal::Event, value: u64) -> Result<(), Error> {
        self.inner.wait_for_event(event, value)
    }

    /// Schedules a wait for drawable availability before later queue work.
    pub fn wait_for_drawable(&self, drawable: &LayerDrawable) -> Result<(), Error> {
        self.inner.wait_for_drawable(drawable)
    }

    /// Updates sparse buffer mappings from safe value operations.
    pub fn update_buffer_mappings(
        &self,
        buffer: &super::Buffer,
        heap: Option<&metal::Heap>,
        operations: &[UpdateSparseBufferMappingOperation],
    ) -> Result<(), Error> {
        if operations.is_empty() {
            return Err(Error::invalid_argument(
                "sparse mapping operations must not be empty",
            ));
        }
        let raw: Result<Vec<_>, Error> = operations
            .iter()
            .map(|value| {
                let range = checked_range(
                    buffer.length(),
                    value.buffer_range.clone(),
                    "MTL4::CommandQueue::updateBufferMappings",
                )?;
                Ok(MTL4UpdateSparseBufferMappingOperation {
                    mode: MTLSparseTextureMappingMode(value.mode.as_raw()),
                    bufferRange: range,
                    heapOffset: value.heap_offset,
                })
            })
            .collect();
        let raw = raw?;
        let operation = "MTL4::CommandQueue::updateBufferMappings";
        require_selector(
            self.inner.as_inner(),
            sel!(updateBufferMappings:heap:operations:count:),
            operation,
        )?;
        // SAFETY: Selector and resource identities are checked; the ABI array is
        // built from bounded safe operations and lives through this call.
        unsafe {
            msg_send![self.inner.as_inner(), updateBufferMappings: buffer.as_any_object(), heap: heap.map(metal::Heap::as_inner), operations: raw.as_ptr(), count: raw.len()]
        }
        Ok(())
    }

    /// Copies sparse buffer mappings using safe value operations.
    pub fn copy_buffer_mappings(
        &self,
        source: &super::Buffer,
        destination: &super::Buffer,
        operations: &[CopySparseBufferMappingOperation],
    ) -> Result<(), Error> {
        if operations.is_empty() {
            return Err(Error::invalid_argument(
                "sparse mapping operations must not be empty",
            ));
        }
        let raw: Result<Vec<_>, Error> = operations
            .iter()
            .map(|value| {
                let range = checked_range(
                    source.length(),
                    value.source_range.clone(),
                    "MTL4::CommandQueue::copyBufferMappings",
                )?;
                value
                    .destination_offset
                    .checked_add(range.length)
                    .filter(|end| *end <= destination.length())
                    .ok_or_else(|| {
                        Error::invalid_argument("sparse destination range is out of bounds")
                    })?;
                Ok(MTL4CopySparseBufferMappingOperation {
                    sourceRange: range,
                    destinationOffset: value.destination_offset,
                })
            })
            .collect();
        let raw = raw?;
        let operation = "MTL4::CommandQueue::copyBufferMappings";
        require_selector(
            self.inner.as_inner(),
            sel!(copyBufferMappingsFromBuffer:toBuffer:operations:count:),
            operation,
        )?;
        // SAFETY: Selector and resource identities are checked; all operation
        // ranges are bounded and the ABI array remains live synchronously.
        unsafe {
            msg_send![self.inner.as_inner(), copyBufferMappingsFromBuffer: source.as_any_object(), toBuffer: destination.as_any_object(), operations: raw.as_ptr(), count: raw.len()]
        }
        Ok(())
    }

    /// Updates sparse texture mappings from safe value operations.
    pub fn update_texture_mappings(
        &self,
        texture: &super::Texture,
        heap: Option<&metal::Heap>,
        operations: &[UpdateSparseTextureMappingOperation],
    ) -> Result<(), Error> {
        if operations.is_empty() {
            return Err(Error::invalid_argument(
                "sparse mapping operations must not be empty",
            ));
        }
        let raw: Result<Vec<_>, Error> = operations
            .iter()
            .map(|value| {
                let (_, array_length, mipmap_levels, _) = texture.layout();
                if value.texture_region.size.width == 0
                    || value.texture_region.size.height == 0
                    || value.texture_region.size.depth == 0
                    || value.texture_level >= mipmap_levels
                    || value.texture_slice >= array_length
                {
                    return Err(Error::invalid_argument(
                        "sparse texture operation has invalid region, level, or slice",
                    ));
                }
                Ok(MTL4UpdateSparseTextureMappingOperation {
                    mode: MTLSparseTextureMappingMode(value.mode.as_raw()),
                    textureRegion: value.texture_region.into(),
                    textureLevel: value.texture_level,
                    textureSlice: value.texture_slice,
                    heapOffset: value.heap_offset,
                })
            })
            .collect();
        let raw = raw?;
        let operation = "MTL4::CommandQueue::updateTextureMappings";
        require_selector(
            self.inner.as_inner(),
            sel!(updateTextureMappings:heap:operations:count:),
            operation,
        )?;
        // SAFETY: Selector/resource identities are checked and the validated ABI
        // array remains live for the synchronous Objective-C invocation.
        unsafe {
            msg_send![self.inner.as_inner(), updateTextureMappings: texture.as_any_object(), heap: heap.map(metal::Heap::as_inner), operations: raw.as_ptr(), count: raw.len()]
        }
        Ok(())
    }

    /// Copies sparse texture mappings from safe value operations.
    pub fn copy_texture_mappings(
        &self,
        source: &super::Texture,
        destination: &super::Texture,
        operations: &[CopySparseTextureMappingOperation],
    ) -> Result<(), Error> {
        if operations.is_empty() {
            return Err(Error::invalid_argument(
                "sparse mapping operations must not be empty",
            ));
        }
        let raw: Result<Vec<_>, Error> = operations
            .iter()
            .map(|value| {
                let (_, source_array_length, source_mipmap_levels, _) = source.layout();
                let (_, destination_array_length, destination_mipmap_levels, _) =
                    destination.layout();
                if value.source_region.size.width == 0
                    || value.source_region.size.height == 0
                    || value.source_region.size.depth == 0
                    || value.source_level >= source_mipmap_levels
                    || value.destination_level >= destination_mipmap_levels
                    || value.source_slice >= source_array_length
                    || value.destination_slice >= destination_array_length
                {
                    return Err(Error::invalid_argument(
                        "sparse texture copy has invalid region, level, or slice",
                    ));
                }
                Ok(MTL4CopySparseTextureMappingOperation {
                    sourceRegion: value.source_region.into(),
                    sourceLevel: value.source_level,
                    sourceSlice: value.source_slice,
                    destinationOrigin: value.destination_origin.into(),
                    destinationLevel: value.destination_level,
                    destinationSlice: value.destination_slice,
                })
            })
            .collect();
        let raw = raw?;
        let operation = "MTL4::CommandQueue::copyTextureMappings";
        require_selector(
            self.inner.as_inner(),
            sel!(copyTextureMappingsFromTexture:toTexture:operations:count:),
            operation,
        )?;
        // SAFETY: Selector/resource identities are checked and the validated ABI
        // array remains live for the synchronous Objective-C invocation.
        unsafe {
            msg_send![self.inner.as_inner(), copyTextureMappingsFromTexture: source.as_any_object(), toTexture: destination.as_any_object(), operations: raw.as_ptr(), count: raw.len()]
        }
        Ok(())
    }
}

impl SubmittedCommandBuffers {
    /// Returns the number of command buffers in the submitted batch.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Blocks without an async runtime until Metal reports completion.
    pub fn wait(self) -> Result<CompletedCommandBuffers, Error> {
        let (lock, ready) = &*self.completion;
        let mut result = lock
            .lock()
            .map_err(|_| Error::unsupported("Metal 4 completion state was poisoned"))?;
        while result.is_none() {
            result = ready
                .wait(result)
                .map_err(|_| Error::unsupported("Metal 4 completion state was poisoned"))?;
        }
        result.take().expect("completion state checked")?;
        Ok(CompletedCommandBuffers {
            count: self.count,
            submission_ids: self.submission_ids,
        })
    }
}

impl CompletedCommandBuffers {
    /// Returns the number of command buffers in the completed batch.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Resolves one identity-matched counter staging handle.
    pub fn resolve_counters(
        &self,
        readback: super::mtl4_counters::CounterReadback,
    ) -> Result<(super::TimestampCounterHeap, Vec<u64>), Error> {
        if !submission_contains(&self.submission_ids, readback.submission_id) {
            return Err(Error::invalid_argument(
                "counter readback belongs to a different Metal 4 submission",
            ));
        }
        let bytes = readback.buffer.completed_bytes(
            readback
                .count
                .checked_mul(8)
                .ok_or_else(|| Error::invalid_argument("counter readback size overflows"))?,
        )?;
        let values = bytes
            .chunks_exact(8)
            .map(|chunk| {
                u64::from_ne_bytes(chunk.try_into().expect("timestamp chunk has fixed width"))
            })
            .collect();
        Ok((readback.heap, values))
    }
}

impl metal4::CommandQueue {
    /// Submits a non-empty batch of ended command buffers exactly once.
    pub fn submit(
        &self,
        buffers: Vec<EndedCommandBuffer>,
        options: Option<&metal4::CommitOptions>,
    ) -> Result<SubmittedCommandBuffers, Error> {
        if buffers.is_empty() {
            return Err(Error::invalid_argument(
                "command buffer batch must not be empty",
            ));
        }
        let submission_ids: Vec<_> = buffers.iter().map(|buffer| buffer.submission_id).collect();
        let objects: Vec<*const AnyObject> = buffers
            .iter()
            .map(|buffer| buffer.inner.as_inner() as *const AnyObject)
            .collect();
        let object = self.as_inner();
        let options = options
            .cloned()
            .map(Ok)
            .unwrap_or_else(metal4::CommitOptions::new)?;
        require_selector(
            options.as_inner(),
            sel!(addFeedbackHandler:),
            "MTL4::CommitOptions::addFeedbackHandler",
        )?;
        require_selector(
            object,
            sel!(commit:count:options:),
            "MTL4::CommandQueue::commit(options)",
        )?;
        let completion: CompletionState = Arc::new((Mutex::new(None), Condvar::new()));
        let callback_state = Arc::clone(&completion);
        let block = RcBlock::new(move |feedback: NonNull<AnyObject>| {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: Metal invokes this block with a non-null object that
                // conforms to MTL4CommitFeedback; error is a nullable NSError.
                let error: Option<Retained<objc2_foundation::NSError>> =
                    unsafe { msg_send![feedback.as_ref(), error] };
                error.map_or(Ok(()), |error| Err(metal_error(&error)))
            }))
            .unwrap_or_else(|_| Err(Error::unsupported("Metal 4 feedback callback panicked")));
            let (lock, ready) = &*callback_state;
            if let Ok(mut slot) = lock.lock() {
                *slot = Some(outcome);
                ready.notify_all();
            }
        });
        // SAFETY: Selector availability is checked and RcBlock has the exact
        // one-object/void ABI declared for MTL4CommitFeedbackHandler. The
        // options object retains the copied block for asynchronous delivery.
        unsafe { msg_send![options.as_inner(), addFeedbackHandler: &*block] }
        // SAFETY: The pointer array is derived from live ended command buffers,
        // count matches it, and options retains the completion block.
        unsafe {
            msg_send![object, commit: objects.as_ptr(), count: objects.len(), options: options.as_inner()]
        }
        Ok(SubmittedCommandBuffers {
            count: objects.len(),
            submission_ids,
            completion,
        })
    }

    /// Adds one residency set to the queue.
    pub fn add_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(addResidencySet:),
            "MTL4::CommandQueue::addResidencySet",
        )?;
        // SAFETY: Selector availability is checked and the generated wrapper
        // retains an object with MTLResidencySet identity.
        unsafe { msg_send![object, addResidencySet: set.as_inner()] }
        Ok(())
    }

    /// Adds a non-empty slice of residency sets to the queue.
    pub fn add_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        if sets.is_empty() {
            return Err(Error::invalid_argument(
                "residency set slice must not be empty",
            ));
        }
        let objects: Vec<*const AnyObject> = sets
            .iter()
            .map(|set| set.as_inner() as *const AnyObject)
            .collect();
        let object = self.as_inner();
        require_selector(
            object,
            sel!(addResidencySets:count:),
            "MTL4::CommandQueue::addResidencySets",
        )?;
        // SAFETY: The pointer array is derived from live retained wrappers,
        // remains allocated for the synchronous call, and count matches it.
        unsafe { msg_send![object, addResidencySets: objects.as_ptr(), count: objects.len()] }
        Ok(())
    }

    /// Removes one residency set from the queue.
    pub fn remove_residency_set(&self, set: &metal::ResidencySet) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(removeResidencySet:),
            "MTL4::CommandQueue::removeResidencySet",
        )?;
        // SAFETY: Selector availability is checked and the generated wrapper
        // retains an object with MTLResidencySet identity.
        unsafe { msg_send![object, removeResidencySet: set.as_inner()] }
        Ok(())
    }

    /// Removes a non-empty slice of residency sets from the queue.
    pub fn remove_residency_sets(&self, sets: &[&metal::ResidencySet]) -> Result<(), Error> {
        if sets.is_empty() {
            return Err(Error::invalid_argument(
                "residency set slice must not be empty",
            ));
        }
        let objects: Vec<*const AnyObject> = sets
            .iter()
            .map(|set| set.as_inner() as *const AnyObject)
            .collect();
        let object = self.as_inner();
        require_selector(
            object,
            sel!(removeResidencySets:count:),
            "MTL4::CommandQueue::removeResidencySets",
        )?;
        // SAFETY: The pointer array is derived from live retained wrappers,
        // remains allocated for the synchronous call, and count matches it.
        unsafe { msg_send![object, removeResidencySets: objects.as_ptr(), count: objects.len()] }
        Ok(())
    }

    /// Schedules an event signal after earlier queue work.
    pub fn signal_event(&self, event: &metal::Event, value: u64) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(signalEvent:value:),
            "MTL4::CommandQueue::signalEvent",
        )?;
        // SAFETY: Selector availability is checked, the event wrapper retains
        // its protocol object, and the value uses the declared uint64 ABI.
        unsafe { msg_send![object, signalEvent: event.as_inner(), value: value] }
        Ok(())
    }

    /// Schedules drawable signaling after earlier queue work.
    pub fn signal_drawable(&self, drawable: &LayerDrawable) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(signalDrawable:),
            "MTL4::CommandQueue::signalDrawable",
        )?;
        let drawable = ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable.inner);
        // SAFETY: Selector availability is checked and the layer drawable is
        // retained for the duration of this synchronous MTLDrawable message.
        unsafe { msg_send![object, signalDrawable: drawable] }
        Ok(())
    }

    /// Schedules a wait for an event value before later queue work.
    pub fn wait_for_event(&self, event: &metal::Event, value: u64) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(waitForEvent:value:),
            "MTL4::CommandQueue::wait(event)",
        )?;
        // SAFETY: Selector availability is checked, the event wrapper retains
        // its protocol object, and the value uses the declared uint64 ABI.
        unsafe { msg_send![object, waitForEvent: event.as_inner(), value: value] }
        Ok(())
    }

    /// Schedules a wait for drawable availability before later queue work.
    pub fn wait_for_drawable(&self, drawable: &LayerDrawable) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(waitForDrawable:),
            "MTL4::CommandQueue::wait(drawable)",
        )?;
        let drawable = ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable.inner);
        // SAFETY: Selector availability is checked and the layer drawable is
        // retained for the duration of this synchronous MTLDrawable message.
        unsafe { msg_send![object, waitForDrawable: drawable] }
        Ok(())
    }
}

impl metal4::RenderCommandEncoder {
    /// Ends render encoding.
    pub(crate) fn finish(&self) -> Result<(), Error> {
        end_encoding(self.as_inner(), "MTL4::RenderCommandEncoder::endEncoding")
    }

    /// Sets the current blend constant after finite-value validation.
    pub(crate) fn set_blend_color(
        &self,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> Result<(), Error> {
        if ![red, green, blue, alpha]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(Error::invalid_argument(
                "blend color components must be finite",
            ));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setBlendColorRed:green:blue:alpha:),
            "MTL4::RenderCommandEncoder::setBlendColor",
        )?;
        // SAFETY: Selector availability and four-float ABI are checked.
        unsafe { msg_send![object, setBlendColorRed: red, green: green, blue: blue, alpha: alpha] }
        Ok(())
    }

    /// Binds an argument table to selected render stages.
    pub(crate) fn set_argument_table(
        &self,
        table: Option<&metal4::ArgumentTable>,
        stages: RenderStages,
    ) -> Result<(), Error> {
        if !stages.is_valid() {
            return Err(Error::invalid_argument("invalid render stage bits"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setArgumentTable:atStages:),
            "MTL4::RenderCommandEncoder::setArgumentTable",
        )?;
        // SAFETY: Selector availability, argument-table identity, and stage bits are checked.
        unsafe {
            msg_send![object,
                setArgumentTable: table.map(metal4::ArgumentTable::as_inner),
                atStages: stages.as_raw()
            ]
        }
        Ok(())
    }

    /// Sets face culling mode.
    pub(crate) fn set_cull_mode_checked(&self, mode: CullMode) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument("invalid cull mode"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setCullMode:),
            "MTL4::RenderCommandEncoder::setCullMode",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setCullMode: mode.as_raw()] }
        Ok(())
    }

    /// Sets front-face winding.
    pub(crate) fn set_front_facing_winding_checked(&self, winding: Winding) -> Result<(), Error> {
        if !winding.is_valid() {
            return Err(Error::invalid_argument("invalid winding"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setFrontFacingWinding:),
            "MTL4::RenderCommandEncoder::setFrontFacingWinding",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setFrontFacingWinding: winding.as_raw()] }
        Ok(())
    }

    /// Sets triangle fill mode.
    pub(crate) fn set_triangle_fill_mode_checked(
        &self,
        mode: TriangleFillMode,
    ) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument("invalid triangle fill mode"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setTriangleFillMode:),
            "MTL4::RenderCommandEncoder::setTriangleFillMode",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setTriangleFillMode: mode.as_raw()] }
        Ok(())
    }

    /// Sets depth clipping mode.
    pub(crate) fn set_depth_clip_mode_checked(&self, mode: DepthClipMode) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument("invalid depth clip mode"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setDepthClipMode:),
            "MTL4::RenderCommandEncoder::setDepthClipMode",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setDepthClipMode: mode.as_raw()] }
        Ok(())
    }

    /// Sets the depth-stencil state object.
    pub(crate) fn set_depth_stencil_state(
        &self,
        state: Option<&metal::DepthStencilState>,
    ) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setDepthStencilState:),
            "MTL4::RenderCommandEncoder::setDepthStencilState",
        )?;
        // SAFETY: Selector availability is checked and the generated wrapper
        // retains an object with MTLDepthStencilState identity.
        unsafe {
            msg_send![object,
                setDepthStencilState: state.map(metal::DepthStencilState::as_inner)
            ]
        }
        Ok(())
    }

    /// Sets the render pipeline state object.
    pub(crate) fn set_render_pipeline_state(
        &self,
        state: &super::RenderPipelineState,
    ) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setRenderPipelineState:),
            "MTL4::RenderCommandEncoder::setRenderPipelineState",
        )?;
        // SAFETY: Selector availability is checked and the generated wrapper
        // retains an object with MTLRenderPipelineState identity.
        unsafe { msg_send![object, setRenderPipelineState: &*state.inner] }
        Ok(())
    }

    /// Sets the raster viewport after bounds validation.
    pub(crate) fn set_viewport_checked(&self, viewport: Viewport) -> Result<(), Error> {
        let viewport = validate_viewport(viewport)?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setViewport:),
            "MTL4::RenderCommandEncoder::setViewport",
        )?;
        // SAFETY: Selector availability and the SDK MTLViewport ABI type are used.
        unsafe { msg_send![object, setViewport: viewport] }
        Ok(())
    }

    /// Sets depth bias parameters after finite-value validation.
    pub(crate) fn set_depth_bias(
        &self,
        bias: f32,
        slope_scale: f32,
        clamp: f32,
    ) -> Result<(), Error> {
        if ![bias, slope_scale, clamp]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(Error::invalid_argument("depth bias values must be finite"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setDepthBias:slopeScale:clamp:),
            "MTL4::RenderCommandEncoder::setDepthBias",
        )?;
        // SAFETY: Selector availability and three-float ABI are checked.
        unsafe { msg_send![object, setDepthBias: bias, slopeScale: slope_scale, clamp: clamp] }
        Ok(())
    }

    /// Sets inclusive depth-test bounds after finite and ordering checks.
    pub(crate) fn set_depth_test_bounds(&self, minimum: f32, maximum: f32) -> Result<(), Error> {
        if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum {
            return Err(Error::invalid_argument("invalid depth-test bounds"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setDepthTestMinBound:maxBound:),
            "MTL4::RenderCommandEncoder::setDepthTestBounds",
        )?;
        // SAFETY: Selector availability and the two-float/void ABI are checked.
        unsafe { msg_send![object, setDepthTestMinBound: minimum, maxBound: maximum] }
        Ok(())
    }

    /// Sets a single stencil reference value.
    pub(crate) fn set_stencil_reference_value(&self, value: u32) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setStencilReferenceValue:),
            "MTL4::RenderCommandEncoder::setStencilReferenceValue",
        )?;
        // SAFETY: Selector availability and uint32 ABI are checked.
        unsafe { msg_send![object, setStencilReferenceValue: value] }
        Ok(())
    }

    /// Sets distinct front and back stencil reference values.
    pub(crate) fn set_stencil_reference_values(&self, front: u32, back: u32) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setStencilFrontReferenceValue:backReferenceValue:),
            "MTL4::RenderCommandEncoder::setStencilReferenceValues",
        )?;
        // SAFETY: Selector availability and two-uint32 ABI are checked.
        unsafe { msg_send![object, setStencilFrontReferenceValue: front, backReferenceValue: back] }
        Ok(())
    }

    /// Draws non-indexed primitives.
    pub(crate) fn draw_primitives(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
    ) -> Result<(), Error> {
        if !primitive.is_valid() || vertex_count == 0 {
            return Err(Error::invalid_argument("invalid primitive draw arguments"));
        }
        vertex_start
            .checked_add(vertex_count)
            .ok_or_else(|| Error::invalid_argument("vertex range overflows usize"))?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(drawPrimitives:vertexStart:vertexCount:),
            "MTL4::RenderCommandEncoder::drawPrimitives",
        )?;
        // SAFETY: Selector availability, validated enum, and checked range are used.
        unsafe {
            msg_send![object, drawPrimitives: primitive.as_raw(), vertexStart: vertex_start, vertexCount: vertex_count]
        }
        Ok(())
    }

    /// Draws instanced non-indexed primitives.
    pub(crate) fn draw_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
    ) -> Result<(), Error> {
        if !primitive.is_valid() || vertex_count == 0 || instance_count == 0 {
            return Err(Error::invalid_argument("invalid instanced draw arguments"));
        }
        vertex_start
            .checked_add(vertex_count)
            .ok_or_else(|| Error::invalid_argument("vertex range overflows usize"))?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(drawPrimitives:vertexStart:vertexCount:instanceCount:),
            "MTL4::RenderCommandEncoder::drawPrimitives(instanceCount)",
        )?;
        // SAFETY: Selector availability, validated enum, and checked counts are used.
        unsafe {
            msg_send![object,
                drawPrimitives: primitive.as_raw(),
                vertexStart: vertex_start,
                vertexCount: vertex_count,
                instanceCount: instance_count
            ]
        }
        Ok(())
    }

    /// Draws instanced non-indexed primitives with a base instance.
    pub(crate) fn draw_primitives_instanced_base_instance(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        if !primitive.is_valid() || vertex_count == 0 || instance_count == 0 {
            return Err(Error::invalid_argument("invalid instanced draw arguments"));
        }
        vertex_start
            .checked_add(vertex_count)
            .ok_or_else(|| Error::invalid_argument("vertex range overflows usize"))?;
        base_instance
            .checked_add(instance_count)
            .ok_or_else(|| Error::invalid_argument("instance range overflows usize"))?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:),
            "MTL4::RenderCommandEncoder::drawPrimitives(baseInstance)",
        )?;
        // SAFETY: Selector availability, validated enum, and checked ranges are used.
        unsafe {
            msg_send![object,
                drawPrimitives: primitive.as_raw(),
                vertexStart: vertex_start,
                vertexCount: vertex_count,
                instanceCount: instance_count,
                baseInstance: base_instance
            ]
        }
        Ok(())
    }

    /// Configures visibility-result writes.
    pub(crate) fn set_visibility_result_mode(
        &self,
        mode: VisibilityResultMode,
        offset: usize,
    ) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument("invalid visibility result mode"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setVisibilityResultMode:offset:),
            "MTL4::RenderCommandEncoder::setVisibilityResultMode",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setVisibilityResultMode: mode.as_raw(), offset: offset] }
        Ok(())
    }

    /// Sets a color attachment store action.
    pub(crate) fn set_color_store_action(
        &self,
        action: StoreAction,
        index: usize,
    ) -> Result<(), Error> {
        if !action.is_valid() {
            return Err(Error::invalid_argument("invalid store action"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setColorStoreAction:atIndex:),
            "MTL4::RenderCommandEncoder::setColorStoreAction",
        )?;
        // SAFETY: Selector availability and validated scalar arguments are checked.
        unsafe { msg_send![object, setColorStoreAction: action.as_raw(), atIndex: index] }
        Ok(())
    }

    /// Sets the depth attachment store action.
    pub(crate) fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        if !action.is_valid() {
            return Err(Error::invalid_argument("invalid store action"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setDepthStoreAction:),
            "MTL4::RenderCommandEncoder::setDepthStoreAction",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setDepthStoreAction: action.as_raw()] }
        Ok(())
    }

    /// Sets the stencil attachment store action.
    pub(crate) fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        if !action.is_valid() {
            return Err(Error::invalid_argument("invalid store action"));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setStencilStoreAction:),
            "MTL4::RenderCommandEncoder::setStencilStoreAction",
        )?;
        // SAFETY: Selector availability and validated NSUInteger enum ABI are checked.
        unsafe { msg_send![object, setStencilStoreAction: action.as_raw()] }
        Ok(())
    }

    /// Configures render threadgroup memory with a byte offset.
    pub(crate) fn set_threadgroup_memory_length(
        &self,
        length: usize,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "threadgroup memory index must be below 31",
            ));
        }
        offset
            .checked_add(length)
            .ok_or_else(|| Error::invalid_argument("threadgroup memory range overflows usize"))?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setThreadgroupMemoryLength:offset:atIndex:),
            "MTL4::RenderCommandEncoder::setThreadgroupMemoryLength",
        )?;
        // SAFETY: Selector availability and checked scalar arguments are used.
        unsafe {
            msg_send![object,
                setThreadgroupMemoryLength: length,
                offset: offset,
                atIndex: index
            ]
        }
        Ok(())
    }

    /// Configures object-stage threadgroup memory.
    pub(crate) fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "object threadgroup memory index must be below 31",
            ));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setObjectThreadgroupMemoryLength:atIndex:),
            "MTL4::RenderCommandEncoder::setObjectThreadgroupMemoryLength",
        )?;
        // SAFETY: Selector availability and checked scalar arguments are used.
        unsafe { msg_send![object, setObjectThreadgroupMemoryLength: length, atIndex: index] }
        Ok(())
    }
}

impl metal4::ComputeCommandEncoder {
    /// Ends compute encoding.
    pub(crate) fn finish(&self) -> Result<(), Error> {
        end_encoding(self.as_inner(), "MTL4::ComputeCommandEncoder::endEncoding")
    }

    /// Binds the compute argument table.
    pub(crate) fn set_argument_table(
        &self,
        table: Option<&metal4::ArgumentTable>,
    ) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setArgumentTable:),
            "MTL4::ComputeCommandEncoder::setArgumentTable",
        )?;
        // SAFETY: Selector availability and argument-table identity are checked.
        unsafe { msg_send![object, setArgumentTable: table.map(metal4::ArgumentTable::as_inner)] }
        Ok(())
    }

    /// Binds the compute pipeline state.
    pub(crate) fn set_compute_pipeline_state(
        &self,
        state: &super::ComputePipelineState,
    ) -> Result<(), Error> {
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setComputePipelineState:),
            "MTL4::ComputeCommandEncoder::setComputePipelineState",
        )?;
        // SAFETY: Selector availability and compute-pipeline identity are checked.
        unsafe { msg_send![object, setComputePipelineState: &*state.inner] }
        Ok(())
    }

    /// Dispatches a grid expressed in threadgroups.
    pub(crate) fn dispatch_threadgroups(
        &self,
        groups: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        let groups = validate_size(groups, "dispatchThreadgroups")?;
        let threads = validate_size(threads_per_group, "dispatchThreadgroups")?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(dispatchThreadgroups:threadsPerThreadgroup:),
            "MTL4::ComputeCommandEncoder::dispatchThreadgroups",
        )?;
        // SAFETY: Selector availability and SDK MTLSize ABI types are used.
        unsafe { msg_send![object, dispatchThreadgroups: groups, threadsPerThreadgroup: threads] }
        Ok(())
    }

    /// Dispatches a grid expressed as individual threads.
    pub(crate) fn dispatch_threads(
        &self,
        threads_per_grid: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        let grid = validate_size(threads_per_grid, "dispatchThreads")?;
        let group = validate_size(threads_per_group, "dispatchThreads")?;
        let object = self.as_inner();
        require_selector(
            object,
            sel!(dispatchThreads:threadsPerThreadgroup:),
            "MTL4::ComputeCommandEncoder::dispatchThreads",
        )?;
        // SAFETY: Selector availability and SDK MTLSize ABI types are used.
        unsafe { msg_send![object, dispatchThreads: grid, threadsPerThreadgroup: group] }
        Ok(())
    }

    /// Configures threadgroup memory for an argument-table index.
    pub(crate) fn set_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "threadgroup memory index must be below 31",
            ));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setThreadgroupMemoryLength:atIndex:),
            "MTL4::ComputeCommandEncoder::setThreadgroupMemoryLength",
        )?;
        // SAFETY: Selector availability and scalar NSUInteger ABI are checked.
        unsafe { msg_send![object, setThreadgroupMemoryLength: length, atIndex: index] }
        Ok(())
    }

    /// Sets the imageblock dimensions.
    pub(crate) fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "imageblock dimensions must be non-zero",
            ));
        }
        let object = self.as_inner();
        require_selector(
            object,
            sel!(setImageblockWidth:height:),
            "MTL4::ComputeCommandEncoder::setImageblockWidth",
        )?;
        // SAFETY: Selector availability and the two-NSUInteger/void ABI are checked.
        unsafe { msg_send![object, setImageblockWidth: width, height: height] }
        Ok(())
    }

    /// Returns the encoder stage mask, preserving system-defined future bits.
    pub(crate) fn stages(&self) -> Result<Stages, Error> {
        let object = self.as_inner();
        require_selector(object, sel!(stages), "MTL4::ComputeCommandEncoder::stages")?;
        // SAFETY: Selector availability and NSUInteger return ABI are checked.
        let raw = unsafe { msg_send![object, stages] };
        Ok(Stages::from_system_raw(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_dimensions_must_be_non_zero() {
        assert!(validate_size(Size::new(1, 1, 1), "dispatch").is_ok());
        assert!(validate_size(Size::new(1, 0, 1), "dispatch").is_err());
    }

    #[test]
    fn viewport_values_are_finite_and_ordered() {
        assert!(validate_viewport(Viewport::new(0.0, 0.0, 64.0, 32.0, 0.0, 1.0)).is_ok());
        assert!(validate_viewport(Viewport::new(0.0, 0.0, f64::NAN, 32.0, 0.0, 1.0)).is_err());
        assert!(validate_viewport(Viewport::new(0.0, 0.0, 64.0, 32.0, 1.0, 0.0)).is_err());
    }

    #[test]
    fn checked_ranges_reject_overflow_and_reversal() {
        assert_eq!(
            checked_range(16, 4..12, "test").unwrap(),
            NSRange::new(4, 8)
        );
        assert!(checked_range(16, std::ops::Range { start: 12, end: 4 }, "test").is_err());
        assert!(checked_range(16, 4..17, "test").is_err());
    }

    #[test]
    fn index_types_define_required_alignment() {
        assert_eq!(
            checked_index_type(IndexType::IndexTypeUInt16, "test")
                .unwrap()
                .1,
            2
        );
        assert_eq!(
            checked_index_type(IndexType::IndexTypeUInt32, "test")
                .unwrap()
                .1,
            4
        );
    }

    #[test]
    fn readbacks_are_bound_to_their_submission_batch() {
        assert!(submission_contains(&[3, 5], 5));
        assert!(!submission_contains(&[3, 5], 4));
    }
}
