//! Safe facade extensions for Metal 4 command recording.

use crate::metal::{
    CullMode, DepthClipMode, IndexType, PrimitiveType, RenderStages, ScissorRect, Size, Stages,
    StoreAction, TriangleFillMode, Viewport, VisibilityResultMode, Winding,
};

use super::{
    ArgumentTable, CommandAllocator, CommandBufferOptions, RenderEncoderOptions,
    RenderPassDescriptor, VisibilityOptions,
};

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
    pub region: crate::metal::Region,
}
/// A checked texture destination origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureOriginSelection {
    /// Array slice.
    pub slice: usize,
    /// Mip level.
    pub level: usize,
    /// Origin within the subresource.
    pub origin: crate::metal::Origin,
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
    pub source_origin: &'a crate::metal::CheckedTensorExtents,
    /// Source dimensions.
    pub source_dimensions: &'a crate::metal::CheckedTensorExtents,
    /// Destination origin.
    pub destination_origin: &'a crate::metal::CheckedTensorExtents,
    /// Destination dimensions.
    pub destination_dimensions: &'a crate::metal::CheckedTensorExtents,
}

/// A safe reusable Metal 4 command queue.
#[derive(Clone)]
pub struct CommandQueue {
    inner: metal_rust_ffi::Metal4CommandQueue,
}

impl CommandQueue {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Metal4CommandQueue) -> Self {
        Self { inner }
    }

    pub(crate) const fn as_ffi(&self) -> &metal_rust_ffi::Metal4CommandQueue {
        &self.inner
    }

    /// Returns the owning device.
    pub fn device(&self) -> Result<crate::metal::Device, crate::Error> {
        self.inner
            .device()
            .map(crate::metal::Device::from_ffi)
            .map_err(crate::Error::from_ffi)
    }

    /// Returns the optional queue label.
    pub fn label(&self) -> Result<Option<String>, crate::Error> {
        self.inner.label().map_err(crate::Error::from_ffi)
    }
}

impl crate::metal::Device {
    /// Creates a non-cloneable Metal 4 command buffer in the available state.
    pub fn new_mtl4_command_buffer(&self) -> Result<AvailableCommandBuffer, crate::Error> {
        self.inner
            .new_mtl4_command_buffer()
            .map(AvailableCommandBuffer::from_ffi)
            .map_err(crate::Error::from_ffi)
    }

    /// Creates a reusable Metal 4 command queue.
    pub fn new_mtl4_command_queue(&self) -> Result<CommandQueue, crate::Error> {
        self.inner
            .new_mtl4_command_queue()
            .map(CommandQueue::from_ffi)
            .map_err(crate::Error::from_ffi)
    }
}

/// A non-cloneable Metal 4 command buffer that has not begun recording.
///
/// ```compile_fail
/// fn duplicate(buffer: metal_rust::metal4::AvailableCommandBuffer) {
///     let _second_owner = buffer.clone();
/// }
/// ```
///
/// The cloneable generated command-buffer shell is intentionally not part of
/// the canonical Metal 4 API:
///
/// ```compile_fail
/// fn bypass_typestate(_: metal_rust::metal4::CommandBuffer) {}
/// ```
pub struct AvailableCommandBuffer {
    inner: metal_rust_ffi::AvailableCommandBuffer,
}

impl AvailableCommandBuffer {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::AvailableCommandBuffer) -> Self {
        Self { inner }
    }

    /// Begins one recording scope and consumes the available state.
    pub fn begin(
        self,
        allocator: &CommandAllocator,
        options: Option<&CommandBufferOptions>,
    ) -> Result<RecordingCommandBuffer, crate::Error> {
        self.inner
            .begin(&allocator.inner, options.map(|value| &value.inner))
            .map(|inner| RecordingCommandBuffer { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Returns the owning device when available.
    pub fn device(&self) -> Result<crate::metal::Device, crate::Error> {
        self.inner
            .device()
            .map(crate::metal::Device::from_ffi)
            .map_err(crate::Error::from_ffi)
    }

    /// Returns the optional label.
    pub fn label(&self) -> Result<Option<String>, crate::Error> {
        self.inner.label().map_err(crate::Error::from_ffi)
    }

    /// Sets or clears the command-buffer label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), crate::Error> {
        self.inner.set_label(label).map_err(crate::Error::from_ffi)
    }
}

/// A non-cloneable Metal 4 command buffer in its recording state.
///
/// An encoder exclusively borrows the recording state, so ending the command
/// buffer while the encoder remains live does not compile:
///
/// ```compile_fail
/// fn end_while_encoding(
///     mut buffer: metal_rust::metal4::RecordingCommandBuffer,
///     descriptor: &metal_rust::metal4::RenderPassDescriptor,
/// ) {
///     let encoder = buffer.render_encoder(descriptor, None).unwrap();
///     let _ended = buffer.end();
///     let _keep_alive = encoder;
/// }
/// ```
pub struct RecordingCommandBuffer {
    inner: metal_rust_ffi::RecordingCommandBuffer,
}

impl RecordingCommandBuffer {
    /// Borrows the internal recording state for crate-private safe adapters.
    pub(crate) fn as_ffi_mut(&mut self) -> &mut metal_rust_ffi::RecordingCommandBuffer {
        &mut self.inner
    }

    /// Returns the owning device.
    pub fn device(&self) -> Result<crate::metal::Device, crate::Error> {
        self.inner
            .device()
            .map(crate::metal::Device::from_ffi)
            .map_err(crate::Error::from_ffi)
    }

    /// Returns the optional command-buffer label.
    pub fn label(&self) -> Result<Option<String>, crate::Error> {
        self.inner.label().map_err(crate::Error::from_ffi)
    }

    /// Sets or clears the command-buffer label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), crate::Error> {
        self.inner.set_label(label).map_err(crate::Error::from_ffi)
    }

    /// Creates an exclusively borrowed compute encoder.
    pub fn compute_encoder(&mut self) -> Result<RecordingComputeEncoder<'_>, crate::Error> {
        self.inner
            .compute_encoder()
            .map(|inner| RecordingComputeEncoder { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Creates an exclusively borrowed render encoder.
    pub fn render_encoder(
        &mut self,
        descriptor: &RenderPassDescriptor,
        options: Option<RenderEncoderOptions>,
    ) -> Result<RecordingRenderEncoder<'_>, crate::Error> {
        self.inner
            .render_encoder(&descriptor.inner, options)
            .map(|inner| RecordingRenderEncoder { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Creates an exclusively borrowed machine-learning encoder.
    pub fn machine_learning_encoder(
        &mut self,
    ) -> Result<RecordingMachineLearningEncoder<'_>, crate::Error> {
        self.inner
            .machine_learning_encoder()
            .map(|inner| RecordingMachineLearningEncoder { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Makes one residency set available to this recording.
    pub fn use_residency_set(&self, set: &crate::metal::ResidencySet) -> Result<(), crate::Error> {
        self.inner
            .use_residency_set(&set.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Makes a non-empty residency-set slice available to this recording.
    pub fn use_residency_sets(
        &self,
        sets: &[&crate::metal::ResidencySet],
    ) -> Result<(), crate::Error> {
        let sets: Vec<_> = sets.iter().map(|set| &set.inner).collect();
        self.inner
            .use_residency_sets(&sets)
            .map_err(crate::Error::from_ffi)
    }

    /// Pushes a command-buffer debug group.
    pub fn push_debug_group(&self, name: &str) -> Result<(), crate::Error> {
        self.inner
            .push_debug_group(name)
            .map_err(crate::Error::from_ffi)
    }

    /// Pops a command-buffer debug group.
    pub fn pop_debug_group(&self) -> Result<(), crate::Error> {
        self.inner.pop_debug_group().map_err(crate::Error::from_ffi)
    }

    /// Encodes a submission-bound timestamp-counter readback.
    pub fn resolve_counter_heap(
        &self,
        heap: crate::metal::TimestampCounterHeap,
        range: std::ops::Range<usize>,
    ) -> Result<crate::metal::CounterReadback, crate::Error> {
        self.inner
            .resolve_counter_heap(heap.inner, range)
            .map(|inner| crate::metal::CounterReadback { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Writes a command-buffer boundary timestamp into a checked heap entry.
    pub fn write_timestamp_into_heap(
        &self,
        heap: &crate::metal::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .write_timestamp_into_heap(&heap.inner, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Ends recording and consumes this state.
    pub fn end(self) -> Result<EndedCommandBuffer, crate::Error> {
        self.inner
            .end()
            .map(|inner| EndedCommandBuffer { inner })
            .map_err(crate::Error::from_ffi)
    }
}

/// A non-cloneable Metal 4 command buffer ready for one submission.
///
/// Submission consumes this state, preventing a second commit:
///
/// ```compile_fail
/// fn submit_twice(
///     queue: &metal_rust::metal4::CommandQueue,
///     buffer: metal_rust::metal4::EndedCommandBuffer,
/// ) {
///     let _first = queue.submit(vec![buffer], None);
///     let _second = queue.submit(vec![buffer], None);
/// }
/// ```
pub struct EndedCommandBuffer {
    inner: metal_rust_ffi::EndedCommandBuffer,
}

/// Proof that a batch of Metal 4 command buffers was submitted once.
pub struct SubmittedCommandBuffers {
    inner: metal_rust_ffi::SubmittedCommandBuffers,
}

/// Proof that a Metal 4 submission batch completed successfully.
pub struct CompletedCommandBuffers {
    inner: metal_rust_ffi::CompletedCommandBuffers,
}

impl SubmittedCommandBuffers {
    /// Returns the submitted batch size.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// Waits for Metal completion without requiring an async runtime.
    pub fn wait(self) -> Result<CompletedCommandBuffers, crate::Error> {
        self.inner
            .wait()
            .map(|inner| CompletedCommandBuffers { inner })
            .map_err(crate::Error::from_ffi)
    }
}

impl CompletedCommandBuffers {
    /// Returns the completed batch size.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// Resolves an identity-matched counter readback and returns heap ownership.
    pub fn resolve_counters(
        &self,
        readback: crate::metal::CounterReadback,
    ) -> Result<(crate::metal::TimestampCounterHeap, Vec<u64>), crate::Error> {
        self.inner
            .resolve_counters(readback.inner)
            .map(|(heap, values)| (crate::metal::TimestampCounterHeap::from_ffi(heap), values))
            .map_err(crate::Error::from_ffi)
    }
}

/// An exclusively borrowed Metal 4 compute encoder.
pub struct RecordingComputeEncoder<'a> {
    inner: metal_rust_ffi::RecordingComputeEncoder<'a>,
}

impl RecordingComputeEncoder<'_> {
    /// Dispatches a grid expressed in threadgroups.
    pub fn dispatch_threadgroups(&self, groups: Size, threads: Size) -> Result<(), crate::Error> {
        self.inner
            .dispatch_threadgroups(groups, threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Dispatches a grid expressed as individual threads.
    pub fn dispatch_threads(&self, grid: Size, threads: Size) -> Result<(), crate::Error> {
        self.inner
            .dispatch_threads(grid, threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Dispatches threadgroups using checked indirect arguments.
    pub fn dispatch_threadgroups_indirect(
        &self,
        buffer: &crate::metal::Buffer,
        offset: usize,
        threads: Size,
    ) -> Result<(), crate::Error> {
        self.inner
            .dispatch_threadgroups_indirect(&buffer.inner, offset, threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Dispatches threads using checked indirect arguments.
    pub fn dispatch_threads_indirect(
        &self,
        buffer: &crate::metal::Buffer,
        offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .dispatch_threads_indirect(&buffer.inner, offset)
            .map_err(crate::Error::from_ffi)
    }

    /// Executes a checked indirect-command range.
    pub fn execute_commands(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .execute_commands(&commands.inner, range)
            .map_err(crate::Error::from_ffi)
    }

    /// Executes a range encoded in a checked indirect buffer.
    pub fn execute_commands_indirect(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        buffer: &crate::metal::Buffer,
        offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .execute_commands_indirect(&commands.inner, &buffer.inner, offset)
            .map_err(crate::Error::from_ffi)
    }

    /// Fills a checked buffer range.
    pub fn fill_buffer(
        &self,
        buffer: &crate::metal::Buffer,
        range: std::ops::Range<usize>,
        value: u8,
    ) -> Result<(), crate::Error> {
        self.inner
            .fill_buffer(&buffer.inner, range, value)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked buffer range.
    pub fn copy_buffer(
        &self,
        source: &crate::metal::Buffer,
        source_range: std::ops::Range<usize>,
        destination: &crate::metal::Buffer,
        destination_offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_buffer(
                &source.inner,
                source_range,
                &destination.inner,
                destination_offset,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Generates texture mipmaps.
    pub fn generate_mipmaps(&self, texture: &crate::metal::Texture) -> Result<(), crate::Error> {
        self.inner
            .generate_mipmaps(&texture.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies all compatible texture subresources.
    pub fn copy_texture(
        &self,
        source: &crate::metal::Texture,
        destination: &crate::metal::Texture,
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_texture(&source.inner, &destination.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies checked texture slice and mip-level spans.
    pub fn copy_texture_levels(
        &self,
        source: &crate::metal::Texture,
        source_span: TextureSubresourceSpan,
        destination: &crate::metal::Texture,
        destination_slice: usize,
        destination_level: usize,
    ) -> Result<(), crate::Error> {
        let source_span = metal_rust_ffi::TextureSubresourceSpan {
            slice: source_span.slice,
            level: source_span.level,
            slice_count: source_span.slice_count,
            level_count: source_span.level_count,
        };
        self.inner
            .copy_texture_levels(
                &source.inner,
                source_span,
                &destination.inner,
                destination_slice,
                destination_level,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked texture region into another texture.
    pub fn copy_texture_region(
        &self,
        source: &crate::metal::Texture,
        source_region: TextureRegionSelection,
        destination: &crate::metal::Texture,
        destination_origin: TextureOriginSelection,
    ) -> Result<(), crate::Error> {
        let source_region = metal_rust_ffi::TextureRegionSelection {
            slice: source_region.slice,
            level: source_region.level,
            region: source_region.region,
        };
        let destination_origin = metal_rust_ffi::TextureOriginSelection {
            slice: destination_origin.slice,
            level: destination_origin.level,
            origin: destination_origin.origin,
        };
        self.inner
            .copy_texture_region(
                &source.inner,
                source_region,
                &destination.inner,
                destination_origin,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked texture region into a checked buffer layout.
    pub fn copy_texture_to_buffer(
        &self,
        source: &crate::metal::Texture,
        source_region: TextureRegionSelection,
        destination: &crate::metal::Buffer,
        layout: BufferImageLayout,
        options: Option<crate::metal::BlitOption>,
    ) -> Result<(), crate::Error> {
        let source_region = metal_rust_ffi::TextureRegionSelection {
            slice: source_region.slice,
            level: source_region.level,
            region: source_region.region,
        };
        let layout = metal_rust_ffi::BufferImageLayout {
            offset: layout.offset,
            bytes_per_row: layout.bytes_per_row,
            bytes_per_image: layout.bytes_per_image,
        };
        self.inner
            .copy_texture_to_buffer(
                &source.inner,
                source_region,
                &destination.inner,
                layout,
                options,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked buffer layout into a checked texture region.
    pub fn copy_buffer_to_texture(
        &self,
        source: &crate::metal::Buffer,
        layout: BufferImageLayout,
        source_size: Size,
        destination: &crate::metal::Texture,
        destination_origin: TextureOriginSelection,
        options: Option<crate::metal::BlitOption>,
    ) -> Result<(), crate::Error> {
        let layout = metal_rust_ffi::BufferImageLayout {
            offset: layout.offset,
            bytes_per_row: layout.bytes_per_row,
            bytes_per_image: layout.bytes_per_image,
        };
        let destination_origin = metal_rust_ffi::TextureOriginSelection {
            slice: destination_origin.slice,
            level: destination_origin.level,
            origin: destination_origin.origin,
        };
        self.inner
            .copy_buffer_to_texture(
                &source.inner,
                layout,
                source_size,
                &destination.inner,
                destination_origin,
                options,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Optimizes all texture contents for GPU access.
    pub fn optimize_texture_for_gpu(
        &self,
        texture: &crate::metal::Texture,
    ) -> Result<(), crate::Error> {
        self.inner
            .optimize_texture_for_gpu(&texture.inner)
            .map_err(crate::Error::from_ffi)
    }
    /// Optimizes one texture subresource for GPU access.
    pub fn optimize_texture_level_for_gpu(
        &self,
        texture: &crate::metal::Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .optimize_texture_level_for_gpu(&texture.inner, slice, level)
            .map_err(crate::Error::from_ffi)
    }
    /// Optimizes all texture contents for CPU access.
    pub fn optimize_texture_for_cpu(
        &self,
        texture: &crate::metal::Texture,
    ) -> Result<(), crate::Error> {
        self.inner
            .optimize_texture_for_cpu(&texture.inner)
            .map_err(crate::Error::from_ffi)
    }
    /// Optimizes one texture subresource for CPU access.
    pub fn optimize_texture_level_for_cpu(
        &self,
        texture: &crate::metal::Texture,
        slice: usize,
        level: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .optimize_texture_level_for_cpu(&texture.inner, slice, level)
            .map_err(crate::Error::from_ffi)
    }

    /// Resets a checked indirect-command range.
    pub fn reset_commands(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .reset_commands(&commands.inner, range)
            .map_err(crate::Error::from_ffi)
    }

    /// Optimizes a checked indirect-command range.
    pub fn optimize_indirect_commands(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .optimize_indirect_commands(&commands.inner, range)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked indirect-command range.
    pub fn copy_indirect_commands(
        &self,
        source: &crate::metal::IndirectCommandBuffer,
        source_range: std::ops::Range<usize>,
        destination: &crate::metal::IndirectCommandBuffer,
        destination_index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_indirect_commands(
                &source.inner,
                source_range,
                &destination.inner,
                destination_index,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Writes a timestamp into a checked counter-heap entry.
    pub fn write_timestamp(
        &self,
        granularity: super::TimestampGranularity,
        heap: &crate::metal::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .write_timestamp(granularity, &heap.inner, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Builds an acceleration structure with a checked scratch range.
    pub fn build_acceleration_structure(
        &self,
        acceleration_structure: &crate::metal::AccelerationStructure,
        descriptor: &super::AccelerationStructureDescriptor,
        scratch: &crate::metal::Buffer,
        scratch_range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .build_acceleration_structure(
                &acceleration_structure.inner,
                &descriptor.inner,
                &scratch.inner,
                scratch_range,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Refits an acceleration structure with a checked scratch range.
    pub fn refit_acceleration_structure(
        &self,
        source: &crate::metal::AccelerationStructure,
        descriptor: &super::AccelerationStructureDescriptor,
        destination: Option<&crate::metal::AccelerationStructure>,
        scratch: &crate::metal::Buffer,
        scratch_range: std::ops::Range<usize>,
        options: Option<crate::metal::AccelerationStructureRefitOptions>,
    ) -> Result<(), crate::Error> {
        self.inner
            .refit_acceleration_structure(
                &source.inner,
                &descriptor.inner,
                destination.map(|value| &value.inner),
                &scratch.inner,
                scratch_range,
                options,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Copies one acceleration structure.
    pub fn copy_acceleration_structure(
        &self,
        source: &crate::metal::AccelerationStructure,
        destination: &crate::metal::AccelerationStructure,
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_acceleration_structure(&source.inner, &destination.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies and compacts one acceleration structure.
    pub fn copy_and_compact_acceleration_structure(
        &self,
        source: &crate::metal::AccelerationStructure,
        destination: &crate::metal::AccelerationStructure,
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_and_compact_acceleration_structure(&source.inner, &destination.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Writes the compacted acceleration-structure size to a checked buffer range.
    pub fn write_compacted_acceleration_structure_size(
        &self,
        acceleration_structure: &crate::metal::AccelerationStructure,
        destination: &crate::metal::Buffer,
        destination_range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .write_compacted_acceleration_structure_size(
                &acceleration_structure.inner,
                &destination.inner,
                destination_range,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Copies a checked tensor region, optionally selecting auxiliary planes.
    pub fn copy_tensor(
        &self,
        source: &crate::metal::Tensor,
        destination: &crate::metal::Tensor,
        region: TensorCopyRegion<'_>,
        planes: Option<(crate::metal::TensorPlaneType, crate::metal::TensorPlaneType)>,
    ) -> Result<(), crate::Error> {
        let region = metal_rust_ffi::TensorCopyRegion {
            source_origin: &region.source_origin.inner,
            source_dimensions: &region.source_dimensions.inner,
            destination_origin: &region.destination_origin.inner,
            destination_dimensions: &region.destination_dimensions.inner,
        };
        self.inner
            .copy_tensor(&source.inner, &destination.inner, region, planes)
            .map_err(crate::Error::from_ffi)
    }

    /// Binds the optional compute argument table.
    pub fn set_argument_table(&self, table: Option<&ArgumentTable>) -> Result<(), crate::Error> {
        self.inner
            .set_argument_table(table.map(|value| &value.inner))
            .map_err(crate::Error::from_ffi)
    }

    /// Binds the compute pipeline state.
    pub fn set_compute_pipeline_state(
        &self,
        state: &crate::metal::ComputePipelineState,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_compute_pipeline_state(&state.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Configures compute threadgroup memory.
    pub fn set_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_threadgroup_memory_length(length, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets imageblock dimensions.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), crate::Error> {
        self.inner
            .set_imageblock_size(width, height)
            .map_err(crate::Error::from_ffi)
    }

    /// Returns the encoder stage mask.
    pub fn stages(&self) -> Result<Stages, crate::Error> {
        self.inner.stages().map_err(crate::Error::from_ffi)
    }

    /// Explicitly ends encoding and releases the exclusive borrow.
    pub fn end(self) -> Result<(), crate::Error> {
        self.inner.end().map_err(crate::Error::from_ffi)
    }
}

/// An exclusively borrowed Metal 4 render encoder.
pub struct RecordingRenderEncoder<'a> {
    inner: metal_rust_ffi::RecordingRenderEncoder<'a>,
}

impl RecordingRenderEncoder<'_> {
    /// Returns the tile width.
    pub fn tile_width(&self) -> Result<usize, crate::Error> {
        self.inner.tile_width().map_err(crate::Error::from_ffi)
    }

    /// Returns the tile height.
    pub fn tile_height(&self) -> Result<usize, crate::Error> {
        self.inner.tile_height().map_err(crate::Error::from_ffi)
    }

    /// Dispatches threads per tile.
    pub fn dispatch_threads_per_tile(&self, threads: Size) -> Result<(), crate::Error> {
        self.inner
            .dispatch_threads_per_tile(threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws indexed primitives from a checked buffer range.
    pub fn draw_indexed_primitives(
        &self,
        primitive: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        buffer: &crate::metal::Buffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_indexed_primitives(primitive, index_count, index_type, &buffer.inner, range)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws instanced indexed primitives from a checked buffer range.
    pub fn draw_indexed_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        buffer: &crate::metal::Buffer,
        range: std::ops::Range<usize>,
        instance_count: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_indexed_primitives_instanced(
                primitive,
                index_count,
                index_type,
                &buffer.inner,
                range,
                instance_count,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Draws indexed primitives with explicit base values.
    pub fn draw_indexed_primitives_base(
        &self,
        primitive: PrimitiveType,
        buffer: &crate::metal::Buffer,
        draw: IndexedDraw,
    ) -> Result<(), crate::Error> {
        let draw = metal_rust_ffi::IndexedDraw {
            index_count: draw.index_count,
            index_type: draw.index_type,
            index_range: draw.index_range,
            instance_count: draw.instance_count,
            base_vertex: draw.base_vertex,
            base_instance: draw.base_instance,
        };
        self.inner
            .draw_indexed_primitives_base(primitive, &buffer.inner, draw)
            .map_err(crate::Error::from_ffi)
    }

    /// Issues a non-indexed indirect draw.
    pub fn draw_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        buffer: &crate::metal::Buffer,
        offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_primitives_indirect(primitive, &buffer.inner, offset)
            .map_err(crate::Error::from_ffi)
    }

    /// Issues an indexed indirect draw using checked buffers.
    pub fn draw_indexed_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        index_type: IndexType,
        index_buffer: &crate::metal::Buffer,
        index_range: std::ops::Range<usize>,
        arguments: &crate::metal::Buffer,
        arguments_offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_indexed_primitives_indirect(
                primitive,
                index_type,
                &index_buffer.inner,
                index_range,
                &arguments.inner,
                arguments_offset,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Draws mesh threadgroups directly.
    pub fn draw_mesh_threadgroups(
        &self,
        groups: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_mesh_threadgroups(groups, object_threads, mesh_threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws mesh threads directly.
    pub fn draw_mesh_threads(
        &self,
        grid: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_mesh_threads(grid, object_threads, mesh_threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws mesh threadgroups using checked indirect arguments.
    pub fn draw_mesh_threadgroups_indirect(
        &self,
        buffer: &crate::metal::Buffer,
        offset: usize,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_mesh_threadgroups_indirect(&buffer.inner, offset, object_threads, mesh_threads)
            .map_err(crate::Error::from_ffi)
    }

    /// Executes a checked indirect-command range.
    pub fn execute_commands(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), crate::Error> {
        self.inner
            .execute_commands(&commands.inner, range)
            .map_err(crate::Error::from_ffi)
    }

    /// Executes a command range read from a checked buffer.
    pub fn execute_commands_indirect(
        &self,
        commands: &crate::metal::IndirectCommandBuffer,
        buffer: &crate::metal::Buffer,
        offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .execute_commands_indirect(&commands.inner, &buffer.inner, offset)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets one scissor rectangle.
    pub fn set_scissor_rect(&self, rect: ScissorRect) -> Result<(), crate::Error> {
        self.inner
            .set_scissor_rect(rect)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets multiple scissor rectangles.
    pub fn set_scissor_rects(&self, rects: &[ScissorRect]) -> Result<(), crate::Error> {
        self.inner
            .set_scissor_rects(rects)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets multiple viewports.
    pub fn set_viewports(&self, viewports: &[Viewport]) -> Result<(), crate::Error> {
        self.inner
            .set_viewports(viewports)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets or clears logical-to-physical color attachment mapping.
    pub fn set_color_attachment_map(
        &self,
        mapping: Option<&crate::metal::LogicalToPhysicalColorAttachmentMap>,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_color_attachment_map(mapping.map(|value| &value.inner))
            .map_err(crate::Error::from_ffi)
    }

    /// Configures vertex amplification with safe mapping values.
    pub fn set_vertex_amplification(
        &self,
        mappings: &[crate::metal::VertexAmplificationViewMapping],
    ) -> Result<(), crate::Error> {
        self.inner
            .set_vertex_amplification(mappings)
            .map_err(crate::Error::from_ffi)
    }

    /// Writes a timestamp after a checked render stage.
    pub fn write_timestamp(
        &self,
        granularity: super::TimestampGranularity,
        stage: RenderStages,
        heap: &crate::metal::TimestampCounterHeap,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .write_timestamp(granularity, stage, &heap.inner, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws a non-indexed primitive range.
    pub fn draw_primitives(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_primitives(primitive, vertex_start, vertex_count)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws instanced non-indexed primitives.
    pub fn draw_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_primitives_instanced(primitive, vertex_start, vertex_count, instance_count)
            .map_err(crate::Error::from_ffi)
    }

    /// Draws instanced non-indexed primitives with a base instance.
    pub fn draw_primitives_instanced_base_instance(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .draw_primitives_instanced_base_instance(
                primitive,
                vertex_start,
                vertex_count,
                instance_count,
                base_instance,
            )
            .map_err(crate::Error::from_ffi)
    }

    /// Sets the constant blend color.
    pub fn set_blend_color(
        &self,
        red: f32,
        green: f32,
        blue: f32,
        alpha: f32,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_blend_color(red, green, blue, alpha)
            .map_err(crate::Error::from_ffi)
    }

    /// Binds an optional argument table to selected render stages.
    pub fn set_argument_table(
        &self,
        table: Option<&ArgumentTable>,
        stages: RenderStages,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_argument_table(table.map(|value| &value.inner), stages)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets face culling mode.
    pub fn set_cull_mode(&self, mode: CullMode) -> Result<(), crate::Error> {
        self.inner
            .set_cull_mode(mode)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets front-face winding.
    pub fn set_front_facing_winding(&self, winding: Winding) -> Result<(), crate::Error> {
        self.inner
            .set_front_facing_winding(winding)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets triangle fill mode.
    pub fn set_triangle_fill_mode(&self, mode: TriangleFillMode) -> Result<(), crate::Error> {
        self.inner
            .set_triangle_fill_mode(mode)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets depth clipping mode.
    pub fn set_depth_clip_mode(&self, mode: DepthClipMode) -> Result<(), crate::Error> {
        self.inner
            .set_depth_clip_mode(mode)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets or clears the depth-stencil state.
    pub fn set_depth_stencil_state(
        &self,
        state: Option<&crate::metal::DepthStencilState>,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_depth_stencil_state(state.map(|value| &value.inner))
            .map_err(crate::Error::from_ffi)
    }

    /// Binds the render pipeline state.
    pub fn set_render_pipeline_state(
        &self,
        state: &crate::metal::RenderPipelineState,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_render_pipeline_state(&state.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets the raster viewport.
    pub fn set_viewport(&self, viewport: Viewport) -> Result<(), crate::Error> {
        self.inner
            .set_viewport(viewport)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets depth bias parameters.
    pub fn set_depth_bias(&self, bias: f32, slope: f32, clamp: f32) -> Result<(), crate::Error> {
        self.inner
            .set_depth_bias(bias, slope, clamp)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets inclusive depth-test bounds.
    pub fn set_depth_test_bounds(&self, minimum: f32, maximum: f32) -> Result<(), crate::Error> {
        self.inner
            .set_depth_test_bounds(minimum, maximum)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets one stencil reference value for both faces.
    pub fn set_stencil_reference_value(&self, value: u32) -> Result<(), crate::Error> {
        self.inner
            .set_stencil_reference_value(value)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets distinct front and back stencil reference values.
    pub fn set_stencil_reference_values(&self, front: u32, back: u32) -> Result<(), crate::Error> {
        self.inner
            .set_stencil_reference_values(front, back)
            .map_err(crate::Error::from_ffi)
    }

    /// Configures visibility-result writes.
    pub fn set_visibility_result_mode(
        &self,
        mode: VisibilityResultMode,
        offset: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_visibility_result_mode(mode, offset)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets a color attachment store action.
    pub fn set_color_store_action(
        &self,
        action: StoreAction,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_color_store_action(action, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets the depth attachment store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), crate::Error> {
        self.inner
            .set_depth_store_action(action)
            .map_err(crate::Error::from_ffi)
    }

    /// Sets the stencil attachment store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), crate::Error> {
        self.inner
            .set_stencil_store_action(action)
            .map_err(crate::Error::from_ffi)
    }

    /// Configures render threadgroup memory.
    pub fn set_threadgroup_memory_length(
        &self,
        length: usize,
        offset: usize,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_threadgroup_memory_length(length, offset, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Configures object-stage threadgroup memory.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_object_threadgroup_memory_length(length, index)
            .map_err(crate::Error::from_ffi)
    }

    /// Explicitly ends encoding and releases the exclusive borrow.
    pub fn end(self) -> Result<(), crate::Error> {
        self.inner.end().map_err(crate::Error::from_ffi)
    }
}

/// An exclusively borrowed Metal 4 machine-learning encoder.
pub struct RecordingMachineLearningEncoder<'a> {
    inner: metal_rust_ffi::RecordingMachineLearningEncoder<'a>,
}

impl RecordingMachineLearningEncoder<'_> {
    /// Binds a machine-learning pipeline state.
    pub fn set_pipeline_state(
        &self,
        state: &super::MachineLearningPipelineState,
    ) -> Result<(), crate::Error> {
        self.inner
            .set_pipeline_state(&state.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Binds or clears the machine-learning argument table.
    pub fn set_argument_table(&self, table: Option<&ArgumentTable>) -> Result<(), crate::Error> {
        self.inner
            .set_argument_table(table.map(|value| &value.inner))
            .map_err(crate::Error::from_ffi)
    }

    /// Dispatches the configured network with a retained intermediate heap.
    pub fn dispatch_network(
        &self,
        intermediates_heap: &crate::metal::Heap,
    ) -> Result<(), crate::Error> {
        self.inner
            .dispatch_network(&intermediates_heap.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Explicitly ends encoding and releases the exclusive borrow.
    pub fn end(self) -> Result<(), crate::Error> {
        self.inner.end().map_err(crate::Error::from_ffi)
    }
}

macro_rules! impl_recording_encoder_common {
    ($type:ident) => {
        impl $type<'_> {
            /// Returns the optional encoder label.
            pub fn label(&self) -> Result<Option<String>, crate::Error> {
                self.inner.label().map_err(crate::Error::from_ffi)
            }

            /// Sets or clears the encoder label.
            pub fn set_label(&self, value: Option<&str>) -> Result<(), crate::Error> {
                self.inner.set_label(value).map_err(crate::Error::from_ffi)
            }

            /// Inserts a debug signpost into the encoder stream.
            pub fn insert_debug_signpost(&self, value: &str) -> Result<(), crate::Error> {
                self.inner
                    .insert_debug_signpost(value)
                    .map_err(crate::Error::from_ffi)
            }

            /// Pushes an encoder debug group.
            pub fn push_debug_group(&self, value: &str) -> Result<(), crate::Error> {
                self.inner
                    .push_debug_group(value)
                    .map_err(crate::Error::from_ffi)
            }

            /// Pops the current encoder debug group.
            pub fn pop_debug_group(&self) -> Result<(), crate::Error> {
                self.inner.pop_debug_group().map_err(crate::Error::from_ffi)
            }

            /// Adds an encoder-to-encoder stage barrier.
            pub fn barrier_after_encoder_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), crate::Error> {
                self.inner
                    .barrier_after_encoder_stages(after, before, visibility)
                    .map_err(crate::Error::from_ffi)
            }

            /// Adds a barrier from earlier queue stages into this encoder.
            pub fn barrier_after_queue_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), crate::Error> {
                self.inner
                    .barrier_after_queue_stages(after, before, visibility)
                    .map_err(crate::Error::from_ffi)
            }

            /// Adds a barrier from this encoder into later queue stages.
            pub fn barrier_after_stages(
                &self,
                after: Stages,
                before: Stages,
                visibility: VisibilityOptions,
            ) -> Result<(), crate::Error> {
                self.inner
                    .barrier_after_stages(after, before, visibility)
                    .map_err(crate::Error::from_ffi)
            }

            /// Updates a fence after selected encoder stages.
            pub fn update_fence(
                &self,
                fence: &crate::metal::Fence,
                after: Stages,
            ) -> Result<(), crate::Error> {
                self.inner
                    .update_fence(&fence.inner, after)
                    .map_err(crate::Error::from_ffi)
            }

            /// Waits for a fence before selected encoder stages.
            pub fn wait_for_fence(
                &self,
                fence: &crate::metal::Fence,
                before: Stages,
            ) -> Result<(), crate::Error> {
                self.inner
                    .wait_for_fence(&fence.inner, before)
                    .map_err(crate::Error::from_ffi)
            }
        }
    };
}

impl_recording_encoder_common!(RecordingComputeEncoder);
impl_recording_encoder_common!(RecordingRenderEncoder);
impl_recording_encoder_common!(RecordingMachineLearningEncoder);

impl CommandQueue {
    /// Submits a non-empty batch of ended command buffers exactly once.
    pub fn submit(
        &self,
        buffers: Vec<EndedCommandBuffer>,
        options: Option<&super::CommitOptions>,
    ) -> Result<SubmittedCommandBuffers, crate::Error> {
        let buffers = buffers.into_iter().map(|buffer| buffer.inner).collect();
        self.inner
            .submit(buffers, options.map(|value| &value.inner))
            .map(|inner| SubmittedCommandBuffers { inner })
            .map_err(crate::Error::from_ffi)
    }

    /// Adds one residency set to the queue.
    pub fn add_residency_set(&self, set: &crate::metal::ResidencySet) -> Result<(), crate::Error> {
        self.inner
            .add_residency_set(&set.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Adds a non-empty slice of residency sets to the queue.
    pub fn add_residency_sets(
        &self,
        sets: &[&crate::metal::ResidencySet],
    ) -> Result<(), crate::Error> {
        let sets: Vec<_> = sets.iter().map(|set| &set.inner).collect();
        self.inner
            .add_residency_sets(&sets)
            .map_err(crate::Error::from_ffi)
    }

    /// Removes one residency set from the queue.
    pub fn remove_residency_set(
        &self,
        set: &crate::metal::ResidencySet,
    ) -> Result<(), crate::Error> {
        self.inner
            .remove_residency_set(&set.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Removes a non-empty slice of residency sets from the queue.
    pub fn remove_residency_sets(
        &self,
        sets: &[&crate::metal::ResidencySet],
    ) -> Result<(), crate::Error> {
        let sets: Vec<_> = sets.iter().map(|set| &set.inner).collect();
        self.inner
            .remove_residency_sets(&sets)
            .map_err(crate::Error::from_ffi)
    }

    /// Schedules an event signal after earlier queue work.
    pub fn signal_event(
        &self,
        event: &crate::metal::Event,
        value: u64,
    ) -> Result<(), crate::Error> {
        self.inner
            .signal_event(&event.inner, value)
            .map_err(crate::Error::from_ffi)
    }

    /// Schedules drawable signaling after earlier queue work.
    pub fn signal_drawable(
        &self,
        drawable: &crate::quartz_core::Drawable,
    ) -> Result<(), crate::Error> {
        self.inner
            .signal_drawable(&drawable.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Schedules a wait for an event value before later queue work.
    pub fn wait_for_event(
        &self,
        event: &crate::metal::Event,
        value: u64,
    ) -> Result<(), crate::Error> {
        self.inner
            .wait_for_event(&event.inner, value)
            .map_err(crate::Error::from_ffi)
    }

    /// Schedules a wait for drawable availability before later queue work.
    pub fn wait_for_drawable(
        &self,
        drawable: &crate::quartz_core::Drawable,
    ) -> Result<(), crate::Error> {
        self.inner
            .wait_for_drawable(&drawable.inner)
            .map_err(crate::Error::from_ffi)
    }

    /// Updates sparse buffer mappings using validated safe operations.
    pub fn update_buffer_mappings(
        &self,
        buffer: &crate::metal::Buffer,
        heap: Option<&crate::metal::Heap>,
        operations: &[super::UpdateSparseBufferMappingOperation],
    ) -> Result<(), crate::Error> {
        self.inner
            .update_buffer_mappings(&buffer.inner, heap.map(|value| &value.inner), operations)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies sparse buffer mappings using validated safe operations.
    pub fn copy_buffer_mappings(
        &self,
        source: &crate::metal::Buffer,
        destination: &crate::metal::Buffer,
        operations: &[super::CopySparseBufferMappingOperation],
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_buffer_mappings(&source.inner, &destination.inner, operations)
            .map_err(crate::Error::from_ffi)
    }

    /// Updates sparse texture mappings using validated safe operations.
    pub fn update_texture_mappings(
        &self,
        texture: &crate::metal::Texture,
        heap: Option<&crate::metal::Heap>,
        operations: &[super::UpdateSparseTextureMappingOperation],
    ) -> Result<(), crate::Error> {
        self.inner
            .update_texture_mappings(&texture.inner, heap.map(|value| &value.inner), operations)
            .map_err(crate::Error::from_ffi)
    }

    /// Copies sparse texture mappings using validated safe operations.
    pub fn copy_texture_mappings(
        &self,
        source: &crate::metal::Texture,
        destination: &crate::metal::Texture,
        operations: &[super::CopySparseTextureMappingOperation],
    ) -> Result<(), crate::Error> {
        self.inner
            .copy_texture_mappings(&source.inner, &destination.inner, operations)
            .map_err(crate::Error::from_ffi)
    }
}
