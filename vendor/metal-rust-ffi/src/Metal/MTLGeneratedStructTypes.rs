//! Generated safe Rust representations for plain framework structs.
//!
//! These are safe value representations, not promises of Objective-C ABI layout.

use super::generated_value_types::*;
use super::{Origin, Region};

/// Safe value representation for Metal `MTL4::BufferRange`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BufferRange {
    /// Metal field `MTL4::BufferRange::bufferAddress`.
    pub buffer_address: u64,
    /// Metal field `MTL4::BufferRange::length`.
    pub length: u64,
}

/// Safe value representation for Metal `MTL4::CopySparseBufferMappingOperation`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CopySparseBufferMappingOperation {
    /// Metal field `MTL4::CopySparseBufferMappingOperation::sourceRange`.
    pub source_range: std::ops::Range<usize>,
    /// Metal field `MTL4::CopySparseBufferMappingOperation::destinationOffset`.
    pub destination_offset: usize,
}

/// Safe value representation for Metal `MTL4::CopySparseTextureMappingOperation`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CopySparseTextureMappingOperation {
    /// Metal field `MTL4::CopySparseTextureMappingOperation::sourceRegion`.
    pub source_region: Region,
    /// Metal field `MTL4::CopySparseTextureMappingOperation::sourceLevel`.
    pub source_level: usize,
    /// Metal field `MTL4::CopySparseTextureMappingOperation::sourceSlice`.
    pub source_slice: usize,
    /// Metal field `MTL4::CopySparseTextureMappingOperation::destinationOrigin`.
    pub destination_origin: Origin,
    /// Metal field `MTL4::CopySparseTextureMappingOperation::destinationLevel`.
    pub destination_level: usize,
    /// Metal field `MTL4::CopySparseTextureMappingOperation::destinationSlice`.
    pub destination_slice: usize,
}

/// Safe value representation for Metal `MTL4::TimestampHeapEntry`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimestampHeapEntry {
    /// Metal field `MTL4::TimestampHeapEntry::timestamp`.
    pub timestamp: u64,
}

/// Safe value representation for Metal `MTL4::UpdateSparseBufferMappingOperation`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateSparseBufferMappingOperation {
    /// Metal field `MTL4::UpdateSparseBufferMappingOperation::mode`.
    pub mode: SparseTextureMappingMode,
    /// Metal field `MTL4::UpdateSparseBufferMappingOperation::bufferRange`.
    pub buffer_range: std::ops::Range<usize>,
    /// Metal field `MTL4::UpdateSparseBufferMappingOperation::heapOffset`.
    pub heap_offset: usize,
}

/// Safe value representation for Metal `MTL4::UpdateSparseTextureMappingOperation`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateSparseTextureMappingOperation {
    /// Metal field `MTL4::UpdateSparseTextureMappingOperation::mode`.
    pub mode: SparseTextureMappingMode,
    /// Metal field `MTL4::UpdateSparseTextureMappingOperation::textureRegion`.
    pub texture_region: Region,
    /// Metal field `MTL4::UpdateSparseTextureMappingOperation::textureLevel`.
    pub texture_level: usize,
    /// Metal field `MTL4::UpdateSparseTextureMappingOperation::textureSlice`.
    pub texture_slice: usize,
    /// Metal field `MTL4::UpdateSparseTextureMappingOperation::heapOffset`.
    pub heap_offset: usize,
}

/// Safe value representation for Metal `MTL::AccelerationStructureInstanceDescriptor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccelerationStructureInstanceDescriptor {
    /// Metal field `MTL::AccelerationStructureInstanceDescriptor::transformationMatrix`.
    pub transformation_matrix: PackedFloat4x3,
    /// Metal field `MTL::AccelerationStructureInstanceDescriptor::options`.
    pub options: AccelerationStructureInstanceOptions,
    /// Metal field `MTL::AccelerationStructureInstanceDescriptor::mask`.
    pub mask: u32,
    /// Metal field `MTL::AccelerationStructureInstanceDescriptor::intersectionFunctionTableOffset`.
    pub intersection_function_table_offset: u32,
    /// Metal field `MTL::AccelerationStructureInstanceDescriptor::accelerationStructureIndex`.
    pub acceleration_structure_index: u32,
}

/// Safe value representation for Metal `MTL::AccelerationStructureMotionInstanceDescriptor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccelerationStructureMotionInstanceDescriptor {
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::options`.
    pub options: AccelerationStructureInstanceOptions,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::mask`.
    pub mask: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::intersectionFunctionTableOffset`.
    pub intersection_function_table_offset: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::accelerationStructureIndex`.
    pub acceleration_structure_index: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::userID`.
    pub user_id: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionTransformsStartIndex`.
    pub motion_transforms_start_index: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionTransformsCount`.
    pub motion_transforms_count: u32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionStartBorderMode`.
    pub motion_start_border_mode: MotionBorderMode,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionEndBorderMode`.
    pub motion_end_border_mode: MotionBorderMode,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionStartTime`.
    pub motion_start_time: f32,
    /// Metal field `MTL::AccelerationStructureMotionInstanceDescriptor::motionEndTime`.
    pub motion_end_time: f32,
}

/// Safe value representation for Metal `MTL::AccelerationStructureSizes`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccelerationStructureSizes {
    /// Metal field `MTL::AccelerationStructureSizes::accelerationStructureSize`.
    pub acceleration_structure_size: usize,
    /// Metal field `MTL::AccelerationStructureSizes::buildScratchBufferSize`.
    pub build_scratch_buffer_size: usize,
    /// Metal field `MTL::AccelerationStructureSizes::refitScratchBufferSize`.
    pub refit_scratch_buffer_size: usize,
}

/// Safe value representation for Metal `MTL::AccelerationStructureUserIDInstanceDescriptor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccelerationStructureUserIDInstanceDescriptor {
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::transformationMatrix`.
    pub transformation_matrix: PackedFloat4x3,
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::options`.
    pub options: AccelerationStructureInstanceOptions,
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::mask`.
    pub mask: u32,
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::intersectionFunctionTableOffset`.
    pub intersection_function_table_offset: u32,
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::accelerationStructureIndex`.
    pub acceleration_structure_index: u32,
    /// Metal field `MTL::AccelerationStructureUserIDInstanceDescriptor::userID`.
    pub user_id: u32,
}

/// Safe value representation for Metal `MTL::AxisAlignedBoundingBox`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisAlignedBoundingBox {
    /// Metal field `MTL::AxisAlignedBoundingBox::min`.
    pub min: PackedFloat3,
    /// Metal field `MTL::AxisAlignedBoundingBox::max`.
    pub max: PackedFloat3,
}

/// Safe value representation for Metal `MTL::ComponentTransform`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentTransform {
    /// Metal field `MTL::ComponentTransform::scale`.
    pub scale: PackedFloat3,
    /// Metal field `MTL::ComponentTransform::shear`.
    pub shear: PackedFloat3,
    /// Metal field `MTL::ComponentTransform::pivot`.
    pub pivot: PackedFloat3,
    /// Metal field `MTL::ComponentTransform::rotation`.
    pub rotation: PackedFloatQuaternion,
    /// Metal field `MTL::ComponentTransform::translation`.
    pub translation: PackedFloat3,
}

/// Safe value representation for Metal `MTL::CounterResultStageUtilization`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CounterResultStageUtilization {
    /// Metal field `MTL::CounterResultStageUtilization::totalCycles`.
    pub total_cycles: u64,
    /// Metal field `MTL::CounterResultStageUtilization::vertexCycles`.
    pub vertex_cycles: u64,
    /// Metal field `MTL::CounterResultStageUtilization::tessellationCycles`.
    pub tessellation_cycles: u64,
    /// Metal field `MTL::CounterResultStageUtilization::postTessellationVertexCycles`.
    pub post_tessellation_vertex_cycles: u64,
    /// Metal field `MTL::CounterResultStageUtilization::fragmentCycles`.
    pub fragment_cycles: u64,
    /// Metal field `MTL::CounterResultStageUtilization::renderTargetCycles`.
    pub render_target_cycles: u64,
}

/// Safe value representation for Metal `MTL::CounterResultStatistic`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CounterResultStatistic {
    /// Metal field `MTL::CounterResultStatistic::tessellationInputPatches`.
    pub tessellation_input_patches: u64,
    /// Metal field `MTL::CounterResultStatistic::vertexInvocations`.
    pub vertex_invocations: u64,
    /// Metal field `MTL::CounterResultStatistic::postTessellationVertexInvocations`.
    pub post_tessellation_vertex_invocations: u64,
    /// Metal field `MTL::CounterResultStatistic::clipperInvocations`.
    pub clipper_invocations: u64,
    /// Metal field `MTL::CounterResultStatistic::clipperPrimitivesOut`.
    pub clipper_primitives_out: u64,
    /// Metal field `MTL::CounterResultStatistic::fragmentInvocations`.
    pub fragment_invocations: u64,
    /// Metal field `MTL::CounterResultStatistic::fragmentsPassed`.
    pub fragments_passed: u64,
    /// Metal field `MTL::CounterResultStatistic::computeKernelInvocations`.
    pub compute_kernel_invocations: u64,
}

/// Safe value representation for Metal `MTL::CounterResultTimestamp`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CounterResultTimestamp {
    /// Metal field `MTL::CounterResultTimestamp::timestamp`.
    pub timestamp: u64,
}

/// Safe value representation for Metal `MTL::DispatchThreadgroupsIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DispatchThreadgroupsIndirectArguments {
    /// Metal field `MTL::DispatchThreadgroupsIndirectArguments::threadgroupsPerGrid`.
    pub threadgroups_per_grid: [u32; 3],
}

/// Safe value representation for Metal `MTL::DispatchThreadsIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DispatchThreadsIndirectArguments {
    /// Metal field `MTL::DispatchThreadsIndirectArguments::threadsPerGrid`.
    pub threads_per_grid: [u32; 3],
    /// Metal field `MTL::DispatchThreadsIndirectArguments::threadsPerThreadgroup`.
    pub threads_per_threadgroup: [u32; 3],
}

/// Safe value representation for Metal `MTL::DrawIndexedPrimitivesIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawIndexedPrimitivesIndirectArguments {
    /// Metal field `MTL::DrawIndexedPrimitivesIndirectArguments::indexCount`.
    pub index_count: u32,
    /// Metal field `MTL::DrawIndexedPrimitivesIndirectArguments::instanceCount`.
    pub instance_count: u32,
    /// Metal field `MTL::DrawIndexedPrimitivesIndirectArguments::indexStart`.
    pub index_start: u32,
    /// Metal field `MTL::DrawIndexedPrimitivesIndirectArguments::baseVertex`.
    pub base_vertex: i32,
    /// Metal field `MTL::DrawIndexedPrimitivesIndirectArguments::baseInstance`.
    pub base_instance: u32,
}

/// Safe value representation for Metal `MTL::DrawPatchIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawPatchIndirectArguments {
    /// Metal field `MTL::DrawPatchIndirectArguments::patchCount`.
    pub patch_count: u32,
    /// Metal field `MTL::DrawPatchIndirectArguments::instanceCount`.
    pub instance_count: u32,
    /// Metal field `MTL::DrawPatchIndirectArguments::patchStart`.
    pub patch_start: u32,
    /// Metal field `MTL::DrawPatchIndirectArguments::baseInstance`.
    pub base_instance: u32,
}

/// Safe value representation for Metal `MTL::DrawPrimitivesIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawPrimitivesIndirectArguments {
    /// Metal field `MTL::DrawPrimitivesIndirectArguments::vertexCount`.
    pub vertex_count: u32,
    /// Metal field `MTL::DrawPrimitivesIndirectArguments::instanceCount`.
    pub instance_count: u32,
    /// Metal field `MTL::DrawPrimitivesIndirectArguments::vertexStart`.
    pub vertex_start: u32,
    /// Metal field `MTL::DrawPrimitivesIndirectArguments::baseInstance`.
    pub base_instance: u32,
}

/// Safe value representation for Metal `MTL::IndirectAccelerationStructureInstanceDescriptor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndirectAccelerationStructureInstanceDescriptor {
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::transformationMatrix`.
    pub transformation_matrix: PackedFloat4x3,
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::options`.
    pub options: AccelerationStructureInstanceOptions,
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::mask`.
    pub mask: u32,
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::intersectionFunctionTableOffset`.
    pub intersection_function_table_offset: u32,
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::userID`.
    pub user_id: u32,
    /// Metal field `MTL::IndirectAccelerationStructureInstanceDescriptor::accelerationStructureID`.
    pub acceleration_structure_id: ResourceID,
}

/// Safe value representation for Metal `MTL::IndirectAccelerationStructureMotionInstanceDescriptor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndirectAccelerationStructureMotionInstanceDescriptor {
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::options`.
    pub options: AccelerationStructureInstanceOptions,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::mask`.
    pub mask: u32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::intersectionFunctionTableOffset`.
    pub intersection_function_table_offset: u32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::userID`.
    pub user_id: u32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::accelerationStructureID`.
    pub acceleration_structure_id: ResourceID,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionTransformsStartIndex`.
    pub motion_transforms_start_index: u32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionTransformsCount`.
    pub motion_transforms_count: u32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionStartBorderMode`.
    pub motion_start_border_mode: MotionBorderMode,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionEndBorderMode`.
    pub motion_end_border_mode: MotionBorderMode,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionStartTime`.
    pub motion_start_time: f32,
    /// Metal field `MTL::IndirectAccelerationStructureMotionInstanceDescriptor::motionEndTime`.
    pub motion_end_time: f32,
}

/// Safe value representation for Metal `MTL::IndirectCommandBufferExecutionRange`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndirectCommandBufferExecutionRange {
    /// Metal field `MTL::IndirectCommandBufferExecutionRange::location`.
    pub location: u32,
    /// Metal field `MTL::IndirectCommandBufferExecutionRange::length`.
    pub length: u32,
}

/// Safe value representation for Metal `MTL::IntersectionFunctionBufferArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntersectionFunctionBufferArguments {
    /// Metal field `MTL::IntersectionFunctionBufferArguments::intersectionFunctionBuffer`.
    pub intersection_function_buffer: u64,
    /// Metal field `MTL::IntersectionFunctionBufferArguments::intersectionFunctionBufferSize`.
    pub intersection_function_buffer_size: u64,
    /// Metal field `MTL::IntersectionFunctionBufferArguments::intersectionFunctionStride`.
    pub intersection_function_stride: u64,
}

/// Safe value representation for Metal `MTL::MapIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapIndirectArguments {
    /// Metal field `MTL::MapIndirectArguments::regionOriginX`.
    pub region_origin_x: u32,
    /// Metal field `MTL::MapIndirectArguments::regionOriginY`.
    pub region_origin_y: u32,
    /// Metal field `MTL::MapIndirectArguments::regionOriginZ`.
    pub region_origin_z: u32,
    /// Metal field `MTL::MapIndirectArguments::regionSizeWidth`.
    pub region_size_width: u32,
    /// Metal field `MTL::MapIndirectArguments::regionSizeHeight`.
    pub region_size_height: u32,
    /// Metal field `MTL::MapIndirectArguments::regionSizeDepth`.
    pub region_size_depth: u32,
    /// Metal field `MTL::MapIndirectArguments::mipMapLevel`.
    pub mip_map_level: u32,
    /// Metal field `MTL::MapIndirectArguments::sliceId`.
    pub slice_id: u32,
}

/// Safe value representation for Metal `MTL::PackedFloat3`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PackedFloat3(pub [f32; 3]);

impl PackedFloat3 {
    /// Creates a packed value from its individual components.
    #[must_use]
    pub const fn new(values: [f32; 3]) -> Self {
        Self(values)
    }

    /// Creates a packed value from separate components.
    #[must_use]
    pub const fn from_components(x: f32, y: f32, z: f32) -> Self {
        Self([x, y, z])
    }

    /// Returns the packed components as an array.
    #[must_use]
    pub const fn into_array(self) -> [f32; 3] {
        self.0
    }
}

impl std::ops::Index<usize> for PackedFloat3 {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::IndexMut<usize> for PackedFloat3 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

/// Safe value representation for Metal `MTL::PackedFloat4x3`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackedFloat4x3 {
    /// Metal field `MTL::PackedFloat4x3::columns`.
    pub columns: [PackedFloat3; 4],
}

impl PackedFloat4x3 {
    /// Creates a packed matrix from its four columns.
    #[must_use]
    pub const fn new(columns: [PackedFloat3; 4]) -> Self {
        Self { columns }
    }
}

impl std::ops::Index<usize> for PackedFloat4x3 {
    type Output = PackedFloat3;

    fn index(&self, index: usize) -> &Self::Output {
        &self.columns[index]
    }
}

impl std::ops::IndexMut<usize> for PackedFloat4x3 {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.columns[index]
    }
}

/// Safe value representation for Metal `MTL::PackedFloatQuaternion`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PackedFloatQuaternion(pub [f32; 4]);

impl PackedFloatQuaternion {
    /// Creates a packed value from its individual components.
    #[must_use]
    pub const fn new(values: [f32; 4]) -> Self {
        Self(values)
    }

    /// Creates a packed value from separate components.
    #[must_use]
    pub const fn from_components(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self([x, y, z, w])
    }

    /// Returns the packed components as an array.
    #[must_use]
    pub const fn into_array(self) -> [f32; 4] {
        self.0
    }
}

impl std::ops::Index<usize> for PackedFloatQuaternion {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl std::ops::IndexMut<usize> for PackedFloatQuaternion {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

/// Safe value representation for Metal `MTL::QuadTessellationFactorsHalf`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadTessellationFactorsHalf {
    /// Metal field `MTL::QuadTessellationFactorsHalf::edgeTessellationFactor`.
    pub edge_tessellation_factor: [u16; 4],
    /// Metal field `MTL::QuadTessellationFactorsHalf::insideTessellationFactor`.
    pub inside_tessellation_factor: [u16; 2],
}

/// Safe value representation for Metal `MTL::ResourceID`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceID {
    /// Metal field `MTL::ResourceID::_impl`.
    pub _impl: u64,
}

/// Safe value representation for Metal `MTL::SamplePosition`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SamplePosition {
    /// Metal field `MTL::SamplePosition::x`.
    pub x: f32,
    /// Metal field `MTL::SamplePosition::y`.
    pub y: f32,
}

/// Safe value representation for Metal `MTL::ScissorRect`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScissorRect {
    /// Metal field `MTL::ScissorRect::x`.
    pub x: usize,
    /// Metal field `MTL::ScissorRect::y`.
    pub y: usize,
    /// Metal field `MTL::ScissorRect::width`.
    pub width: usize,
    /// Metal field `MTL::ScissorRect::height`.
    pub height: usize,
}

/// Safe value representation for Metal `MTL::SizeAndAlign`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SizeAndAlign {
    /// Metal field `MTL::SizeAndAlign::size`.
    pub size: usize,
    /// Metal field `MTL::SizeAndAlign::align`.
    pub align: usize,
}

/// Safe value representation for Metal `MTL::StageInRegionIndirectArguments`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageInRegionIndirectArguments {
    /// Metal field `MTL::StageInRegionIndirectArguments::stageInOrigin`.
    pub stage_in_origin: [u32; 3],
    /// Metal field `MTL::StageInRegionIndirectArguments::stageInSize`.
    pub stage_in_size: [u32; 3],
}

/// Safe value representation for Metal `MTL::TextureSwizzleChannels`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureSwizzleChannels {
    /// Metal field `MTL::TextureSwizzleChannels::red`.
    pub red: TextureSwizzle,
    /// Metal field `MTL::TextureSwizzleChannels::green`.
    pub green: TextureSwizzle,
    /// Metal field `MTL::TextureSwizzleChannels::blue`.
    pub blue: TextureSwizzle,
    /// Metal field `MTL::TextureSwizzleChannels::alpha`.
    pub alpha: TextureSwizzle,
}

/// Safe value representation for Metal `MTL::TriangleTessellationFactorsHalf`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangleTessellationFactorsHalf {
    /// Metal field `MTL::TriangleTessellationFactorsHalf::edgeTessellationFactor`.
    pub edge_tessellation_factor: [u16; 3],
    /// Metal field `MTL::TriangleTessellationFactorsHalf::insideTessellationFactor`.
    pub inside_tessellation_factor: u16,
}

/// Safe value representation for Metal `MTL::VertexAmplificationViewMapping`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VertexAmplificationViewMapping {
    /// Metal field `MTL::VertexAmplificationViewMapping::viewportArrayIndexOffset`.
    pub viewport_array_index_offset: u32,
    /// Metal field `MTL::VertexAmplificationViewMapping::renderTargetArrayIndexOffset`.
    pub render_target_array_index_offset: u32,
}

/// Safe value representation for Metal `NS::OperatingSystemVersion`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OperatingSystemVersion {
    /// Metal field `NS::OperatingSystemVersion::majorVersion`.
    pub major_version: isize,
    /// Metal field `NS::OperatingSystemVersion::minorVersion`.
    pub minor_version: isize,
    /// Metal field `NS::OperatingSystemVersion::patchVersion`.
    pub patch_version: isize,
}

/// Safe value representation for Metal `NS::Range`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Range {
    /// Metal field `NS::Range::location`.
    pub location: usize,
    /// Metal field `NS::Range::length`.
    pub length: usize,
}

impl Range {
    /// Creates a checked Foundation-style range.
    #[must_use]
    pub const fn new(location: usize, length: usize) -> Option<Self> {
        if location.checked_add(length).is_some() {
            Some(Self { location, length })
        } else {
            None
        }
    }

    /// Returns whether the location is inside the half-open range.
    #[must_use]
    pub const fn contains(&self, location: usize) -> bool {
        location >= self.location && location < self.location.saturating_add(self.length)
    }

    /// Returns the exclusive range end when it does not overflow.
    #[must_use]
    pub const fn end(&self) -> Option<usize> {
        self.location.checked_add(self.length)
    }
}
