#![allow(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(missing_docs)]

//! Audited Objective-C interoperability boundary for `metal-rust`.
//!
//! This crate is published only because Cargo registries resolve workspace
//! dependencies as separate packages. Applications should depend on
//! `metal-rust`. Its directory and file layout follows metal-cpp while
//! converting generated Objective-C bindings into the narrow safe interface
//! consumed by the public crate. No raw Objective-C binding is re-exported.
//!
//! The implementation package does not offer an alternate unsynchronized
//! resource-readback path:
//!
//! ```compile_fail
//! let device = metal_rust_ffi::Device::system_default().unwrap();
//! let buffer = device
//!     .new_buffer(16, metal_rust_ffi::ResourceOptions::SHARED)
//!     .unwrap();
//! let _bytes = buffer.read(0, 16);
//! ```
//!
//! ```compile_fail
//! let device = metal_rust_ffi::Device::system_default().unwrap();
//! let descriptor = metal_rust_ffi::TextureDescriptor::new_2d(
//!     metal_rust_ffi::PixelFormat::RGBA8_UNORM,
//!     1,
//!     1,
//!     metal_rust_ffi::ResourceOptions::SHARED,
//!     metal_rust_ffi::TextureUsage::SHADER_READ,
//! )
//! .unwrap();
//! let texture = device.new_texture(&descriptor).unwrap();
//! let region = metal_rust_ffi::Region::new(
//!     metal_rust_ffi::Origin::new(0, 0, 0),
//!     metal_rust_ffi::Size::new(1, 1, 1),
//! );
//! let _bytes = texture.read_region(region, 4);
//! ```
//!
//! ```compile_fail
//! let device = metal_rust_ffi::Device::system_default().unwrap();
//! let heap = device.new_timestamp_counter_heap(1).unwrap();
//! let _timestamps = heap.resolve(0..1);
//! ```

use std::marker::PhantomData;
use std::rc::Rc;

/// Marks an Objective-C wrapper as thread-bound unless it is explicitly audited.
///
/// The FFI boundary intentionally defaults every owned object wrapper to
/// `!Send + !Sync`. A type may opt out only with a per-type implementation that
/// cites Apple's thread-safety contract and states why every exposed operation
/// preserves it.
#[derive(Clone, Copy, Default)]
pub(crate) struct ThreadBound(PhantomData<Rc<()>>);

impl ThreadBound {
    pub(crate) const fn new() -> Self {
        Self(PhantomData)
    }
}

#[path = "Foundation/Foundation.rs"]
mod foundation;
#[path = "Metal/Metal.rs"]
mod metal;
#[path = "MetalFX/MetalFX.rs"]
mod metal_fx;
#[path = "QuartzCore/QuartzCore.rs"]
mod quartz_core;

pub use foundation::{
    Bundle, Error, Notification, NotificationCenter, ProcessInfo, Registration, URL,
    device_certification_iphone_performance_gaming, process_performance_profile_default,
    process_performance_profile_sustained,
};
/// Implementation details shared exclusively with the safe facade crate.
///
/// This module is public only because `metal-rust` and `metal-rust-ffi` are
/// separate packages. Applications must use the canonical framework modules
/// exported by `metal-rust` instead of depending on these mechanical types.
#[doc(hidden)]
pub mod __private {
    /// Generated ownership/helper aliases used by the facade implementation.
    #[doc(hidden)]
    pub use crate::metal::generated_alias_types as aliases;
    /// Generated Objective-C object wrappers used by the facade implementation.
    #[doc(hidden)]
    pub use crate::metal::generated_object_types as objects;
    /// Generated C-compatible structures used by the facade implementation.
    #[doc(hidden)]
    pub use crate::metal::generated_struct_types as structs;
    /// Generated scalar and option-set wrappers used by the facade implementation.
    #[doc(hidden)]
    pub use crate::metal::generated_value_types as values;
}
pub use metal::{
    ACCELERATION_STRUCTURE_SAMPLE_ATTACHMENT_CAPACITY, AccelerationStructureBufferRange,
    AccelerationStructureBuildDescriptor, AccelerationStructureEncoder,
    AccelerationStructureGeometry, AccelerationStructureResource, ArgumentDetails,
    ArgumentMetadata, AvailableCommandBuffer, BlitCommandEncoder, Buffer, BufferImageLayout,
    BufferReadback, CaptureSession, CheckedTensorBufferAttachments, CheckedTensorDescriptor,
    CheckedTensorExtents, ClearColor, CommandBuffer, CommandBufferStatus, CommandQueue,
    CompileOptions, CompletedCommandBuffer, CompletedCommandBuffers, CompletedIoCommandBuffer,
    ComputeCommandEncoder, ComputePassDescriptor, ComputePipelineState, ComputeResource,
    CounterReadback, Device, DeviceCapability, DeviceNumericProperty, DeviceObserverRegistration,
    EndedCommandBuffer, Function, IndexedDraw, IoCommandBuffer, IoCommandQueue,
    IoCompressionContext, IoFileHandle, IoSurface, Library, LogHandlerRegistration,
    MAX_TENSOR_RANK, MAX_TIMESTAMP_COUNTERS, METAL_CPP_VERSION_MAJOR, METAL_CPP_VERSION_MINOR,
    METAL_CPP_VERSION_PATCH, Metal4AccelerationStructureGeometry, Metal4CommandQueue,
    MetalStringConstant, Mtl4Archive, Mtl4ArgumentTable, Mtl4ArgumentTableDescriptor,
    Mtl4ArgumentTableResource, Origin, ParallelRenderCommandEncoder,
    PassSampleBufferAttachmentIndex, PixelFormat, PrimitiveType, RecordingCommandBuffer,
    RecordingComputeEncoder, RecordingMachineLearningEncoder, RecordingRenderEncoder, Region,
    RenderCommandEncoder, RenderPassDescriptor, RenderPassOptions, RenderPipelineBuildDescriptor,
    RenderPipelineDescriptor, RenderPipelineOptions, RenderPipelineState, RenderResource,
    ResidencyAllocation, ResourceOptions, ResourceOwnerIdentity, ResourceStateCommandEncoder,
    SharedEventNotificationRegistration, Size, SparseTextureMapping, StitchingAttributeRef,
    StitchingNodeRef, StorageMode, SubmittedCommandBuffer, SubmittedCommandBuffers,
    SubmittedIoCommandBuffer, TensorBufferAttachment, TensorCopyRegion, TensorLayout, Texture,
    TextureDescriptor, TextureOriginSelection, TextureReadback, TextureReadbackData,
    TextureRegionSelection, TextureSubresourceSpan, TextureType, TextureUsage,
    TimestampCounterHeap, Viewport, all_devices, all_devices_with_observer,
    metal_cpp_supports_version, metal_string_constant,
};
pub use metal_fx::{
    Matrix4x4, MatrixValidationError, SpatialScaler, SpatialScalerColorProcessingMode,
    SpatialScalerDescriptor, TemporalScaler, TemporalScalerDescriptor,
};
pub use quartz_core::{ColorSpace, Drawable, Layer};
