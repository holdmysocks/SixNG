//! Safe resource-range and collection boundaries for Metal 4 acceleration structures.
//!
//! `MTL4BufferRange` contains a GPU virtual address.  Accepting that value from
//! callers would make an otherwise safe setter capable of scheduling arbitrary
//! GPU memory access.  This module instead derives ranges from retained
//! [`Buffer`] objects and associates those buffers with their descriptors.

use std::ops::Range;
use std::ptr;

use objc2::ffi::{OBJC_ASSOCIATION_RETAIN_NONATOMIC, objc_setAssociatedObject};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_metal::MTL4BufferRange;

use crate::foundation::Error;
use crate::metal::Buffer;
use crate::metal::generated_object_types::metal4;

/// An opaque snapshot of a buffer range stored in a Metal 4 descriptor.
///
/// The GPU virtual address deliberately remains private.  A snapshot is useful
/// for validation and diagnostics, but cannot be fed back into a descriptor.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AccelerationStructureBufferRange {
    address: u64,
    length: u64,
}

impl std::fmt::Debug for AccelerationStructureBufferRange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccelerationStructureBufferRange")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

impl AccelerationStructureBufferRange {
    /// Returns the encoded byte length.
    pub const fn len(self) -> u64 {
        self.length
    }

    /// Returns whether the descriptor contains an empty range.
    pub const fn is_empty(self) -> bool {
        self.length == 0
    }
}

/// A borrowed heterogeneous geometry descriptor accepted by a primitive AS.
#[derive(Clone, Copy)]
pub enum Metal4AccelerationStructureGeometry<'a> {
    /// A static triangle geometry descriptor.
    Triangle(&'a metal4::AccelerationStructureTriangleGeometryDescriptor),
    /// A static axis-aligned bounding-box geometry descriptor.
    BoundingBox(&'a metal4::AccelerationStructureBoundingBoxGeometryDescriptor),
    /// A motion triangle geometry descriptor.
    MotionTriangle(&'a metal4::AccelerationStructureMotionTriangleGeometryDescriptor),
    /// A motion axis-aligned bounding-box geometry descriptor.
    MotionBoundingBox(&'a metal4::AccelerationStructureMotionBoundingBoxGeometryDescriptor),
    /// A static curve geometry descriptor.
    Curve(&'a metal4::AccelerationStructureCurveGeometryDescriptor),
    /// A motion curve geometry descriptor.
    MotionCurve(&'a metal4::AccelerationStructureMotionCurveGeometryDescriptor),
}

impl Metal4AccelerationStructureGeometry<'_> {
    fn as_inner(&self) -> &AnyObject {
        match *self {
            Self::Triangle(value) => value.as_inner(),
            Self::BoundingBox(value) => value.as_inner(),
            Self::MotionTriangle(value) => value.as_inner(),
            Self::MotionBoundingBox(value) => value.as_inner(),
            Self::Curve(value) => value.as_inner(),
            Self::MotionCurve(value) => value.as_inner(),
        }
    }
}

fn require_selector(object: &AnyObject, selector: Sel, operation: &str) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements respondsToSelector:, and Sel
    // and bool use the stable Objective-C runtime ABI.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if available {
        Ok(())
    } else {
        Err(Error::unsupported(format!(
            "{operation} is unavailable on this system"
        )))
    }
}

fn checked_raw_range(
    buffer: &Buffer,
    range: Range<usize>,
    operation: &str,
) -> Result<MTL4BufferRange, Error> {
    if range.start > range.end || range.end > buffer.length() {
        return Err(Error::invalid_argument(format!(
            "{operation} buffer range is out of bounds"
        )));
    }
    let length = range.end - range.start;
    if length == 0 {
        return Err(Error::invalid_argument(format!(
            "{operation} requires a non-empty buffer range"
        )));
    }
    require_selector(buffer.as_any_object(), sel!(gpuAddress), operation)?;
    // SAFETY: selector availability was checked and Metal declares gpuAddress
    // as an unsigned 64-bit value.
    let base = buffer.gpu_address();
    let offset = u64::try_from(range.start)
        .map_err(|_| Error::invalid_argument(format!("{operation} offset is too large")))?;
    let length = u64::try_from(length)
        .map_err(|_| Error::invalid_argument(format!("{operation} length is too large")))?;
    let address = base
        .checked_add(offset)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} address overflow")))?;
    Ok(MTL4BufferRange {
        bufferAddress: address,
        length,
    })
}

fn retain_buffer_for_property(descriptor: &AnyObject, key: &'static u8, buffer: Option<&Buffer>) {
    let value = buffer
        .map(|value| value.as_any_object() as *const AnyObject as *mut AnyObject)
        .unwrap_or(ptr::null_mut());
    // SAFETY: `descriptor` and an optional `buffer` are live Objective-C
    // objects.  The unique static key identifies one property, and RETAIN makes
    // the resource live for as long as the descriptor references its address.
    unsafe {
        objc_setAssociatedObject(
            descriptor as *const AnyObject as *mut AnyObject,
            key as *const u8 as *const _,
            value,
            OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        )
    }
}

macro_rules! range_property {
    ($type:ty, $get:ident, $set:ident, $getter:ident, $setter:ident, $key:ident) => {
        static $key: u8 = 0;

        impl $type {
            #[doc = concat!("Returns an opaque snapshot of `", stringify!($getter), "`.")]
            pub fn $get(&self) -> Result<AccelerationStructureBufferRange, Error> {
                require_selector(
                    self.as_inner(),
                    sel!($getter),
                    concat!("MTL4::", stringify!($type), "::", stringify!($getter)),
                )?;
                // SAFETY: selector availability is checked and this property
                // getter's return ABI is exactly MTL4BufferRange.
                let value: MTL4BufferRange = unsafe { msg_send![self.as_inner(), $getter] };
                Ok(AccelerationStructureBufferRange {
                    address: value.bufferAddress,
                    length: value.length,
                })
            }

            #[doc = concat!("Sets `", stringify!($setter), "` from a checked live buffer region, or clears it.")]
            pub fn $set(
                &self,
                value: Option<(&Buffer, Range<usize>)>,
            ) -> Result<(), Error> {
                let operation =
                    concat!("MTL4::", stringify!($type), "::", stringify!($setter));
                require_selector(
                    self.as_inner(),
                    sel!($setter:),
                    operation,
                )?;
                let (raw, buffer) = match value {
                    Some((buffer, range)) => {
                        (checked_raw_range(buffer, range, operation)?, Some(buffer))
                    }
                    None => (
                        MTL4BufferRange {
                            bufferAddress: 0,
                            length: 0,
                        },
                        None,
                    ),
                };
                // SAFETY: selector availability is checked and this property
                // setter's sole argument ABI is exactly MTL4BufferRange.
                unsafe {
                    let _: () = msg_send![self.as_inner(), $setter: raw];
                }
                retain_buffer_for_property(self.as_inner(), &$key, buffer);
                Ok(())
            }
        }
    };
}

range_property!(
    metal4::AccelerationStructureGeometryDescriptor,
    primitive_data_buffer,
    set_primitive_data_buffer,
    primitiveDataBuffer,
    setPrimitiveDataBuffer,
    PRIMITIVE_DATA_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureTriangleGeometryDescriptor,
    index_buffer,
    set_index_buffer,
    indexBuffer,
    setIndexBuffer,
    TRIANGLE_INDEX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureTriangleGeometryDescriptor,
    transformation_matrix_buffer,
    set_transformation_matrix_buffer,
    transformationMatrixBuffer,
    setTransformationMatrixBuffer,
    TRIANGLE_TRANSFORM_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureTriangleGeometryDescriptor,
    vertex_buffer,
    set_vertex_buffer,
    vertexBuffer,
    setVertexBuffer,
    TRIANGLE_VERTEX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureBoundingBoxGeometryDescriptor,
    bounding_box_buffer,
    set_bounding_box_buffer,
    boundingBoxBuffer,
    setBoundingBoxBuffer,
    BOUNDING_BOX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureMotionTriangleGeometryDescriptor,
    index_buffer,
    set_index_buffer,
    indexBuffer,
    setIndexBuffer,
    MOTION_TRIANGLE_INDEX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureMotionTriangleGeometryDescriptor,
    transformation_matrix_buffer,
    set_transformation_matrix_buffer,
    transformationMatrixBuffer,
    setTransformationMatrixBuffer,
    MOTION_TRIANGLE_TRANSFORM_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureMotionTriangleGeometryDescriptor,
    vertex_buffers,
    set_vertex_buffers,
    vertexBuffers,
    setVertexBuffers,
    MOTION_TRIANGLE_VERTEX_BUFFERS_KEY
);
range_property!(
    metal4::AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    bounding_box_buffers,
    set_bounding_box_buffers,
    boundingBoxBuffers,
    setBoundingBoxBuffers,
    MOTION_BOUNDING_BOX_BUFFERS_KEY
);
range_property!(
    metal4::AccelerationStructureCurveGeometryDescriptor,
    control_point_buffer,
    set_control_point_buffer,
    controlPointBuffer,
    setControlPointBuffer,
    CURVE_CONTROL_POINT_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureCurveGeometryDescriptor,
    index_buffer,
    set_index_buffer,
    indexBuffer,
    setIndexBuffer,
    CURVE_INDEX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureCurveGeometryDescriptor,
    radius_buffer,
    set_radius_buffer,
    radiusBuffer,
    setRadiusBuffer,
    CURVE_RADIUS_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureMotionCurveGeometryDescriptor,
    control_point_buffers,
    set_control_point_buffers,
    controlPointBuffers,
    setControlPointBuffers,
    MOTION_CURVE_CONTROL_POINT_BUFFERS_KEY
);
range_property!(
    metal4::AccelerationStructureMotionCurveGeometryDescriptor,
    index_buffer,
    set_index_buffer,
    indexBuffer,
    setIndexBuffer,
    MOTION_CURVE_INDEX_BUFFER_KEY
);
range_property!(
    metal4::AccelerationStructureMotionCurveGeometryDescriptor,
    radius_buffers,
    set_radius_buffers,
    radiusBuffers,
    setRadiusBuffers,
    MOTION_CURVE_RADIUS_BUFFERS_KEY
);
range_property!(
    metal4::InstanceAccelerationStructureDescriptor,
    instance_descriptor_buffer,
    set_instance_descriptor_buffer,
    instanceDescriptorBuffer,
    setInstanceDescriptorBuffer,
    INSTANCE_DESCRIPTOR_BUFFER_KEY
);
range_property!(
    metal4::InstanceAccelerationStructureDescriptor,
    motion_transform_buffer,
    set_motion_transform_buffer,
    motionTransformBuffer,
    setMotionTransformBuffer,
    INSTANCE_MOTION_TRANSFORM_BUFFER_KEY
);
range_property!(
    metal4::IndirectInstanceAccelerationStructureDescriptor,
    instance_count_buffer,
    set_instance_count_buffer,
    instanceCountBuffer,
    setInstanceCountBuffer,
    INDIRECT_INSTANCE_COUNT_BUFFER_KEY
);
range_property!(
    metal4::IndirectInstanceAccelerationStructureDescriptor,
    instance_descriptor_buffer,
    set_instance_descriptor_buffer,
    instanceDescriptorBuffer,
    setInstanceDescriptorBuffer,
    INDIRECT_INSTANCE_DESCRIPTOR_BUFFER_KEY
);
range_property!(
    metal4::IndirectInstanceAccelerationStructureDescriptor,
    motion_transform_buffer,
    set_motion_transform_buffer,
    motionTransformBuffer,
    setMotionTransformBuffer,
    INDIRECT_MOTION_TRANSFORM_BUFFER_KEY
);
range_property!(
    metal4::IndirectInstanceAccelerationStructureDescriptor,
    motion_transform_count_buffer,
    set_motion_transform_count_buffer,
    motionTransformCountBuffer,
    setMotionTransformCountBuffer,
    INDIRECT_MOTION_TRANSFORM_COUNT_BUFFER_KEY
);

impl metal4::PrimitiveAccelerationStructureDescriptor {
    /// Replaces the geometry list with an owned immutable Foundation array.
    pub fn set_geometry_descriptor_slice(
        &self,
        descriptors: &[Metal4AccelerationStructureGeometry<'_>],
    ) -> Result<(), Error> {
        if descriptors.is_empty() {
            return Err(Error::invalid_argument(
                "a primitive acceleration structure requires at least one geometry",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setGeometryDescriptors:),
            "MTL4::PrimitiveAccelerationStructureDescriptor::setGeometryDescriptors",
        )?;
        let class = AnyClass::get(c"NSMutableArray")
            .ok_or_else(|| Error::unsupported("NSMutableArray is unavailable"))?;
        // SAFETY: NSMutableArray implements new and returns a retained object.
        let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
        for descriptor in descriptors {
            // SAFETY: the array and descriptor are live, and addObject: retains
            // each non-null geometry descriptor.
            unsafe {
                let _: () = msg_send![&*array, addObject: descriptor.as_inner()];
            }
        }
        // SAFETY: selector availability was checked and the retained mutable
        // array is valid wherever Metal declares NSArray.
        unsafe {
            let _: () = msg_send![self.as_inner(), setGeometryDescriptors: &*array];
        }
        Ok(())
    }

    /// Returns owned base wrappers for all currently configured geometries.
    pub fn geometry_descriptor_vec(
        &self,
    ) -> Result<Vec<metal4::AccelerationStructureGeometryDescriptor>, Error> {
        require_selector(
            self.as_inner(),
            sel!(geometryDescriptors),
            "MTL4::PrimitiveAccelerationStructureDescriptor::geometryDescriptors",
        )?;
        // SAFETY: selector availability was checked and the property is NSArray.
        let array: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), geometryDescriptors] };
        let Some(array) = array else {
            return Ok(Vec::new());
        };
        // SAFETY: NSArray count has the stable NSUInteger return ABI.
        let count: usize = unsafe { msg_send![&*array, count] };
        let mut result = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: index is below count for the same immutable array and the
            // retained return is captured before wrapping it.
            let value: Retained<AnyObject> = unsafe { msg_send![&*array, objectAtIndex: index] };
            result.push(metal4::AccelerationStructureGeometryDescriptor::from_inner(
                value,
            ));
        }
        Ok(result)
    }
}
