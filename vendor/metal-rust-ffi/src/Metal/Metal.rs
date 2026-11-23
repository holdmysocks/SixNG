#[path = "MTLGeneratedAliasTypes.rs"]
pub mod generated_alias_types;
#[path = "MTLGeneratedObjectTypes.rs"]
pub mod generated_object_types;
#[path = "MTLGeneratedStructTypes.rs"]
pub mod generated_struct_types;
#[path = "MTLGeneratedValueTypes.rs"]
pub mod generated_value_types;
#[path = "MTL4AccelerationStructureSafe.rs"]
mod mtl4_acceleration_structure_safe;
#[path = "MTL4ArchiveArgumentSafe.rs"]
mod mtl4_archive_argument_safe;
#[path = "MTL4Command.rs"]
mod mtl4_command;
#[path = "MTL4CompilerSafe.rs"]
mod mtl4_compiler_safe;
#[path = "MTL4Counters.rs"]
mod mtl4_counters;
#[path = "MTL4PipelineDescriptorsSafe.rs"]
mod mtl4_pipeline_descriptors_safe;
#[path = "MTLAccelerationStructureSafe.rs"]
mod mtl_acceleration_structure_safe;
#[path = "MTLArgumentIndirect.rs"]
mod mtl_argument_indirect;
#[path = "MTLArgumentMetadataSafe.rs"]
mod mtl_argument_metadata_safe;
#[path = "MTLAuxiliaryEncoders.rs"]
mod mtl_auxiliary_encoders;
#[path = "MTLBlitCommandEncoder.rs"]
mod mtl_blit_command_encoder;
#[path = "MTLBuffer.rs"]
mod mtl_buffer;
#[path = "MTLCaptureManager.rs"]
mod mtl_capture_manager;
#[path = "MTLCommandBuffer.rs"]
mod mtl_command_buffer;
#[path = "MTLCommandEncoder.rs"]
mod mtl_command_encoder;
#[path = "MTLCommandQueue.rs"]
mod mtl_command_queue;
#[path = "MTLConstants.rs"]
mod mtl_constants;
#[path = "MTLDevice.rs"]
mod mtl_device;
#[path = "MTLEventDrawableSafe.rs"]
mod mtl_event_drawable_safe;
#[path = "MTLGlobalSafe.rs"]
mod mtl_global_safe;
#[path = "MTLIOCommand.rs"]
mod mtl_io_command;
#[path = "MTLLibraryAdvanced.rs"]
mod mtl_library_advanced;
#[path = "MTLPipelineDescriptorsSafe.rs"]
mod mtl_pipeline_descriptors_safe;
#[path = "MTLRenderPass.rs"]
mod mtl_render_pass;
#[path = "MTLResourceAdvanced.rs"]
mod mtl_resource_advanced;
#[path = "MTLResourceTailSafe.rs"]
mod mtl_resource_tail_safe;
#[path = "MTLShader.rs"]
mod mtl_shader;
#[path = "MTLTensorSafe.rs"]
mod mtl_tensor_safe;
#[path = "MTLTexture.rs"]
mod mtl_texture;
#[path = "MTLTextureViewPoolSafe.rs"]
mod mtl_texture_view_pool_safe;
#[path = "MTLTypes.rs"]
mod mtl_types;
#[path = "MTLVertexStageDescriptorsSafe.rs"]
mod mtl_vertex_stage_descriptors_safe;

pub use mtl_acceleration_structure_safe::{
    ACCELERATION_STRUCTURE_SAMPLE_ATTACHMENT_CAPACITY, AccelerationStructureBuildDescriptor,
    AccelerationStructureEncoder, AccelerationStructureGeometry, AccelerationStructureResource,
};
pub use mtl_argument_metadata_safe::{ArgumentDetails, ArgumentMetadata};
pub use mtl_auxiliary_encoders::{
    ParallelRenderCommandEncoder, PassSampleBufferAttachmentIndex, ResourceStateCommandEncoder,
    SparseTextureMapping,
};
pub use mtl_blit_command_encoder::BlitCommandEncoder;
pub use mtl_buffer::Buffer;
pub use mtl_capture_manager::CaptureSession;
pub use mtl_command_buffer::{
    BufferReadback, CommandBuffer, CompletedCommandBuffer, SubmittedCommandBuffer, TextureReadback,
    TextureReadbackData,
};
pub use mtl_command_encoder::{
    ComputeCommandEncoder, ComputeResource, RenderCommandEncoder, RenderResource,
};
pub use mtl_command_queue::CommandQueue;
pub use mtl_constants::{MetalStringConstant, metal_string_constant};
pub use mtl_device::{Device, DeviceCapability, DeviceNumericProperty};
pub use mtl_event_drawable_safe::SharedEventNotificationRegistration;
pub use mtl_global_safe::{
    DeviceObserverRegistration, LogHandlerRegistration, METAL_CPP_VERSION_MAJOR,
    METAL_CPP_VERSION_MINOR, METAL_CPP_VERSION_PATCH, all_devices, all_devices_with_observer,
    metal_cpp_supports_version,
};
pub use mtl_io_command::{
    CompletedIoCommandBuffer, IoCommandBuffer, IoCommandQueue, IoCompressionContext, IoFileHandle,
    SubmittedIoCommandBuffer,
};
pub use mtl_library_advanced::{StitchingAttributeRef, StitchingNodeRef};
pub use mtl_render_pass::{ComputePassDescriptor, RenderPassDescriptor, RenderPassOptions};
pub use mtl_resource_advanced::ResidencyAllocation;
pub use mtl_resource_tail_safe::ResourceOwnerIdentity;
pub use mtl_shader::{
    CompileOptions, ComputePipelineState, Function, Library, RenderPipelineDescriptor,
    RenderPipelineOptions, RenderPipelineState,
};
pub use mtl_tensor_safe::{
    CheckedTensorBufferAttachments, CheckedTensorDescriptor, CheckedTensorExtents, MAX_TENSOR_RANK,
    TensorBufferAttachment, TensorLayout,
};
pub use mtl_texture::{IoSurface, Texture, TextureDescriptor};
pub use mtl_types::{
    ClearColor, CommandBufferStatus, Origin, PixelFormat, PrimitiveType, Region, ResourceOptions,
    Size, StorageMode, TextureType, TextureUsage, Viewport,
};
pub use mtl4_acceleration_structure_safe::{
    AccelerationStructureBufferRange, Metal4AccelerationStructureGeometry,
};
pub use mtl4_archive_argument_safe::{
    Archive as Mtl4Archive, ArgumentTable as Mtl4ArgumentTable,
    ArgumentTableDescriptor as Mtl4ArgumentTableDescriptor,
    ArgumentTableResource as Mtl4ArgumentTableResource,
};
pub use mtl4_command::{
    AvailableCommandBuffer, BufferImageLayout, CompletedCommandBuffers, EndedCommandBuffer,
    IndexedDraw, Metal4CommandQueue, RecordingCommandBuffer, RecordingComputeEncoder,
    RecordingMachineLearningEncoder, RecordingRenderEncoder, SubmittedCommandBuffers,
    TensorCopyRegion, TextureOriginSelection, TextureRegionSelection, TextureSubresourceSpan,
};
pub use mtl4_compiler_safe::RenderPipelineBuildDescriptor;
pub use mtl4_counters::{CounterReadback, MAX_TIMESTAMP_COUNTERS, TimestampCounterHeap};
