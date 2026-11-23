//! Safe argument-buffer and indirect-command encoding APIs.

use super::{
    AccelerationStructure, ArgumentEncoder, Buffer, ComputePipelineState, CullMode, DepthClipMode,
    DepthStencilState, IndexType, IndirectCommandBuffer, IndirectComputeCommand,
    IndirectRenderCommand, IntersectionFunctionTable, PrimitiveType, Region, RenderPipelineState,
    ResourceID, SamplerState, Size, Texture, TriangleFillMode, VisibleFunctionTable, Winding,
};
use crate::Error;

enum RecordedIndirectRenderOperation<'a> {
    Reset,
    SetBarrier,
    ClearBarrier,
    DrawPatches {
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&'a Buffer>,
        patch_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &'a Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    },
    DrawIndexedPatches {
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&'a Buffer>,
        patch_index_buffer_offset: usize,
        control_point_index_buffer: &'a Buffer,
        control_point_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &'a Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    },
}

#[derive(Clone, Copy)]
enum RecordedIndirectComputeOperation {
    Reset,
    SetBarrier,
    ClearBarrier,
}

/// A safe command-list builder for one indirect render command.
#[derive(Default)]
pub struct IndirectRenderCommandRecording<'a> {
    operations: Vec<RecordedIndirectRenderOperation<'a>>,
}

impl<'a> IndirectRenderCommandRecording<'a> {
    /// Records a reset of the selected indirect render command.
    pub fn reset(&mut self) {
        self.operations.push(RecordedIndirectRenderOperation::Reset);
    }

    /// Records insertion of a barrier before later operations.
    pub fn set_barrier(&mut self) {
        self.operations
            .push(RecordedIndirectRenderOperation::SetBarrier);
    }

    /// Records removal of the currently configured barrier.
    pub fn clear_barrier(&mut self) {
        self.operations
            .push(RecordedIndirectRenderOperation::ClearBarrier);
    }

    /// Records a non-indexed patch draw whose buffers are checked when applied.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_patches(
        &mut self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&'a Buffer>,
        patch_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &'a Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) {
        self.operations
            .push(RecordedIndirectRenderOperation::DrawPatches {
                number_of_patch_control_points,
                patch_start,
                patch_count,
                patch_index_buffer,
                patch_index_buffer_offset,
                instance_count,
                base_instance,
                tessellation_factor_buffer,
                tessellation_factor_buffer_offset,
                instance_stride,
            });
    }

    /// Records an indexed patch draw whose buffers are checked when applied.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches(
        &mut self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&'a Buffer>,
        patch_index_buffer_offset: usize,
        control_point_index_buffer: &'a Buffer,
        control_point_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &'a Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) {
        self.operations
            .push(RecordedIndirectRenderOperation::DrawIndexedPatches {
                number_of_patch_control_points,
                patch_start,
                patch_count,
                patch_index_buffer,
                patch_index_buffer_offset,
                control_point_index_buffer,
                control_point_index_buffer_offset,
                instance_count,
                base_instance,
                tessellation_factor_buffer,
                tessellation_factor_buffer_offset,
                instance_stride,
            });
    }
}

/// A safe command-list builder for one indirect compute command.
#[derive(Default)]
pub struct IndirectComputeCommandRecording {
    operations: Vec<RecordedIndirectComputeOperation>,
}

impl IndirectComputeCommandRecording {
    /// Records a reset of the selected indirect compute command.
    pub fn reset(&mut self) {
        self.operations
            .push(RecordedIndirectComputeOperation::Reset);
    }

    /// Records insertion of a barrier before later operations.
    pub fn set_barrier(&mut self) {
        self.operations
            .push(RecordedIndirectComputeOperation::SetBarrier);
    }

    /// Records removal of the currently configured barrier.
    pub fn clear_barrier(&mut self) {
        self.operations
            .push(RecordedIndirectComputeOperation::ClearBarrier);
    }
}

impl IndirectCommandBuffer {
    /// Returns the stable GPU resource identifier after an availability check.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }

    /// Builds and applies operations to a checked indirect render command.
    pub fn with_indirect_render_command<R>(
        &self,
        command_index: usize,
        body: impl for<'recording> FnOnce(&mut IndirectRenderCommandRecording<'recording>) -> R,
    ) -> Result<R, Error> {
        let mut recording = IndirectRenderCommandRecording::default();
        let result = body(&mut recording);
        self.inner
            .with_indirect_render_command(command_index, |inner| {
                for operation in recording.operations {
                    match operation {
                        RecordedIndirectRenderOperation::Reset => inner.reset()?,
                        RecordedIndirectRenderOperation::SetBarrier => inner.set_barrier()?,
                        RecordedIndirectRenderOperation::ClearBarrier => inner.clear_barrier()?,
                        RecordedIndirectRenderOperation::DrawPatches {
                            number_of_patch_control_points,
                            patch_start,
                            patch_count,
                            patch_index_buffer,
                            patch_index_buffer_offset,
                            instance_count,
                            base_instance,
                            tessellation_factor_buffer,
                            tessellation_factor_buffer_offset,
                            instance_stride,
                        } => inner.draw_patches(
                            number_of_patch_control_points,
                            patch_start,
                            patch_count,
                            patch_index_buffer.map(|value| &value.inner),
                            patch_index_buffer_offset,
                            instance_count,
                            base_instance,
                            &tessellation_factor_buffer.inner,
                            tessellation_factor_buffer_offset,
                            instance_stride,
                        )?,
                        RecordedIndirectRenderOperation::DrawIndexedPatches {
                            number_of_patch_control_points,
                            patch_start,
                            patch_count,
                            patch_index_buffer,
                            patch_index_buffer_offset,
                            control_point_index_buffer,
                            control_point_index_buffer_offset,
                            instance_count,
                            base_instance,
                            tessellation_factor_buffer,
                            tessellation_factor_buffer_offset,
                            instance_stride,
                        } => inner.draw_indexed_patches(
                            number_of_patch_control_points,
                            patch_start,
                            patch_count,
                            patch_index_buffer.map(|value| &value.inner),
                            patch_index_buffer_offset,
                            &control_point_index_buffer.inner,
                            control_point_index_buffer_offset,
                            instance_count,
                            base_instance,
                            &tessellation_factor_buffer.inner,
                            tessellation_factor_buffer_offset,
                            instance_stride,
                        )?,
                    }
                }
                Ok(())
            })
            .map_err(Error::from_ffi)?
            .map_err(Error::from_ffi)?;
        Ok(result)
    }

    /// Builds and applies operations to a checked indirect compute command.
    pub fn with_indirect_compute_command<R>(
        &self,
        command_index: usize,
        body: impl FnOnce(&mut IndirectComputeCommandRecording) -> R,
    ) -> Result<R, Error> {
        let mut recording = IndirectComputeCommandRecording::default();
        let result = body(&mut recording);
        self.inner
            .with_indirect_compute_command(command_index, |inner| {
                for operation in recording.operations {
                    match operation {
                        RecordedIndirectComputeOperation::Reset => inner.reset()?,
                        RecordedIndirectComputeOperation::SetBarrier => inner.set_barrier()?,
                        RecordedIndirectComputeOperation::ClearBarrier => inner.clear_barrier()?,
                    }
                }
                Ok(())
            })
            .map_err(Error::from_ffi)?
            .map_err(Error::from_ffi)?;
        Ok(result)
    }

    /// Resets a checked half-open range of commands.
    pub fn reset_commands(&self, range: std::ops::Range<usize>) -> Result<(), Error> {
        self.inner.reset_commands(range).map_err(Error::from_ffi)
    }
}

impl ArgumentEncoder {
    /// Creates a nested argument encoder for the member at `index`.
    pub fn new_argument_encoder(&self, index: usize) -> Result<Self, Error> {
        self.inner
            .new_argument_encoder(index)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Selects a checked byte offset in `buffer` as the argument destination.
    pub fn set_argument_buffer(&self, buffer: &Buffer, offset: usize) -> Result<(), Error> {
        self.inner
            .set_argument_buffer(&buffer.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Selects one array element in a checked buffer-backed argument array.
    pub fn set_argument_buffer_element(
        &self,
        buffer: &Buffer,
        start_offset: usize,
        array_element: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_argument_buffer_element(&buffer.inner, start_offset, array_element)
            .map_err(Error::from_ffi)
    }

    /// Builds initialized bytes in a scoped slice and writes them to a verified
    /// member of the currently selected CPU-visible argument buffer.
    pub fn with_constant_data<R>(
        &self,
        index: usize,
        argument_buffer: &Buffer,
        argument_buffer_offset: usize,
        member_range: std::ops::Range<usize>,
        body: impl for<'data> FnOnce(&'data mut [u8]) -> R,
    ) -> Result<R, Error> {
        self.inner
            .with_constant_data(
                index,
                &argument_buffer.inner,
                argument_buffer_offset,
                member_range,
                body,
            )
            .map_err(Error::from_ffi)
    }

    /// Binds an optional buffer at an argument index with a checked byte offset.
    pub fn set_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of optional buffers and offsets at consecutive indices.
    pub fn set_buffers(
        &self,
        bindings: &[(Option<&Buffer>, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = bindings
            .iter()
            .map(|(buffer, offset)| (buffer.map(|value| &value.inner), *offset))
            .collect::<Vec<_>>();
        self.inner
            .set_buffers(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the texture at an argument index.
    pub fn set_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_texture(texture.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of optional textures at consecutive indices.
    pub fn set_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = textures
            .iter()
            .map(|texture| texture.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_textures(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the acceleration structure at an argument index.
    pub fn set_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_acceleration_structure(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the compute pipeline state at an argument index.
    pub fn set_compute_pipeline_state(
        &self,
        value: Option<&ComputePipelineState>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_compute_pipeline_state(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of compute pipeline states at consecutive indices.
    pub fn set_compute_pipeline_states(
        &self,
        values: &[Option<&ComputePipelineState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_compute_pipeline_states(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the depth-stencil state at an argument index.
    pub fn set_depth_stencil_state(
        &self,
        value: Option<&DepthStencilState>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_depth_stencil_state(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of depth-stencil states at consecutive indices.
    pub fn set_depth_stencil_states(
        &self,
        values: &[Option<&DepthStencilState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_depth_stencil_states(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the indirect command buffer at an argument index.
    pub fn set_indirect_command_buffer(
        &self,
        value: Option<&IndirectCommandBuffer>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_indirect_command_buffer(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of indirect command buffers at consecutive indices.
    pub fn set_indirect_command_buffers(
        &self,
        values: &[Option<&IndirectCommandBuffer>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_indirect_command_buffers(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the intersection function table at an argument index.
    pub fn set_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_intersection_function_table(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds intersection function tables at consecutive argument indices.
    pub fn set_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_intersection_function_tables(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the render pipeline state at an argument index.
    pub fn set_render_pipeline_state(
        &self,
        value: Option<&RenderPipelineState>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_render_pipeline_state(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of render pipeline states at consecutive indices.
    pub fn set_render_pipeline_states(
        &self,
        values: &[Option<&RenderPipelineState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_render_pipeline_states(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the sampler state at an argument index.
    pub fn set_sampler_state(
        &self,
        value: Option<&SamplerState>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_sampler_state(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds a slice of sampler states at consecutive indices.
    pub fn set_sampler_states(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_sampler_states(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears the visible function table at an argument index.
    pub fn set_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_visible_function_table(value.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safely binds visible function tables at consecutive argument indices.
    pub fn set_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = values
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_visible_function_tables(&ffi, start_index)
            .map_err(Error::from_ffi)
    }
}

impl IndirectRenderCommand {
    /// Resets the indirect render command to its initial empty state.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }
    /// Inserts a barrier before later operations in this indirect command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        self.inner.set_barrier().map_err(Error::from_ffi)
    }
    /// Removes the barrier previously configured on this indirect command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        self.inner.clear_barrier().map_err(Error::from_ffi)
    }

    /// Binds an optional vertex buffer after validating its byte offset.
    pub fn set_vertex_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds an optional vertex buffer with a checked dynamic attribute stride.
    pub fn set_vertex_buffer_with_stride(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer_with_stride(
                buffer.map(|value| &value.inner),
                offset,
                Some(stride),
                index,
            )
            .map_err(Error::from_ffi)
    }

    /// Binds an optional fragment buffer after validating its byte offset.
    pub fn set_fragment_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds an optional mesh-stage buffer after validating its byte offset.
    pub fn set_mesh_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds an optional object-stage buffer after validating its byte offset.
    pub fn set_object_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Selects the render pipeline state used by this indirect command.
    pub fn set_render_pipeline_state(&self, state: &RenderPipelineState) -> Result<(), Error> {
        self.inner
            .set_render_pipeline_state(&state.inner)
            .map_err(Error::from_ffi)
    }

    /// Selects or clears the depth-stencil state used by this indirect command.
    pub fn set_depth_stencil_state(&self, state: Option<&DepthStencilState>) -> Result<(), Error> {
        self.inner
            .set_depth_stencil_state(state.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Sets a validated triangle culling mode.
    pub fn set_cull_mode(&self, value: CullMode) -> Result<(), Error> {
        self.inner.set_cull_mode(value).map_err(Error::from_ffi)
    }
    /// Sets a validated depth clipping mode.
    pub fn set_depth_clip_mode(&self, value: DepthClipMode) -> Result<(), Error> {
        self.inner
            .set_depth_clip_mode(value)
            .map_err(Error::from_ffi)
    }
    /// Sets the winding used to identify front-facing primitives.
    pub fn set_front_facing_winding(&self, value: Winding) -> Result<(), Error> {
        self.inner
            .set_front_facing_winding(value)
            .map_err(Error::from_ffi)
    }
    /// Sets a validated triangle fill mode.
    pub fn set_triangle_fill_mode(&self, value: TriangleFillMode) -> Result<(), Error> {
        self.inner
            .set_triangle_fill_mode(value)
            .map_err(Error::from_ffi)
    }
    /// Sets finite depth-bias parameters for subsequent indirect draws.
    pub fn set_depth_bias(
        &self,
        depth_bias: f32,
        slope_scale: f32,
        clamp: f32,
    ) -> Result<(), Error> {
        self.inner
            .set_depth_bias(depth_bias, slope_scale, clamp)
            .map_err(Error::from_ffi)
    }
    /// Sets object-stage threadgroup memory length for one binding index.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_threadgroup_memory_length(length, index)
            .map_err(Error::from_ffi)
    }

    /// Encodes a non-indexed draw after checking vertex and instance ranges.
    pub fn draw_primitives(
        &self,
        primitive_type: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives(
                primitive_type,
                vertex_start,
                vertex_count,
                instance_count,
                base_instance,
            )
            .map_err(Error::from_ffi)
    }

    #[allow(clippy::too_many_arguments)]
    /// Encodes an indexed draw after validating the index-buffer byte range.
    pub fn draw_indexed_primitives(
        &self,
        primitive_type: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        index_buffer: &Buffer,
        index_buffer_offset: usize,
        instance_count: usize,
        base_vertex: isize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_indexed_primitives(
                primitive_type,
                index_count,
                index_type,
                &index_buffer.inner,
                index_buffer_offset,
                instance_count,
                base_vertex,
                base_instance,
            )
            .map_err(Error::from_ffi)
    }

    /// Encodes a mesh draw using checked non-zero threadgroup dimensions.
    pub fn draw_mesh_threadgroups(
        &self,
        groups: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        self.inner
            .draw_mesh_threadgroups(groups, object_threads, mesh_threads)
            .map_err(Error::from_ffi)
    }

    /// Encodes a mesh draw using checked non-zero thread-grid dimensions.
    pub fn draw_mesh_threads(
        &self,
        threads: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        self.inner
            .draw_mesh_threads(threads, object_threads, mesh_threads)
            .map_err(Error::from_ffi)
    }
}

impl IndirectComputeCommand {
    /// Resets the indirect compute command to its initial empty state.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }
    /// Inserts a barrier before later operations in this indirect command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        self.inner.set_barrier().map_err(Error::from_ffi)
    }
    /// Removes the barrier previously configured on this indirect command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        self.inner.clear_barrier().map_err(Error::from_ffi)
    }

    /// Selects the compute pipeline state used by this indirect command.
    pub fn set_compute_pipeline_state(&self, state: &ComputePipelineState) -> Result<(), Error> {
        self.inner
            .set_compute_pipeline_state(&state.inner)
            .map_err(Error::from_ffi)
    }

    /// Binds an optional kernel buffer after validating its byte offset.
    pub fn set_kernel_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_kernel_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds an optional kernel buffer with a checked dynamic attribute stride.
    pub fn set_kernel_buffer_with_stride(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_kernel_buffer_with_stride(
                buffer.map(|value| &value.inner),
                offset,
                Some(stride),
                index,
            )
            .map_err(Error::from_ffi)
    }

    /// Sets checked non-zero imageblock dimensions.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner
            .set_imageblock_size(width, height)
            .map_err(Error::from_ffi)
    }
    /// Sets a non-empty stage-in region after checking coordinate overflow.
    pub fn set_stage_in_region(&self, region: Region) -> Result<(), Error> {
        self.inner
            .set_stage_in_region(region)
            .map_err(Error::from_ffi)
    }
    /// Sets threadgroup memory length for one binding index.
    pub fn set_threadgroup_memory_length(&self, length: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_threadgroup_memory_length(length, index)
            .map_err(Error::from_ffi)
    }
    /// Encodes concurrent threadgroup dispatch with non-zero dimensions.
    pub fn concurrent_dispatch_threadgroups(
        &self,
        groups: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        self.inner
            .concurrent_dispatch_threadgroups(groups, threads_per_group)
            .map_err(Error::from_ffi)
    }
    /// Encodes concurrent thread dispatch with non-zero dimensions.
    pub fn concurrent_dispatch_threads(
        &self,
        threads: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        self.inner
            .concurrent_dispatch_threads(threads, threads_per_group)
            .map_err(Error::from_ffi)
    }
}
