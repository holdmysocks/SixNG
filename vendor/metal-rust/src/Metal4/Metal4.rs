//! Safe interfaces corresponding to Apple's Metal 4 namespace.

#[path = "MTL4AccelerationStructureSafe.rs"]
mod mtl4_acceleration_structure_safe;
#[path = "MTL4ArchiveArgumentSafe.rs"]
mod mtl4_archive_argument_safe;
#[path = "MTL4Command.rs"]
mod mtl4_command;
#[path = "MTL4CompilerSafe.rs"]
mod mtl4_compiler_safe;
#[path = "MTL4PipelineDescriptorsSafe.rs"]
mod mtl4_pipeline_descriptors_safe;

pub use mtl4_acceleration_structure_safe::{
    AccelerationStructureBufferRange, AccelerationStructureGeometry,
};
pub use mtl4_archive_argument_safe::{
    Archive, ArgumentTable, ArgumentTableDescriptor, ArgumentTableResource,
};
pub use mtl4_command::{
    AvailableCommandBuffer, BufferImageLayout, CommandQueue, CompletedCommandBuffers,
    EndedCommandBuffer, IndexedDraw, RecordingCommandBuffer, RecordingComputeEncoder,
    RecordingMachineLearningEncoder, RecordingRenderEncoder, SubmittedCommandBuffers,
    TensorCopyRegion, TextureOriginSelection, TextureRegionSelection, TextureSubresourceSpan,
};
pub use mtl4_compiler_safe::RenderPipelineBuildDescriptor;

pub use crate::generated_facade::metal4::*;
pub use crate::metal::{MAX_TIMESTAMP_COUNTERS, TimestampCounterHeap};
