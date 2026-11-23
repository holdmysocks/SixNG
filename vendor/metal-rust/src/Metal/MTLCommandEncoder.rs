//! Safe render and compute command encoders.

use super::{
    AccelerationStructure, BarrierScope, Buffer, ComputePipelineState, CounterSampleBuffer,
    DepthStencilState, Device, Fence, Heap, IndexType, IndirectCommandBuffer,
    IntersectionFunctionTable, LogicalToPhysicalColorAttachmentMap, PrimitiveType,
    RenderPipelineState, RenderStages, ResourceUsage, SamplerState, Size, StoreAction,
    StoreActionOptions, Texture, VertexAmplificationViewMapping, Viewport, VisibilityResultMode,
    VisibleFunctionTable,
};
use crate::Error;

/// A render encoder that borrows its command buffer until encoding ends.
pub struct RenderCommandEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::RenderCommandEncoder<'a>,
}

/// A safely borrowed resource accepted by render usage declarations.
pub enum RenderResource<'a> {
    /// A buffer resource.
    Buffer(&'a Buffer),
    /// A texture resource.
    Texture(&'a Texture),
    /// An acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl RenderResource<'_> {
    fn as_ffi(&self) -> metal_rust_ffi::RenderResource<'_> {
        match self {
            Self::Buffer(value) => metal_rust_ffi::RenderResource::Buffer(&value.inner),
            Self::Texture(value) => metal_rust_ffi::RenderResource::Texture(&value.inner),
            Self::AccelerationStructure(value) => {
                metal_rust_ffi::RenderResource::AccelerationStructure(&value.inner)
            }
        }
    }
}

impl RenderCommandEncoder<'_> {
    /// Binds a render pipeline.
    pub fn set_pipeline(&self, pipeline: &RenderPipelineState) {
        self.inner.set_pipeline(&pipeline.inner);
    }

    /// Returns the encoder's device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }
    /// Inserts a debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insert_debug_signpost(value);
    }
    /// Pushes a debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.push_debug_group(value);
    }
    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.pop_debug_group();
    }
    /// Copies vertex-stage constant bytes.
    pub fn set_vertex_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner
            .set_vertex_bytes(bytes, index)
            .map_err(Error::from_ffi)
    }
    /// Copies fragment-stage constant bytes.
    pub fn set_fragment_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner
            .set_fragment_bytes(bytes, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a vertex buffer with checked offset and binding index.
    pub fn set_vertex_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a vertex buffer with an attribute stride.
    pub fn set_vertex_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer_with_stride(&buffer.inner, offset, stride, index)
            .map_err(Error::from_ffi)
    }
    /// Copies vertex bytes with an attribute stride.
    pub fn set_vertex_bytes_with_stride(
        &self,
        bytes: &[u8],
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_bytes_with_stride(bytes, stride, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a fragment buffer with checked offset and binding index.
    pub fn set_fragment_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a vertex texture at a checked index.
    pub fn set_vertex_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_vertex_texture(texture.map(|texture| &texture.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a fragment texture at a checked index.
    pub fn set_fragment_texture(
        &self,
        texture: Option<&Texture>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_texture(texture.map(|texture| &texture.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Copies tile-stage constant bytes.
    pub fn set_tile_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner
            .set_tile_bytes(bytes, index)
            .map_err(Error::from_ffi)
    }
    /// Copies object-stage constant bytes.
    pub fn set_object_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner
            .set_object_bytes(bytes, index)
            .map_err(Error::from_ffi)
    }
    /// Copies mesh-stage constant bytes.
    pub fn set_mesh_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner
            .set_mesh_bytes(bytes, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a tile-stage buffer.
    pub fn set_tile_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }
    /// Binds an object-stage buffer.
    pub fn set_object_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }
    /// Binds a mesh-stage buffer.
    pub fn set_mesh_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }

    /// Updates a previously bound vertex-buffer offset.
    pub fn set_vertex_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }
    /// Updates a vertex-buffer offset and attribute stride.
    pub fn set_vertex_buffer_offset_with_stride(
        &self,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffer_offset_with_stride(offset, stride, index)
            .map_err(Error::from_ffi)
    }
    /// Updates a previously bound fragment-buffer offset.
    pub fn set_fragment_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_fragment_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }
    /// Updates a previously bound tile-buffer offset.
    pub fn set_tile_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_tile_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }
    /// Updates a previously bound object-buffer offset.
    pub fn set_object_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_object_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }
    /// Updates a previously bound mesh-buffer offset.
    pub fn set_mesh_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_mesh_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }

    fn buffer_bindings<'a>(
        bindings: &'a [(&'a Buffer, usize)],
    ) -> Vec<(&'a metal_rust_ffi::Buffer, usize)> {
        bindings
            .iter()
            .map(|(buffer, offset)| (&buffer.inner, *offset))
            .collect()
    }
    /// Binds multiple vertex buffers through safe calls.
    pub fn set_vertex_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_buffers(&Self::buffer_bindings(bindings), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple vertex buffers and attribute strides.
    pub fn set_vertex_buffers_with_strides(
        &self,
        bindings: &[(&Buffer, usize, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi: Vec<_> = bindings
            .iter()
            .map(|(buffer, offset, stride)| (&buffer.inner, *offset, *stride))
            .collect();
        self.inner
            .set_vertex_buffers_with_strides(&ffi, start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple fragment buffers through safe calls.
    pub fn set_fragment_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_buffers(&Self::buffer_bindings(bindings), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple tile buffers through safe calls.
    pub fn set_tile_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_buffers(&Self::buffer_bindings(bindings), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple object buffers through safe calls.
    pub fn set_object_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_buffers(&Self::buffer_bindings(bindings), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple mesh buffers through safe calls.
    pub fn set_mesh_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_buffers(&Self::buffer_bindings(bindings), start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a tile texture.
    pub fn set_tile_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_tile_texture(texture.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears an object texture.
    pub fn set_object_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_object_texture(texture.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a mesh texture.
    pub fn set_mesh_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_mesh_texture(texture.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }

    fn textures<'a>(values: &'a [Option<&'a Texture>]) -> Vec<Option<&'a metal_rust_ffi::Texture>> {
        values.iter().map(|value| value.map(|v| &v.inner)).collect()
    }
    /// Binds multiple vertex textures through safe calls.
    pub fn set_vertex_textures(
        &self,
        values: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_textures(&Self::textures(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple fragment textures through safe calls.
    pub fn set_fragment_textures(
        &self,
        values: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_textures(&Self::textures(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple tile textures through safe calls.
    pub fn set_tile_textures(
        &self,
        values: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_textures(&Self::textures(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple object textures through safe calls.
    pub fn set_object_textures(
        &self,
        values: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_textures(&Self::textures(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple mesh textures through safe calls.
    pub fn set_mesh_textures(
        &self,
        values: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_textures(&Self::textures(values), start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a vertex sampler with optional LOD clamps.
    pub fn set_vertex_sampler(
        &self,
        value: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_sampler(value.map(|v| &v.inner), lod_clamp, index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a fragment sampler with optional LOD clamps.
    pub fn set_fragment_sampler(
        &self,
        value: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_sampler(value.map(|v| &v.inner), lod_clamp, index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a tile sampler with optional LOD clamps.
    pub fn set_tile_sampler(
        &self,
        value: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_sampler(value.map(|v| &v.inner), lod_clamp, index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears an object sampler with optional LOD clamps.
    pub fn set_object_sampler(
        &self,
        value: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_sampler(value.map(|v| &v.inner), lod_clamp, index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a mesh sampler with optional LOD clamps.
    pub fn set_mesh_sampler(
        &self,
        value: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_sampler(value.map(|v| &v.inner), lod_clamp, index)
            .map_err(Error::from_ffi)
    }

    fn samplers<'a>(
        values: &'a [Option<&'a SamplerState>],
    ) -> Vec<Option<&'a metal_rust_ffi::__private::objects::metal::SamplerState>> {
        values.iter().map(|value| value.map(|v| &v.inner)).collect()
    }
    /// Binds multiple vertex samplers through safe calls.
    pub fn set_vertex_samplers(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_samplers(&Self::samplers(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple fragment samplers through safe calls.
    pub fn set_fragment_samplers(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_samplers(&Self::samplers(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple tile samplers through safe calls.
    pub fn set_tile_samplers(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_samplers(&Self::samplers(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple object samplers through safe calls.
    pub fn set_object_samplers(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_samplers(&Self::samplers(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple mesh samplers through safe calls.
    pub fn set_mesh_samplers(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_samplers(&Self::samplers(values), start_index)
            .map_err(Error::from_ffi)
    }

    fn sampler_clamps<'a>(
        values: &'a [(Option<&'a SamplerState>, f32, f32)],
    ) -> Vec<(
        Option<&'a metal_rust_ffi::__private::objects::metal::SamplerState>,
        f32,
        f32,
    )> {
        values
            .iter()
            .map(|(value, min, max)| (value.map(|v| &v.inner), *min, *max))
            .collect()
    }
    /// Binds vertex samplers with per-slot LOD clamps.
    pub fn set_vertex_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_samplers_with_lod_clamps(&Self::sampler_clamps(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds fragment samplers with per-slot LOD clamps.
    pub fn set_fragment_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_samplers_with_lod_clamps(&Self::sampler_clamps(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds tile samplers with per-slot LOD clamps.
    pub fn set_tile_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_samplers_with_lod_clamps(&Self::sampler_clamps(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds object samplers with per-slot LOD clamps.
    pub fn set_object_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_samplers_with_lod_clamps(&Self::sampler_clamps(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds mesh samplers with per-slot LOD clamps.
    pub fn set_mesh_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_mesh_samplers_with_lod_clamps(&Self::sampler_clamps(values), start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a vertex acceleration structure.
    pub fn set_vertex_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_acceleration_structure(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a fragment acceleration structure.
    pub fn set_fragment_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_acceleration_structure(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a tile acceleration structure.
    pub fn set_tile_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_acceleration_structure(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a vertex visible-function table.
    pub fn set_vertex_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_visible_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a fragment visible-function table.
    pub fn set_fragment_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_visible_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a tile visible-function table.
    pub fn set_tile_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_visible_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a vertex intersection-function table.
    pub fn set_vertex_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_intersection_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a fragment intersection-function table.
    pub fn set_fragment_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_intersection_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }
    /// Binds or clears a tile intersection-function table.
    pub fn set_tile_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_intersection_function_table(value.map(|v| &v.inner), index)
            .map_err(Error::from_ffi)
    }

    fn visible_tables<'a>(
        values: &'a [Option<&'a VisibleFunctionTable>],
    ) -> Vec<Option<&'a metal_rust_ffi::__private::objects::metal::VisibleFunctionTable>> {
        values.iter().map(|value| value.map(|v| &v.inner)).collect()
    }
    fn intersection_tables<'a>(
        values: &'a [Option<&'a IntersectionFunctionTable>],
    ) -> Vec<Option<&'a metal_rust_ffi::__private::objects::metal::IntersectionFunctionTable>> {
        values.iter().map(|value| value.map(|v| &v.inner)).collect()
    }
    /// Binds multiple vertex visible-function tables.
    pub fn set_vertex_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_visible_function_tables(&Self::visible_tables(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple fragment visible-function tables.
    pub fn set_fragment_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_visible_function_tables(&Self::visible_tables(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple tile visible-function tables.
    pub fn set_tile_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_visible_function_tables(&Self::visible_tables(values), start_index)
            .map_err(Error::from_ffi)
    }
    /// Binds multiple vertex intersection-function tables.
    pub fn set_vertex_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_intersection_function_tables(
                &Self::intersection_tables(values),
                start_index,
            )
            .map_err(Error::from_ffi)
    }
    /// Binds multiple fragment intersection-function tables.
    pub fn set_fragment_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_intersection_function_tables(
                &Self::intersection_tables(values),
                start_index,
            )
            .map_err(Error::from_ffi)
    }
    /// Binds multiple tile intersection-function tables.
    pub fn set_tile_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tile_intersection_function_tables(&Self::intersection_tables(values), start_index)
            .map_err(Error::from_ffi)
    }

    /// Sets a finite RGBA blend constant.
    pub fn set_blend_color(&self, color: [f32; 4]) -> Result<(), Error> {
        self.inner.set_blend_color(color).map_err(Error::from_ffi)
    }
    /// Binds or clears a depth-stencil state.
    pub fn set_depth_stencil_state(&self, state: Option<&DepthStencilState>) -> Result<(), Error> {
        self.inner
            .set_depth_stencil_state(state.map(|v| &v.inner))
            .map_err(Error::from_ffi)
    }
    /// Sets a logical-to-physical color attachment map.
    pub fn set_color_attachment_map(
        &self,
        mapping: &LogicalToPhysicalColorAttachmentMap,
    ) -> Result<(), Error> {
        self.inner
            .set_color_attachment_map(&mapping.inner)
            .map_err(Error::from_ffi)
    }
    /// Sets checked vertex amplification view mappings.
    pub fn set_vertex_amplification(
        &self,
        mappings: &[VertexAmplificationViewMapping],
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_amplification(mappings)
            .map_err(Error::from_ffi)
    }

    /// Sets triangle culling.
    pub fn set_cull_mode(&self, value: super::CullMode) {
        self.inner.set_cull_mode(value);
    }

    /// Sets the front-facing winding order.
    pub fn set_front_facing_winding(&self, value: super::Winding) {
        self.inner.set_front_facing_winding(value);
    }

    /// Sets the triangle fill mode.
    pub fn set_triangle_fill_mode(&self, value: super::TriangleFillMode) {
        self.inner.set_triangle_fill_mode(value);
    }

    /// Sets finite depth-bias parameters.
    pub fn set_depth_bias(&self, bias: f32, slope_scale: f32, clamp: f32) -> Result<(), Error> {
        self.inner
            .set_depth_bias(bias, slope_scale, clamp)
            .map_err(Error::from_ffi)
    }

    /// Sets the depth clipping mode when supported.
    pub fn set_depth_clip_mode(&self, value: super::DepthClipMode) -> Result<(), Error> {
        self.inner
            .set_depth_clip_mode(value)
            .map_err(Error::from_ffi)
    }

    /// Sets finite ordered depth-test bounds when supported.
    pub fn set_depth_test_bounds(&self, min: f32, max: f32) -> Result<(), Error> {
        self.inner
            .set_depth_test_bounds(min, max)
            .map_err(Error::from_ffi)
    }

    /// Sets one stencil reference value for both faces.
    pub fn set_stencil_reference_value(&self, value: u32) {
        self.inner.set_stencil_reference_value(value);
    }

    /// Sets independent front and back stencil reference values.
    pub fn set_stencil_reference_values(&self, front: u32, back: u32) {
        self.inner.set_stencil_reference_values(front, back);
    }

    /// Sets a checked scissor rectangle.
    pub fn set_scissor_rect(&self, rect: super::ScissorRect) -> Result<(), Error> {
        self.inner.set_scissor_rect(rect).map_err(Error::from_ffi)
    }
    /// Sets multiple validated scissor rectangles.
    pub fn set_scissor_rects(&self, rects: &[super::ScissorRect]) -> Result<(), Error> {
        self.inner.set_scissor_rects(rects).map_err(Error::from_ffi)
    }

    /// Returns tile dimensions when tile shaders are available.
    pub fn tile_size(&self) -> Result<Size, Error> {
        self.inner.tile_size().map_err(Error::from_ffi)
    }

    /// Encodes a tile-texture barrier when supported.
    pub fn texture_barrier(&self) -> Result<(), Error> {
        self.inner.texture_barrier().map_err(Error::from_ffi)
    }

    /// Sets a validated viewport.
    pub fn set_viewport(&self, viewport: Viewport) -> Result<(), Error> {
        self.inner.set_viewport(viewport).map_err(Error::from_ffi)
    }
    /// Sets multiple validated viewports.
    pub fn set_viewports(&self, viewports: &[Viewport]) -> Result<(), Error> {
        self.inner.set_viewports(viewports).map_err(Error::from_ffi)
    }
    /// Configures visibility-result writes.
    pub fn set_visibility_result_mode(
        &self,
        mode: VisibilityResultMode,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_visibility_result_mode(mode, offset)
            .map_err(Error::from_ffi)
    }
    /// Sets a color attachment store action.
    pub fn set_color_store_action(&self, action: StoreAction, index: usize) -> Result<(), Error> {
        self.inner
            .set_color_store_action(action, index)
            .map_err(Error::from_ffi)
    }
    /// Sets the depth attachment store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner
            .set_depth_store_action(action)
            .map_err(Error::from_ffi)
    }
    /// Sets the stencil attachment store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.inner
            .set_stencil_store_action(action)
            .map_err(Error::from_ffi)
    }
    /// Sets color attachment store options.
    pub fn set_color_store_action_options(
        &self,
        options: StoreActionOptions,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_color_store_action_options(options, index)
            .map_err(Error::from_ffi)
    }
    /// Sets depth attachment store options.
    pub fn set_depth_store_action_options(&self, options: StoreActionOptions) -> Result<(), Error> {
        self.inner
            .set_depth_store_action_options(options)
            .map_err(Error::from_ffi)
    }
    /// Sets stencil attachment store options.
    pub fn set_stencil_store_action_options(
        &self,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        self.inner
            .set_stencil_store_action_options(options)
            .map_err(Error::from_ffi)
    }

    /// Encodes a non-indexed primitive draw.
    pub fn draw_primitives(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives(primitive, vertex_start, vertex_count)
            .map_err(Error::from_ffi)
    }

    /// Encodes an instanced non-indexed primitive draw.
    pub fn draw_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives_instanced(
                primitive,
                vertex_start,
                vertex_count,
                instance_count,
                base_instance,
            )
            .map_err(Error::from_ffi)
    }

    /// Encodes an indexed draw with validated index storage.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_primitives(
        &self,
        primitive: PrimitiveType,
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
                primitive,
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
    /// Encodes a non-indexed indirect draw.
    pub fn draw_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_primitives_indirect(primitive, &buffer.inner, offset)
            .map_err(Error::from_ffi)
    }
    /// Encodes an indexed indirect draw.
    pub fn draw_indexed_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        index_type: IndexType,
        index_buffer: &Buffer,
        index_buffer_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_indexed_primitives_indirect(
                primitive,
                index_type,
                &index_buffer.inner,
                index_buffer_offset,
                &indirect_buffer.inner,
                indirect_offset,
            )
            .map_err(Error::from_ffi)
    }
    /// Dispatches tile threads.
    pub fn dispatch_threads_per_tile(&self, threads: Size) -> Result<(), Error> {
        self.inner
            .dispatch_threads_per_tile(threads)
            .map_err(Error::from_ffi)
    }
    /// Draws mesh threadgroups.
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
    /// Draws mesh threadgroups from checked indirect storage.
    pub fn draw_mesh_threadgroups_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        self.inner
            .draw_mesh_threadgroups_indirect(&buffer.inner, offset, object_threads, mesh_threads)
            .map_err(Error::from_ffi)
    }
    /// Draws a mesh thread grid.
    pub fn draw_mesh_threads(
        &self,
        grid: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        self.inner
            .draw_mesh_threads(grid, object_threads, mesh_threads)
            .map_err(Error::from_ffi)
    }
    /// Draws tessellation patches.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_patches(
        &self,
        control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_patches(
                control_points,
                patch_start,
                patch_count,
                patch_indices.map(|v| &v.inner),
                patch_index_offset,
                instance_count,
                base_instance,
            )
            .map_err(Error::from_ffi)
    }
    /// Draws tessellation patches from indirect storage.
    pub fn draw_patches_indirect(
        &self,
        control_points: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_patches_indirect(
                control_points,
                patch_indices.map(|v| &v.inner),
                patch_index_offset,
                &indirect_buffer.inner,
                indirect_offset,
            )
            .map_err(Error::from_ffi)
    }
    /// Draws indexed tessellation patches.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches(
        &self,
        control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        control_point_indices: &Buffer,
        control_point_index_offset: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_indexed_patches(
                control_points,
                patch_start,
                patch_count,
                patch_indices.map(|v| &v.inner),
                patch_index_offset,
                &control_point_indices.inner,
                control_point_index_offset,
                instance_count,
                base_instance,
            )
            .map_err(Error::from_ffi)
    }
    /// Draws indexed tessellation patches from indirect storage.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches_indirect(
        &self,
        control_points: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        control_point_indices: &Buffer,
        control_point_index_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .draw_indexed_patches_indirect(
                control_points,
                patch_indices.map(|v| &v.inner),
                patch_index_offset,
                &control_point_indices.inner,
                control_point_index_offset,
                &indirect_buffer.inner,
                indirect_offset,
            )
            .map_err(Error::from_ffi)
    }
    /// Samples a counter buffer.
    pub fn sample_counters(
        &self,
        buffer: &CounterSampleBuffer,
        index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        self.inner
            .sample_counters(&buffer.inner, index, barrier)
            .map_err(Error::from_ffi)
    }
    /// Executes a checked indirect-command range.
    pub fn execute_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .execute_commands(&buffer.inner, range)
            .map_err(Error::from_ffi)
    }
    /// Executes an indirect-command range read from a checked buffer.
    pub fn execute_commands_indirect(
        &self,
        commands: &IndirectCommandBuffer,
        range_buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .execute_commands_indirect(&commands.inner, &range_buffer.inner, offset)
            .map_err(Error::from_ffi)
    }
    /// Declares one resource for optional render stages.
    pub fn use_resource(
        &self,
        resource: RenderResource<'_>,
        usage: ResourceUsage,
        stages: Option<RenderStages>,
    ) -> Result<(), Error> {
        self.inner
            .use_resource(resource.as_ffi(), usage, stages)
            .map_err(Error::from_ffi)
    }
    /// Declares multiple resources through safe calls.
    pub fn use_resources(
        &self,
        resources: &[RenderResource<'_>],
        usage: ResourceUsage,
        stages: Option<RenderStages>,
    ) -> Result<(), Error> {
        let ffi: Vec<_> = resources.iter().map(RenderResource::as_ffi).collect();
        self.inner
            .use_resources(&ffi, usage, stages)
            .map_err(Error::from_ffi)
    }
    /// Declares one heap for optional render stages.
    pub fn use_heap(&self, heap: &Heap, stages: Option<RenderStages>) -> Result<(), Error> {
        self.inner
            .use_heap(&heap.inner, stages)
            .map_err(Error::from_ffi)
    }
    /// Declares multiple heaps through safe calls.
    pub fn use_heaps(&self, heaps: &[&Heap], stages: Option<RenderStages>) -> Result<(), Error> {
        let ffi: Vec<_> = heaps.iter().map(|heap| &heap.inner).collect();
        self.inner.use_heaps(&ffi, stages).map_err(Error::from_ffi)
    }
    /// Waits for a fence before selected render stages.
    pub fn wait_for_fence(&self, fence: &Fence, stages: RenderStages) -> Result<(), Error> {
        self.inner
            .wait_for_fence(&fence.inner, stages)
            .map_err(Error::from_ffi)
    }
    /// Updates a fence after selected render stages.
    pub fn update_fence(&self, fence: &Fence, stages: RenderStages) -> Result<(), Error> {
        self.inner
            .update_fence(&fence.inner, stages)
            .map_err(Error::from_ffi)
    }
    /// Inserts a validated render-stage memory barrier.
    pub fn memory_barrier(
        &self,
        scope: super::BarrierScope,
        after: RenderStages,
        before: RenderStages,
    ) -> Result<(), Error> {
        self.inner
            .memory_barrier(scope, after, before)
            .map_err(Error::from_ffi)
    }
    /// Inserts render-stage barriers for safely borrowed resources.
    pub fn memory_barriers(
        &self,
        resources: &[RenderResource<'_>],
        after: RenderStages,
        before: RenderStages,
    ) -> Result<(), Error> {
        let ffi: Vec<_> = resources.iter().map(RenderResource::as_ffi).collect();
        self.inner
            .memory_barriers(&ffi, after, before)
            .map_err(Error::from_ffi)
    }
    /// Sets tile threadgroup memory.
    pub fn set_threadgroup_memory_length(
        &self,
        length: usize,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_threadgroup_memory_length(length, offset, index)
            .map_err(Error::from_ffi)
    }
    /// Sets object threadgroup memory.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_object_threadgroup_memory_length(length, index)
            .map_err(Error::from_ffi)
    }
    /// Sets a tessellation factor scale.
    pub fn set_tessellation_factor_scale(&self, scale: f32) -> Result<(), Error> {
        self.inner
            .set_tessellation_factor_scale(scale)
            .map_err(Error::from_ffi)
    }
    /// Binds a tessellation-factor buffer.
    pub fn set_tessellation_factor_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_tessellation_factor_buffer(&buffer.inner, offset, instance_stride)
            .map_err(Error::from_ffi)
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(self) {
        self.inner.end_encoding();
    }
}

/// A compute encoder that borrows its command buffer until encoding ends.
pub struct ComputeCommandEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::ComputeCommandEncoder<'a>,
}

/// A safely borrowed resource accepted by compute usage declarations.
pub enum ComputeResource<'a> {
    /// A buffer resource.
    Buffer(&'a Buffer),
    /// A texture resource.
    Texture(&'a Texture),
    /// An acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl ComputeResource<'_> {
    fn as_ffi(&self) -> metal_rust_ffi::ComputeResource<'_> {
        match self {
            Self::Buffer(value) => metal_rust_ffi::ComputeResource::Buffer(&value.inner),
            Self::Texture(value) => metal_rust_ffi::ComputeResource::Texture(&value.inner),
            Self::AccelerationStructure(value) => {
                metal_rust_ffi::ComputeResource::AccelerationStructure(&value.inner)
            }
        }
    }
}

impl ComputeCommandEncoder<'_> {
    /// Binds a compute pipeline.
    pub fn set_pipeline(&self, pipeline: &ComputePipelineState) {
        self.inner.set_pipeline(&pipeline.inner);
    }

    /// Returns the encoder's device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }
    /// Inserts a debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insert_debug_signpost(value);
    }
    /// Pushes a debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.push_debug_group(value);
    }
    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.pop_debug_group();
    }
    /// Copies compute constant bytes.
    pub fn set_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.inner.set_bytes(bytes, index).map_err(Error::from_ffi)
    }

    /// Binds a compute buffer with checked offset and binding index.
    pub fn set_buffer(&self, buffer: &Buffer, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a compute buffer with a checked attribute stride.
    pub fn set_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer_with_stride(&buffer.inner, offset, stride, index)
            .map_err(Error::from_ffi)
    }

    /// Updates the offset of a previously bound compute buffer.
    pub fn set_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_buffer_offset(offset, index)
            .map_err(Error::from_ffi)
    }

    /// Updates a bound buffer's checked offset and attribute stride.
    pub fn set_buffer_offset_with_stride(
        &self,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer_offset_with_stride(offset, stride, index)
            .map_err(Error::from_ffi)
    }

    /// Binds multiple compute buffers using checked individual bindings.
    pub fn set_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi_bindings: Vec<_> = bindings
            .iter()
            .map(|(buffer, offset)| (&buffer.inner, *offset))
            .collect();
        self.inner
            .set_buffers(&ffi_bindings, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds multiple compute buffers and strides through checked calls.
    pub fn set_buffers_with_strides(
        &self,
        bindings: &[(&Buffer, usize, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi_bindings: Vec<_> = bindings
            .iter()
            .map(|(buffer, offset, stride)| (&buffer.inner, *offset, *stride))
            .collect();
        self.inner
            .set_buffers_with_strides(&ffi_bindings, start_index)
            .map_err(Error::from_ffi)
    }

    /// Copies compute bytes with a checked attribute stride.
    pub fn set_bytes_with_stride(
        &self,
        bytes: &[u8],
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_bytes_with_stride(bytes, stride, index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a compute texture at a checked index.
    pub fn set_texture(&self, texture: Option<&super::Texture>, index: usize) -> Result<(), Error> {
        self.inner
            .set_texture(texture.map(|texture| &texture.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds multiple compute textures using checked individual bindings.
    pub fn set_textures(
        &self,
        textures: &[Option<&super::Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi_textures: Vec<_> = textures
            .iter()
            .map(|texture| texture.map(|texture| &texture.inner))
            .collect();
        self.inner
            .set_textures(&ffi_textures, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a sampler at a checked index.
    pub fn set_sampler_state(
        &self,
        sampler: Option<&SamplerState>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_sampler_state(sampler.map(|sampler| &sampler.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds a sampler with finite ordered LOD clamps.
    pub fn set_sampler_state_with_lod(
        &self,
        sampler: Option<&SamplerState>,
        lod_min: f32,
        lod_max: f32,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_sampler_state_with_lod(
                sampler.map(|sampler| &sampler.inner),
                lod_min,
                lod_max,
                index,
            )
            .map_err(Error::from_ffi)
    }

    /// Binds multiple samplers through checked individual calls.
    pub fn set_sampler_states(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let samplers: Vec<_> = samplers
            .iter()
            .map(|sampler| sampler.map(|sampler| &sampler.inner))
            .collect();
        self.inner
            .set_sampler_states(&samplers, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds multiple samplers and LOD ranges through checked calls.
    pub fn set_sampler_states_with_lod(
        &self,
        samplers: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        let samplers: Vec<_> = samplers
            .iter()
            .map(|(sampler, min, max)| (sampler.map(|sampler| &sampler.inner), *min, *max))
            .collect();
        self.inner
            .set_sampler_states_with_lod(&samplers, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears a visible function table.
    pub fn set_visible_function_table(
        &self,
        table: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_visible_function_table(table.map(|table| &table.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds visible function tables through checked individual calls.
    pub fn set_visible_function_tables(
        &self,
        tables: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let tables: Vec<_> = tables
            .iter()
            .map(|table| table.map(|table| &table.inner))
            .collect();
        self.inner
            .set_visible_function_tables(&tables, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears an intersection function table.
    pub fn set_intersection_function_table(
        &self,
        table: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_intersection_function_table(table.map(|table| &table.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Binds intersection function tables through checked individual calls.
    pub fn set_intersection_function_tables(
        &self,
        tables: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let tables: Vec<_> = tables
            .iter()
            .map(|table| table.map(|table| &table.inner))
            .collect();
        self.inner
            .set_intersection_function_tables(&tables, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears an acceleration structure.
    pub fn set_acceleration_structure(
        &self,
        structure: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_acceleration_structure(structure.map(|structure| &structure.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Makes subsequent compute work wait for a fence.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .wait_for_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Updates a fence after prior compute work.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .update_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Declares one heap used by this encoder.
    pub fn use_heap(&self, heap: &Heap) -> Result<(), Error> {
        self.inner.use_heap(&heap.inner).map_err(Error::from_ffi)
    }

    /// Declares multiple heaps through checked individual calls.
    pub fn use_heaps(&self, heaps: &[&Heap]) -> Result<(), Error> {
        let heaps: Vec<_> = heaps.iter().map(|heap| &heap.inner).collect();
        self.inner.use_heaps(&heaps).map_err(Error::from_ffi)
    }

    /// Inserts a validated compute memory barrier scope.
    pub fn memory_barrier(&self, scope: BarrierScope) -> Result<(), Error> {
        self.inner.memory_barrier(scope).map_err(Error::from_ffi)
    }

    /// Inserts memory barriers for safely borrowed resources.
    pub fn memory_barriers(&self, resources: &[ComputeResource<'_>]) -> Result<(), Error> {
        let resources: Vec<_> = resources.iter().map(ComputeResource::as_ffi).collect();
        self.inner
            .memory_barriers(&resources)
            .map_err(Error::from_ffi)
    }

    /// Declares one safely borrowed resource and validated usage flags.
    pub fn use_resource(
        &self,
        resource: ComputeResource<'_>,
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        self.inner
            .use_resource(resource.as_ffi(), usage)
            .map_err(Error::from_ffi)
    }

    /// Declares multiple resources through individually checked calls.
    pub fn use_resources(
        &self,
        resources: &[ComputeResource<'_>],
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        let resources: Vec<_> = resources.iter().map(ComputeResource::as_ffi).collect();
        self.inner
            .use_resources(&resources, usage)
            .map_err(Error::from_ffi)
    }

    /// Samples a counter buffer at a checked sample index.
    pub fn sample_counters(
        &self,
        buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        self.inner
            .sample_counters(&buffer.inner, sample_index, barrier)
            .map_err(Error::from_ffi)
    }

    /// Executes a checked range from an indirect command buffer.
    pub fn execute_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .execute_commands(&buffer.inner, range)
            .map_err(Error::from_ffi)
    }

    /// Reads an indirect execution range from checked buffer storage.
    pub fn execute_commands_indirect(
        &self,
        commands: &IndirectCommandBuffer,
        range_buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .execute_commands_indirect(&commands.inner, &range_buffer.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Returns this encoder's dispatch type.
    #[must_use]
    pub fn dispatch_type(&self) -> super::DispatchType {
        self.inner.dispatch_type()
    }

    /// Sets a checked imageblock size.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner
            .set_imageblock_size(width, height)
            .map_err(Error::from_ffi)
    }

    /// Sets a checked stage-in region.
    pub fn set_stage_in_region(&self, region: super::Region) -> Result<(), Error> {
        self.inner
            .set_stage_in_region(region)
            .map_err(Error::from_ffi)
    }

    /// Reads a stage-in region from a checked indirect-buffer range.
    pub fn set_stage_in_region_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_stage_in_region_indirect(&buffer.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Sets checked threadgroup memory length.
    pub fn set_threadgroup_memory_length(&self, length: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_threadgroup_memory_length(length, index)
            .map_err(Error::from_ffi)
    }

    /// Dispatches using arguments stored in a checked indirect buffer range.
    pub fn dispatch_threadgroups_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        self.inner
            .dispatch_threadgroups_indirect(&buffer.inner, offset, threads_per_group)
            .map_err(Error::from_ffi)
    }

    /// Dispatches a one-dimensional grid.
    pub fn dispatch_1d(&self, threads: usize, threads_per_group: usize) -> Result<(), Error> {
        self.inner
            .dispatch_1d(threads, threads_per_group)
            .map_err(Error::from_ffi)
    }

    /// Dispatches a checked multidimensional thread grid.
    pub fn dispatch_threads(&self, threads: Size, threads_per_group: Size) -> Result<(), Error> {
        self.inner
            .dispatch_threads(threads, threads_per_group)
            .map_err(Error::from_ffi)
    }

    /// Dispatches a checked multidimensional threadgroup grid.
    pub fn dispatch_threadgroups(
        &self,
        groups: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        self.inner
            .dispatch_threadgroups(groups, threads_per_group)
            .map_err(Error::from_ffi)
    }

    /// Ends encoding explicitly. Dropping the encoder also ends encoding.
    pub fn end_encoding(self) {
        self.inner.end_encoding();
    }
}
