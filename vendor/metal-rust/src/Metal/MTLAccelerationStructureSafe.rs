//! Safe acceleration-structure command encoding.

use super::{
    AccelerationStructure, AccelerationStructureBoundingBoxGeometryDescriptor,
    AccelerationStructureCurveGeometryDescriptor, AccelerationStructureDescriptor,
    AccelerationStructureGeometryDescriptor,
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    AccelerationStructureMotionCurveGeometryDescriptor,
    AccelerationStructureMotionTriangleGeometryDescriptor, AccelerationStructurePassDescriptor,
    AccelerationStructurePassSampleBufferAttachmentDescriptor,
    AccelerationStructurePassSampleBufferAttachmentDescriptorArray,
    AccelerationStructureRefitOptions, AccelerationStructureTriangleGeometryDescriptor, Buffer,
    CommandBuffer, CounterSampleBuffer, Fence, Heap,
    IndirectInstanceAccelerationStructureDescriptor, InstanceAccelerationStructureDescriptor,
    PrimitiveAccelerationStructureDescriptor, ResourceID, ResourceUsage, Texture,
};
use crate::Error;

/// Conservatively supported acceleration-pass sample-buffer attachment slots.
pub const ACCELERATION_STRUCTURE_SAMPLE_ATTACHMENT_CAPACITY: usize = 4;

/// An acceleration-structure encoder that exclusively borrows its command buffer.
pub struct AccelerationStructureEncoder<'a> {
    pub(crate) inner: metal_rust_ffi::AccelerationStructureEncoder<'a>,
}

impl<'a> AccelerationStructureEncoder<'a> {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::AccelerationStructureEncoder<'a>) -> Self {
        Self { inner }
    }
}

impl CommandBuffer {
    /// Begins an acceleration-structure encoder while borrowing this command buffer exclusively.
    pub fn acceleration_structure_encoder(
        &mut self,
    ) -> Result<AccelerationStructureEncoder<'_>, Error> {
        self.inner
            .acceleration_structure_encoder()
            .map(AccelerationStructureEncoder::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Begins a descriptor-configured acceleration-structure encoder with an exclusive borrow.
    pub fn acceleration_structure_encoder_with_descriptor(
        &mut self,
        descriptor: &AccelerationStructurePassDescriptor,
    ) -> Result<AccelerationStructureEncoder<'_>, Error> {
        self.inner
            .acceleration_structure_encoder_with_descriptor(&descriptor.inner)
            .map(AccelerationStructureEncoder::from_ffi)
            .map_err(Error::from_ffi)
    }
}

/// A concrete descriptor accepted by acceleration-structure build and refit commands.
#[derive(Clone, Copy)]
pub enum AccelerationStructureBuildDescriptor<'a> {
    /// The common acceleration-structure descriptor.
    Base(&'a AccelerationStructureDescriptor),
    /// A bottom-level primitive acceleration-structure descriptor.
    Primitive(&'a PrimitiveAccelerationStructureDescriptor),
    /// A top-level instance acceleration-structure descriptor.
    Instance(&'a InstanceAccelerationStructureDescriptor),
    /// An indirect top-level instance acceleration-structure descriptor.
    IndirectInstance(&'a IndirectInstanceAccelerationStructureDescriptor),
}

impl<'a> AccelerationStructureBuildDescriptor<'a> {
    fn as_ffi(self) -> metal_rust_ffi::AccelerationStructureBuildDescriptor<'a> {
        match self {
            Self::Base(value) => {
                metal_rust_ffi::AccelerationStructureBuildDescriptor::Base(&value.inner)
            }
            Self::Primitive(value) => {
                metal_rust_ffi::AccelerationStructureBuildDescriptor::Primitive(&value.inner)
            }
            Self::Instance(value) => {
                metal_rust_ffi::AccelerationStructureBuildDescriptor::Instance(&value.inner)
            }
            Self::IndirectInstance(value) => {
                metal_rust_ffi::AccelerationStructureBuildDescriptor::IndirectInstance(&value.inner)
            }
        }
    }
}

/// A Metal resource that can be declared for acceleration-structure encoding.
#[derive(Clone, Copy)]
pub enum AccelerationStructureResource<'a> {
    /// A buffer resource.
    Buffer(&'a Buffer),
    /// A texture resource.
    Texture(&'a Texture),
    /// An acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl<'a> AccelerationStructureResource<'a> {
    fn as_ffi(self) -> metal_rust_ffi::AccelerationStructureResource<'a> {
        match self {
            Self::Buffer(value) => {
                metal_rust_ffi::AccelerationStructureResource::Buffer(&value.inner)
            }
            Self::Texture(value) => {
                metal_rust_ffi::AccelerationStructureResource::Texture(&value.inner)
            }
            Self::AccelerationStructure(value) => {
                metal_rust_ffi::AccelerationStructureResource::AccelerationStructure(&value.inner)
            }
        }
    }
}

/// A concrete geometry descriptor used by a primitive acceleration structure.
#[derive(Clone, Copy)]
pub enum AccelerationStructureGeometry<'a> {
    /// A triangle geometry descriptor.
    Triangle(&'a AccelerationStructureTriangleGeometryDescriptor),
    /// An axis-aligned bounding-box geometry descriptor.
    BoundingBox(&'a AccelerationStructureBoundingBoxGeometryDescriptor),
    /// A curve geometry descriptor.
    Curve(&'a AccelerationStructureCurveGeometryDescriptor),
    /// A motion triangle geometry descriptor.
    MotionTriangle(&'a AccelerationStructureMotionTriangleGeometryDescriptor),
    /// A motion bounding-box geometry descriptor.
    MotionBoundingBox(&'a AccelerationStructureMotionBoundingBoxGeometryDescriptor),
    /// A motion curve geometry descriptor.
    MotionCurve(&'a AccelerationStructureMotionCurveGeometryDescriptor),
}

impl<'a> AccelerationStructureGeometry<'a> {
    fn as_ffi(self) -> metal_rust_ffi::AccelerationStructureGeometry<'a> {
        match self {
            Self::Triangle(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::Triangle(&value.inner)
            }
            Self::BoundingBox(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::BoundingBox(&value.inner)
            }
            Self::Curve(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::Curve(&value.inner)
            }
            Self::MotionTriangle(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::MotionTriangle(&value.inner)
            }
            Self::MotionBoundingBox(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::MotionBoundingBox(&value.inner)
            }
            Self::MotionCurve(value) => {
                metal_rust_ffi::AccelerationStructureGeometry::MotionCurve(&value.inner)
            }
        }
    }
}

impl AccelerationStructure {
    /// Returns this acceleration structure's GPU resource identifier when available.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }
}

impl AccelerationStructurePassSampleBufferAttachmentDescriptorArray {
    /// Returns an owned attachment descriptor from a checked fixed-capacity slot.
    pub fn attachment(
        &self,
        index: usize,
    ) -> Result<AccelerationStructurePassSampleBufferAttachmentDescriptor, Error> {
        self.inner
            .attachment(index)
            .map(AccelerationStructurePassSampleBufferAttachmentDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Sets or clears an attachment descriptor in a checked fixed-capacity slot.
    pub fn set_attachment(
        &self,
        index: usize,
        attachment: Option<&AccelerationStructurePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_attachment(index, attachment.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl PrimitiveAccelerationStructureDescriptor {
    /// Returns owned base wrappers for all configured geometry descriptors.
    pub fn geometry_descriptors_vec(
        &self,
    ) -> Result<Vec<AccelerationStructureGeometryDescriptor>, Error> {
        self.inner
            .geometry_descriptors_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(AccelerationStructureGeometryDescriptor::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces geometry descriptors from a safe borrowed slice.
    pub fn set_geometry_descriptors_slice(
        &self,
        values: &[AccelerationStructureGeometry<'_>],
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .copied()
            .map(AccelerationStructureGeometry::as_ffi)
            .collect::<Vec<_>>();
        self.inner
            .set_geometry_descriptors_slice(&values)
            .map_err(Error::from_ffi)
    }
}

macro_rules! buffer_array_property {
    ($owner:ty, $getter_fn:ident, $setter_fn:ident) => {
        impl $owner {
            #[doc = concat!("Returns owned safe buffers from `", stringify!($getter_fn), "`.")]
            pub fn $getter_fn(&self) -> Result<Vec<Buffer>, Error> {
                self.inner
                    .$getter_fn()
                    .map(|values| values.into_iter().map(Buffer::from_ffi).collect())
                    .map_err(Error::from_ffi)
            }

            #[doc = concat!("Writes borrowed safe buffers through `", stringify!($setter_fn), "`.")]
            pub fn $setter_fn(&self, values: &[&Buffer]) -> Result<(), Error> {
                let values = values.iter().map(|value| &value.inner).collect::<Vec<_>>();
                self.inner.$setter_fn(&values).map_err(Error::from_ffi)
            }
        }
    };
}

buffer_array_property!(
    AccelerationStructureMotionTriangleGeometryDescriptor,
    vertex_buffers_vec,
    set_vertex_buffers_slice
);
buffer_array_property!(
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    bounding_box_buffers_vec,
    set_bounding_box_buffers_slice
);
buffer_array_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    control_point_buffers_vec,
    set_control_point_buffers_slice
);
buffer_array_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    radius_buffers_vec,
    set_radius_buffers_slice
);

impl InstanceAccelerationStructureDescriptor {
    /// Returns the owned acceleration structures referenced by this descriptor.
    pub fn instanced_acceleration_structures_vec(
        &self,
    ) -> Result<Vec<AccelerationStructure>, Error> {
        self.inner
            .instanced_acceleration_structures_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(AccelerationStructure::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces referenced acceleration structures from a safe borrowed slice.
    pub fn set_instanced_acceleration_structures_slice(
        &self,
        values: &[&AccelerationStructure],
    ) -> Result<(), Error> {
        let values = values.iter().map(|value| &value.inner).collect::<Vec<_>>();
        self.inner
            .set_instanced_acceleration_structures_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl AccelerationStructureEncoder<'_> {
    /// Encodes a checked acceleration-structure build.
    pub fn build(
        &self,
        destination: &AccelerationStructure,
        descriptor: AccelerationStructureBuildDescriptor<'_>,
        required_acceleration_structure_size: usize,
        scratch: &Buffer,
        scratch_offset: usize,
        required_scratch_size: usize,
    ) -> Result<(), Error> {
        self.inner
            .build(
                &destination.inner,
                descriptor.as_ffi(),
                required_acceleration_structure_size,
                &scratch.inner,
                scratch_offset,
                required_scratch_size,
            )
            .map_err(Error::from_ffi)
    }

    /// Encodes an acceleration-structure refit with checked destination and scratch capacities.
    #[allow(clippy::too_many_arguments)]
    pub fn refit(
        &self,
        source: &AccelerationStructure,
        descriptor: AccelerationStructureBuildDescriptor<'_>,
        destination: Option<&AccelerationStructure>,
        required_destination_size: usize,
        scratch: &Buffer,
        scratch_offset: usize,
        required_scratch_size: usize,
        options: AccelerationStructureRefitOptions,
    ) -> Result<(), Error> {
        self.inner
            .refit(
                &source.inner,
                descriptor.as_ffi(),
                destination.map(|value| &value.inner),
                required_destination_size,
                &scratch.inner,
                scratch_offset,
                required_scratch_size,
                options,
            )
            .map_err(Error::from_ffi)
    }

    /// Copies an acceleration structure after checking identity overlap and destination size.
    pub fn copy(
        &self,
        source: &AccelerationStructure,
        destination: &AccelerationStructure,
    ) -> Result<(), Error> {
        self.inner
            .copy(&source.inner, &destination.inner)
            .map_err(Error::from_ffi)
    }

    /// Writes the compacted size as a checked 32-bit value.
    pub fn write_compacted_size_u32(
        &self,
        source: &AccelerationStructure,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .write_compacted_size_u32(&source.inner, &destination.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Writes the compacted size as a checked 64-bit value when supported.
    pub fn write_compacted_size_u64(
        &self,
        source: &AccelerationStructure,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .write_compacted_size_u64(&source.inner, &destination.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Copies and compacts an acceleration structure into a checked destination allocation.
    pub fn copy_and_compact(
        &self,
        source: &AccelerationStructure,
        destination: &AccelerationStructure,
        compacted_size: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_and_compact(&source.inner, &destination.inner, compacted_size)
            .map_err(Error::from_ffi)
    }

    /// Updates a fence when supported by the encoder.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .update_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Waits for a fence when supported by the encoder.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.inner
            .wait_for_fence(&fence.inner)
            .map_err(Error::from_ffi)
    }

    /// Declares one checked resource usage.
    pub fn use_resource(
        &self,
        resource: AccelerationStructureResource<'_>,
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        self.inner
            .use_resource(resource.as_ffi(), usage)
            .map_err(Error::from_ffi)
    }

    /// Declares multiple resource usages without exposing the native pointer-array ABI.
    pub fn use_resources(
        &self,
        resources: &[AccelerationStructureResource<'_>],
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        let resources = resources
            .iter()
            .copied()
            .map(AccelerationStructureResource::as_ffi)
            .collect::<Vec<_>>();
        self.inner
            .use_resources(&resources, usage)
            .map_err(Error::from_ffi)
    }

    /// Declares one heap usage.
    pub fn use_heap(&self, heap: &Heap) -> Result<(), Error> {
        self.inner.use_heap(&heap.inner).map_err(Error::from_ffi)
    }

    /// Declares multiple heap usages without exposing the native pointer-array ABI.
    pub fn use_heaps(&self, heaps: &[&Heap]) -> Result<(), Error> {
        let heaps = heaps.iter().map(|heap| &heap.inner).collect::<Vec<_>>();
        self.inner.use_heaps(&heaps).map_err(Error::from_ffi)
    }

    /// Samples a counter buffer after checking the destination sample index.
    pub fn sample_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        self.inner
            .sample_counters(&sample_buffer.inner, sample_index, barrier)
            .map_err(Error::from_ffi)
    }
}
