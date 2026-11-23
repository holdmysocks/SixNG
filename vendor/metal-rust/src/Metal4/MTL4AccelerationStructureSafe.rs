//! Safe Metal 4 acceleration-structure resource ranges and geometry lists.

use std::ops::Range;

use crate::metal::Buffer;

use super::{
    AccelerationStructureBoundingBoxGeometryDescriptor,
    AccelerationStructureCurveGeometryDescriptor, AccelerationStructureGeometryDescriptor,
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    AccelerationStructureMotionCurveGeometryDescriptor,
    AccelerationStructureMotionTriangleGeometryDescriptor,
    AccelerationStructureTriangleGeometryDescriptor,
    IndirectInstanceAccelerationStructureDescriptor, InstanceAccelerationStructureDescriptor,
    PrimitiveAccelerationStructureDescriptor,
};

/// An opaque snapshot of a resource range encoded in a Metal 4 descriptor.
///
/// Its GPU virtual address is intentionally not exposed or reusable.  New
/// ranges must always be derived from a live [`Buffer`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccelerationStructureBufferRange {
    inner: metal_rust_ffi::AccelerationStructureBufferRange,
}

impl AccelerationStructureBufferRange {
    fn from_ffi(inner: metal_rust_ffi::AccelerationStructureBufferRange) -> Self {
        Self { inner }
    }

    /// Returns the encoded byte length.
    pub const fn len(self) -> u64 {
        self.inner.len()
    }

    /// Returns whether this snapshot describes an empty range.
    pub const fn is_empty(self) -> bool {
        self.inner.is_empty()
    }
}

/// A heterogeneous borrowed geometry descriptor.
#[derive(Clone, Copy)]
pub enum AccelerationStructureGeometry<'a> {
    /// A static triangle geometry descriptor.
    Triangle(&'a AccelerationStructureTriangleGeometryDescriptor),
    /// A static axis-aligned bounding-box geometry descriptor.
    BoundingBox(&'a AccelerationStructureBoundingBoxGeometryDescriptor),
    /// A motion triangle geometry descriptor.
    MotionTriangle(&'a AccelerationStructureMotionTriangleGeometryDescriptor),
    /// A motion axis-aligned bounding-box geometry descriptor.
    MotionBoundingBox(&'a AccelerationStructureMotionBoundingBoxGeometryDescriptor),
    /// A static curve geometry descriptor.
    Curve(&'a AccelerationStructureCurveGeometryDescriptor),
    /// A motion curve geometry descriptor.
    MotionCurve(&'a AccelerationStructureMotionCurveGeometryDescriptor),
}

macro_rules! range_property {
    ($type:ty, $get:ident, $set:ident) => {
        impl $type {
            #[doc = concat!("Returns an opaque snapshot from `", stringify!($get), "`.")]
            pub fn $get(&self) -> Result<AccelerationStructureBufferRange, crate::Error> {
                self.inner
                    .$get()
                    .map(AccelerationStructureBufferRange::from_ffi)
                    .map_err(crate::Error::from_ffi)
            }

            #[doc = concat!("Updates `", stringify!($set), "` from a checked live buffer region, or clears it.")]
            pub fn $set(
                &self,
                value: Option<(&Buffer, Range<usize>)>,
            ) -> Result<(), crate::Error> {
                self.inner
                    .$set(value.map(|(buffer, range)| (&buffer.inner, range)))
                    .map_err(crate::Error::from_ffi)
            }
        }
    };
}

range_property!(
    AccelerationStructureGeometryDescriptor,
    primitive_data_buffer,
    set_primitive_data_buffer
);
range_property!(
    AccelerationStructureTriangleGeometryDescriptor,
    index_buffer,
    set_index_buffer
);
range_property!(
    AccelerationStructureTriangleGeometryDescriptor,
    transformation_matrix_buffer,
    set_transformation_matrix_buffer
);
range_property!(
    AccelerationStructureTriangleGeometryDescriptor,
    vertex_buffer,
    set_vertex_buffer
);
range_property!(
    AccelerationStructureBoundingBoxGeometryDescriptor,
    bounding_box_buffer,
    set_bounding_box_buffer
);
range_property!(
    AccelerationStructureMotionTriangleGeometryDescriptor,
    index_buffer,
    set_index_buffer
);
range_property!(
    AccelerationStructureMotionTriangleGeometryDescriptor,
    transformation_matrix_buffer,
    set_transformation_matrix_buffer
);
range_property!(
    AccelerationStructureMotionTriangleGeometryDescriptor,
    vertex_buffers,
    set_vertex_buffers
);
range_property!(
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    bounding_box_buffers,
    set_bounding_box_buffers
);
range_property!(
    AccelerationStructureCurveGeometryDescriptor,
    control_point_buffer,
    set_control_point_buffer
);
range_property!(
    AccelerationStructureCurveGeometryDescriptor,
    index_buffer,
    set_index_buffer
);
range_property!(
    AccelerationStructureCurveGeometryDescriptor,
    radius_buffer,
    set_radius_buffer
);
range_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    control_point_buffers,
    set_control_point_buffers
);
range_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    index_buffer,
    set_index_buffer
);
range_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    radius_buffers,
    set_radius_buffers
);
range_property!(
    InstanceAccelerationStructureDescriptor,
    instance_descriptor_buffer,
    set_instance_descriptor_buffer
);
range_property!(
    InstanceAccelerationStructureDescriptor,
    motion_transform_buffer,
    set_motion_transform_buffer
);
range_property!(
    IndirectInstanceAccelerationStructureDescriptor,
    instance_count_buffer,
    set_instance_count_buffer
);
range_property!(
    IndirectInstanceAccelerationStructureDescriptor,
    instance_descriptor_buffer,
    set_instance_descriptor_buffer
);
range_property!(
    IndirectInstanceAccelerationStructureDescriptor,
    motion_transform_buffer,
    set_motion_transform_buffer
);
range_property!(
    IndirectInstanceAccelerationStructureDescriptor,
    motion_transform_count_buffer,
    set_motion_transform_count_buffer
);

impl PrimitiveAccelerationStructureDescriptor {
    /// Replaces the descriptor's geometry array with a non-empty safe slice.
    pub fn set_geometry_descriptor_slice(
        &self,
        descriptors: &[AccelerationStructureGeometry<'_>],
    ) -> Result<(), crate::Error> {
        let descriptors: Vec<_> = descriptors
            .iter()
            .map(|descriptor| match descriptor {
                AccelerationStructureGeometry::Triangle(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::Triangle(&value.inner)
                }
                AccelerationStructureGeometry::BoundingBox(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::BoundingBox(&value.inner)
                }
                AccelerationStructureGeometry::MotionTriangle(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::MotionTriangle(
                        &value.inner,
                    )
                }
                AccelerationStructureGeometry::MotionBoundingBox(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::MotionBoundingBox(
                        &value.inner,
                    )
                }
                AccelerationStructureGeometry::Curve(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::Curve(&value.inner)
                }
                AccelerationStructureGeometry::MotionCurve(value) => {
                    metal_rust_ffi::Metal4AccelerationStructureGeometry::MotionCurve(&value.inner)
                }
            })
            .collect();
        self.inner
            .set_geometry_descriptor_slice(&descriptors)
            .map_err(crate::Error::from_ffi)
    }

    /// Returns owned base geometry wrappers for the configured geometry list.
    pub fn geometry_descriptor_vec(
        &self,
    ) -> Result<Vec<AccelerationStructureGeometryDescriptor>, crate::Error> {
        self.inner
            .geometry_descriptor_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(AccelerationStructureGeometryDescriptor::from_ffi)
                    .collect()
            })
            .map_err(crate::Error::from_ffi)
    }
}
