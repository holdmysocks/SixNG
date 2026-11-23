//! Audited acceleration-structure encoding boundary.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    AccelerationStructure, AccelerationStructureBoundingBoxGeometryDescriptor,
    AccelerationStructureCommandEncoder as GeneratedEncoder,
    AccelerationStructureCurveGeometryDescriptor, AccelerationStructureDescriptor,
    AccelerationStructureGeometryDescriptor,
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    AccelerationStructureMotionCurveGeometryDescriptor,
    AccelerationStructureMotionTriangleGeometryDescriptor,
    AccelerationStructurePassSampleBufferAttachmentDescriptor,
    AccelerationStructurePassSampleBufferAttachmentDescriptorArray,
    AccelerationStructureTriangleGeometryDescriptor, CounterSampleBuffer, Fence, Heap,
    IndirectInstanceAccelerationStructureDescriptor, InstanceAccelerationStructureDescriptor,
    PrimitiveAccelerationStructureDescriptor,
};
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::generated_value_types::{
    AccelerationStructureRefitOptions, DataType, ResourceUsage,
};
use crate::metal::{Buffer, CommandBuffer, Texture};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, MessageReceiver, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{msg_send, sel};
use objc2_metal::{MTLAccelerationStructureCommandEncoder, MTLCommandBuffer as _, MTLResourceID};
use std::marker::PhantomData;

const SCRATCH_OFFSET_ALIGNMENT: usize = 256;

/// Conservatively supported acceleration-pass sample-buffer attachment slots.
pub const ACCELERATION_STRUCTURE_SAMPLE_ATTACHMENT_CAPACITY: usize = 4;

/// An acceleration-structure encoder that exclusively borrows its command buffer.
pub struct AccelerationStructureEncoder<'a> {
    inner: GeneratedEncoder,
    _command_buffer: PhantomData<&'a mut CommandBuffer>,
    ended: bool,
}

impl<'a> AccelerationStructureEncoder<'a> {
    pub(super) fn from_protocol(
        inner: Retained<ProtocolObject<dyn MTLAccelerationStructureCommandEncoder>>,
        _command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        // SAFETY: The protocol object is backed by the same Objective-C
        // allocation; GeneratedEncoder retains it as an opaque owned object.
        let inner = GeneratedEncoder::from_inner(unsafe { Retained::cast_unchecked(inner) });
        Self {
            inner,
            _command_buffer: PhantomData,
            ended: false,
        }
    }

    fn as_inner(&self) -> &AnyObject {
        self.inner.as_inner()
    }
}

impl CommandBuffer {
    /// Begins an acceleration-structure encoder while borrowing this command buffer exclusively.
    pub fn acceleration_structure_encoder(
        &mut self,
    ) -> Result<AccelerationStructureEncoder<'_>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(accelerationStructureCommandEncoder))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::accelerationStructureCommandEncoder is unavailable",
            ));
        }
        self.inner
            .accelerationStructureCommandEncoder()
            .map(|inner| AccelerationStructureEncoder::from_protocol(inner, self))
            .ok_or_else(|| {
                Error::unsupported("Metal could not create an acceleration-structure encoder")
            })
    }

    /// Begins a descriptor-configured acceleration-structure encoder with an exclusive borrow.
    pub fn acceleration_structure_encoder_with_descriptor(
        &mut self,
        descriptor: &crate::metal::generated_object_types::metal::AccelerationStructurePassDescriptor,
    ) -> Result<AccelerationStructureEncoder<'_>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(accelerationStructureCommandEncoderWithDescriptor:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::accelerationStructureCommandEncoderWithDescriptor is unavailable",
            ));
        }
        // SAFETY: Selector availability was checked; the descriptor wrapper
        // retains a valid pass descriptor and the returned protocol object is
        // tied to this command buffer's exclusive mutable borrow.
        let inner: Option<Retained<ProtocolObject<dyn MTLAccelerationStructureCommandEncoder>>> = unsafe {
            msg_send![
                &*self.inner,
                accelerationStructureCommandEncoderWithDescriptor: descriptor.as_inner()
            ]
        };
        inner
            .map(|inner| AccelerationStructureEncoder::from_protocol(inner, self))
            .ok_or_else(|| {
                Error::unsupported("Metal could not create an acceleration-structure encoder")
            })
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
    fn as_object(self) -> &'a AnyObject {
        match self {
            Self::Base(value) => value.as_inner(),
            Self::Primitive(value) => value.as_inner(),
            Self::Instance(value) => value.as_inner(),
            Self::IndirectInstance(value) => value.as_inner(),
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
    fn as_object(self) -> &'a AnyObject {
        match self {
            Self::Triangle(value) => value.as_inner(),
            Self::BoundingBox(value) => value.as_inner(),
            Self::Curve(value) => value.as_inner(),
            Self::MotionTriangle(value) => value.as_inner(),
            Self::MotionBoundingBox(value) => value.as_inner(),
            Self::MotionCurve(value) => value.as_inner(),
        }
    }
}

impl<'a> AccelerationStructureResource<'a> {
    fn as_object(self) -> &'a AnyObject {
        match self {
            Self::Buffer(value) => value.as_any_object(),
            Self::Texture(value) => value.as_any_object(),
            Self::AccelerationStructure(value) => value.as_inner(),
        }
    }
}

fn supports(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: Every Objective-C object implements respondsToSelector:, and
    // the selector and bool arguments use their declared runtime encodings.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

fn require_selector(object: &AnyObject, selector: Sel, operation: &str) -> Result<(), Error> {
    if supports(object, selector) {
        Ok(())
    } else {
        Err(Error::unsupported(format!(
            "MTL::AccelerationStructureCommandEncoder::{operation} is unavailable"
        )))
    }
}

fn read_object_array(
    owner: &AnyObject,
    getter: Sel,
    operation: &str,
) -> Result<Vec<Retained<AnyObject>>, Error> {
    require_selector(owner, getter, operation)?;
    // SAFETY: Selector availability was checked and this property is declared
    // to return an NSArray-compatible object.
    // SAFETY: Retaining immediately in the same call frame follows Cocoa's
    // autoreleased-return convention for Objective-C object properties.
    let array = unsafe {
        Retained::retain_autoreleased(owner.send_message::<_, *mut AnyObject>(getter, ()))
    };
    let Some(array) = array else {
        return Ok(Vec::new());
    };
    // SAFETY: NSArray-compatible objects implement count with NSUInteger ABI.
    let count: usize = unsafe { msg_send![&*array, count] };
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: index is strictly below count and NSArray retains each
        // returned object long enough for objc2 to acquire owned retention.
        let value: Retained<AnyObject> = unsafe { msg_send![&*array, objectAtIndex: index] };
        values.push(value);
    }
    Ok(values)
}

fn write_object_array(
    owner: &AnyObject,
    setter: Sel,
    operation: &str,
    values: &[&AnyObject],
) -> Result<(), Error> {
    require_selector(owner, setter, operation)?;
    let class = AnyClass::get(c"NSMutableArray")
        .ok_or_else(|| Error::unsupported("NSMutableArray is unavailable on this system"))?;
    // SAFETY: NSMutableArray implements +new with retained return semantics.
    let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
    for value in values {
        // SAFETY: NSMutableArray implements addObject: and each borrowed value
        // remains alive for the call; the array retains inserted objects.
        unsafe {
            let _: () = msg_send![&*array, addObject: *value];
        }
    }
    // SAFETY: Setter availability was checked and it accepts an NSArray
    // object with void return ABI.
    unsafe {
        let _: () = owner.send_message(setter, (&*array,));
    }
    Ok(())
}

fn checked_buffer_range(
    buffer: &Buffer,
    offset: usize,
    length: usize,
    operation: &str,
) -> Result<(), Error> {
    if length == 0 {
        return Err(Error::invalid_argument(format!(
            "{operation} requires a non-zero buffer range"
        )));
    }
    let end = offset.checked_add(length).ok_or_else(|| {
        Error::invalid_argument(format!("{operation} buffer range overflows usize"))
    })?;
    if end > buffer.length() {
        return Err(Error::invalid_argument(format!(
            "{operation} buffer range is out of bounds"
        )));
    }
    Ok(())
}

fn checked_attachment_index(index: usize) -> Result<(), Error> {
    if index >= ACCELERATION_STRUCTURE_SAMPLE_ATTACHMENT_CAPACITY {
        Err(Error::invalid_argument(
            "acceleration-pass sample attachment index must be below 4",
        ))
    } else {
        Ok(())
    }
}

fn validate_scratch(
    scratch: &Buffer,
    offset: usize,
    required_size: usize,
    operation: &str,
) -> Result<(), Error> {
    if !offset.is_multiple_of(SCRATCH_OFFSET_ALIGNMENT) {
        return Err(Error::invalid_argument(format!(
            "{operation} scratch offset must be 256-byte aligned"
        )));
    }
    checked_buffer_range(scratch, offset, required_size, operation)
}

impl AccelerationStructure {
    /// Returns this acceleration structure's GPU resource identifier when available.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        require_selector(self.as_inner(), sel!(gpuResourceID), "gpuResourceID")?;
        // SAFETY: Selector availability was checked and MTLResourceID has the
        // framework-declared one-u64 C representation.
        let raw: MTLResourceID = unsafe { msg_send![self.as_inner(), gpuResourceID] };
        // SAFETY: MTLResourceID is repr(C) with exactly one u64 field; this
        // copies its opaque handle bits without constructing an invalid value.
        let value = unsafe { std::mem::transmute::<MTLResourceID, u64>(raw) };
        Ok(ResourceID { _impl: value })
    }
}

impl AccelerationStructurePassSampleBufferAttachmentDescriptorArray {
    /// Returns an owned attachment descriptor from a checked fixed-capacity slot.
    pub fn attachment(
        &self,
        index: usize,
    ) -> Result<AccelerationStructurePassSampleBufferAttachmentDescriptor, Error> {
        checked_attachment_index(index)?;
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "objectAtIndexedSubscript",
        )?;
        // SAFETY: Selector availability and the framework's fixed four-slot
        // attachment capacity are checked before sending the index.
        let value: Retained<AnyObject> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(AccelerationStructurePassSampleBufferAttachmentDescriptor::from_inner(value))
    }

    /// Sets or clears an attachment descriptor in a checked fixed-capacity slot.
    pub fn set_attachment(
        &self,
        index: usize,
        attachment: Option<&AccelerationStructurePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        checked_attachment_index(index)?;
        if let Some(attachment) = attachment
            && let Some(sample_buffer) = attachment.sample_buffer()?
        {
            let sample_count = sample_buffer.sample_count()?;
            for sample_index in [
                attachment.start_of_encoder_sample_index()?,
                attachment.end_of_encoder_sample_index()?,
            ] {
                if sample_index != usize::MAX && sample_index >= sample_count {
                    return Err(Error::invalid_argument(
                        "acceleration-pass counter sample index is out of bounds",
                    ));
                }
            }
        }
        require_selector(
            self.as_inner(),
            sel!(setObject:atIndexedSubscript:),
            "setObjectAtIndexedSubscript",
        )?;
        // SAFETY: Selector availability and the fixed four-slot capacity are
        // checked; the optional owned wrapper remains alive for the call.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                setObject: attachment.map(
                    AccelerationStructurePassSampleBufferAttachmentDescriptor::as_inner
                ),
                atIndexedSubscript: index
            ];
        }
        Ok(())
    }
}

impl PrimitiveAccelerationStructureDescriptor {
    /// Returns owned base wrappers for all configured geometry descriptors.
    pub fn geometry_descriptors_vec(
        &self,
    ) -> Result<Vec<AccelerationStructureGeometryDescriptor>, Error> {
        read_object_array(
            self.as_inner(),
            sel!(geometryDescriptors),
            "geometryDescriptors",
        )
        .map(|values| {
            values
                .into_iter()
                .map(AccelerationStructureGeometryDescriptor::from_inner)
                .collect()
        })
    }

    /// Replaces geometry descriptors from a safe borrowed slice.
    pub fn set_geometry_descriptors_slice(
        &self,
        values: &[AccelerationStructureGeometry<'_>],
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .copied()
            .map(AccelerationStructureGeometry::as_object)
            .collect::<Vec<_>>();
        write_object_array(
            self.as_inner(),
            sel!(setGeometryDescriptors:),
            "setGeometryDescriptors",
            &values,
        )
    }
}

macro_rules! buffer_array_property {
    ($owner:ty, $getter_fn:ident, $setter_fn:ident, $getter:ident, $setter:ident) => {
        impl $owner {
            #[doc = concat!("Returns `", stringify!($getter), "` as owned safe buffers.")]
            pub fn $getter_fn(&self) -> Result<Vec<Buffer>, Error> {
                read_object_array(self.as_inner(), sel!($getter), stringify!($getter))?
                    .into_iter()
                    .map(Buffer::from_any_object)
                    .collect()
            }

            #[doc = concat!("Writes `", stringify!($setter), "` from a borrowed safe slice.")]
            pub fn $setter_fn(&self, values: &[&Buffer]) -> Result<(), Error> {
                let values = values
                    .iter()
                    .map(|value| value.as_any_object())
                    .collect::<Vec<_>>();
                write_object_array(
                    self.as_inner(),
                    sel!($setter:),
                    stringify!($setter),
                    &values,
                )
            }
        }
    };
}

buffer_array_property!(
    AccelerationStructureMotionTriangleGeometryDescriptor,
    vertex_buffers_vec,
    set_vertex_buffers_slice,
    vertexBuffers,
    setVertexBuffers
);
buffer_array_property!(
    AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    bounding_box_buffers_vec,
    set_bounding_box_buffers_slice,
    boundingBoxBuffers,
    setBoundingBoxBuffers
);
buffer_array_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    control_point_buffers_vec,
    set_control_point_buffers_slice,
    controlPointBuffers,
    setControlPointBuffers
);
buffer_array_property!(
    AccelerationStructureMotionCurveGeometryDescriptor,
    radius_buffers_vec,
    set_radius_buffers_slice,
    radiusBuffers,
    setRadiusBuffers
);

impl InstanceAccelerationStructureDescriptor {
    /// Returns the owned acceleration structures referenced by this instance descriptor.
    pub fn instanced_acceleration_structures_vec(
        &self,
    ) -> Result<Vec<AccelerationStructure>, Error> {
        read_object_array(
            self.as_inner(),
            sel!(instancedAccelerationStructures),
            "instancedAccelerationStructures",
        )
        .map(|values| {
            values
                .into_iter()
                .map(AccelerationStructure::from_inner)
                .collect()
        })
    }

    /// Replaces referenced acceleration structures from a safe borrowed slice.
    pub fn set_instanced_acceleration_structures_slice(
        &self,
        values: &[&AccelerationStructure],
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.as_inner())
            .collect::<Vec<_>>();
        write_object_array(
            self.as_inner(),
            sel!(setInstancedAccelerationStructures:),
            "setInstancedAccelerationStructures",
            &values,
        )
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
        let encoder = self.as_inner();
        require_selector(
            encoder,
            sel!(buildAccelerationStructure:descriptor:scratchBuffer:scratchBufferOffset:),
            "buildAccelerationStructure",
        )?;
        if required_acceleration_structure_size == 0
            || destination.size()? < required_acceleration_structure_size
        {
            return Err(Error::invalid_argument(
                "build destination is smaller than the required acceleration-structure size",
            ));
        }
        validate_scratch(
            scratch,
            scratch_offset,
            required_scratch_size,
            "acceleration-structure build",
        )?;
        // SAFETY: Selector availability was checked; all objects remain
        // retained by borrowed safe wrappers for this call, and destination
        // and scratch capacities plus scratch alignment were validated.
        unsafe {
            let _: () = msg_send![
                encoder,
                buildAccelerationStructure: destination.as_inner(),
                descriptor: descriptor.as_object(),
                scratchBuffer: scratch.as_any_object(),
                scratchBufferOffset: scratch_offset
            ];
        }
        Ok(())
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
        let encoder = self.as_inner();
        require_selector(
            encoder,
            sel!(refitAccelerationStructure:descriptor:destination:scratchBuffer:scratchBufferOffset:options:),
            "refitAccelerationStructure",
        )?;
        if !options.is_valid() {
            return Err(Error::invalid_argument(
                "refit options contain undeclared bits",
            ));
        }
        if required_destination_size == 0 {
            return Err(Error::invalid_argument(
                "refit requires a non-zero destination size",
            ));
        }
        let target = destination.unwrap_or(source);
        if target.size()? < required_destination_size {
            return Err(Error::invalid_argument(
                "refit destination is smaller than the required size",
            ));
        }
        validate_scratch(
            scratch,
            scratch_offset,
            required_scratch_size,
            "acceleration-structure refit",
        )?;
        // SAFETY: Selector availability, option bits, object lifetimes,
        // destination capacity, scratch capacity and alignment are checked.
        unsafe {
            let _: () = msg_send![
                encoder,
                refitAccelerationStructure: source.as_inner(),
                descriptor: descriptor.as_object(),
                destination: destination.map(AccelerationStructure::as_inner),
                scratchBuffer: scratch.as_any_object(),
                scratchBufferOffset: scratch_offset,
                options: options.as_raw()
            ];
        }
        Ok(())
    }

    /// Copies an acceleration structure after rejecting identity overlap and undersized destinations.
    pub fn copy(
        &self,
        source: &AccelerationStructure,
        destination: &AccelerationStructure,
    ) -> Result<(), Error> {
        let encoder = self.as_inner();
        require_selector(
            encoder,
            sel!(copyAccelerationStructure:toAccelerationStructure:),
            "copyAccelerationStructure",
        )?;
        if std::ptr::eq(source.as_inner(), destination.as_inner()) {
            return Err(Error::invalid_argument(
                "copy source and destination must be distinct",
            ));
        }
        if destination.size()? < source.size()? {
            return Err(Error::invalid_argument(
                "copy destination is smaller than the source",
            ));
        }
        // SAFETY: Selector availability and both object lifetimes are checked;
        // identity overlap is rejected and destination capacity is sufficient.
        unsafe {
            let _: () = msg_send![
                encoder,
                copyAccelerationStructure: source.as_inner(),
                toAccelerationStructure: destination.as_inner()
            ];
        }
        Ok(())
    }

    /// Writes the compacted size as a checked 32-bit value.
    pub fn write_compacted_size_u32(
        &self,
        source: &AccelerationStructure,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.write_compacted_size(source, destination, offset, 4, false)
    }

    /// Writes the compacted size as a checked 64-bit value when supported.
    pub fn write_compacted_size_u64(
        &self,
        source: &AccelerationStructure,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.write_compacted_size(source, destination, offset, 8, true)
    }

    fn write_compacted_size(
        &self,
        source: &AccelerationStructure,
        destination: &Buffer,
        offset: usize,
        width: usize,
        wide: bool,
    ) -> Result<(), Error> {
        if !offset.is_multiple_of(width) {
            return Err(Error::invalid_argument(
                "compacted-size destination offset is not naturally aligned",
            ));
        }
        checked_buffer_range(destination, offset, width, "compacted-size write")?;
        let encoder = self.as_inner();
        if wide {
            require_selector(
                encoder,
                sel!(writeCompactedAccelerationStructureSize:toBuffer:offset:sizeDataType:),
                "writeCompactedAccelerationStructureSize:sizeDataType",
            )?;
            let ulong_data_type = DataType::DataTypeULong.as_raw();
            // SAFETY: Selector availability, object lifetimes, natural
            // alignment and the complete 8-byte destination range are checked.
            unsafe {
                let _: () = msg_send![
                    encoder,
                    writeCompactedAccelerationStructureSize: source.as_inner(),
                    toBuffer: destination.as_any_object(),
                    offset: offset,
                    sizeDataType: ulong_data_type
                ];
            }
        } else {
            require_selector(
                encoder,
                sel!(writeCompactedAccelerationStructureSize:toBuffer:offset:),
                "writeCompactedAccelerationStructureSize",
            )?;
            // SAFETY: Selector availability, object lifetimes, natural
            // alignment and the complete 4-byte destination range are checked.
            unsafe {
                let _: () = msg_send![
                    encoder,
                    writeCompactedAccelerationStructureSize: source.as_inner(),
                    toBuffer: destination.as_any_object(),
                    offset: offset
                ];
            }
        }
        Ok(())
    }

    /// Copies and compacts an acceleration structure into a checked destination allocation.
    pub fn copy_and_compact(
        &self,
        source: &AccelerationStructure,
        destination: &AccelerationStructure,
        compacted_size: usize,
    ) -> Result<(), Error> {
        let encoder = self.as_inner();
        require_selector(
            encoder,
            sel!(copyAndCompactAccelerationStructure:toAccelerationStructure:),
            "copyAndCompactAccelerationStructure",
        )?;
        if std::ptr::eq(source.as_inner(), destination.as_inner()) {
            return Err(Error::invalid_argument(
                "compaction source and destination must be distinct",
            ));
        }
        if compacted_size == 0 || destination.size()? < compacted_size {
            return Err(Error::invalid_argument(
                "compaction destination is smaller than the compacted size",
            ));
        }
        // SAFETY: Selector availability and both object lifetimes are checked;
        // identity overlap is rejected and the declared compacted size fits.
        unsafe {
            let _: () = msg_send![
                encoder,
                copyAndCompactAccelerationStructure: source.as_inner(),
                toAccelerationStructure: destination.as_inner()
            ];
        }
        Ok(())
    }

    /// Updates a fence when the operation is available.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        let encoder = self.as_inner();
        require_selector(encoder, sel!(updateFence:), "updateFence")?;
        // SAFETY: The selector is present and the borrowed fence remains alive.
        unsafe {
            let _: () = msg_send![encoder, updateFence: fence.as_inner()];
        }
        Ok(())
    }

    /// Waits for a fence when the operation is available.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        let encoder = self.as_inner();
        require_selector(encoder, sel!(waitForFence:), "waitForFence")?;
        // SAFETY: The selector is present and the borrowed fence remains alive.
        unsafe {
            let _: () = msg_send![encoder, waitForFence: fence.as_inner()];
        }
        Ok(())
    }

    /// Declares one checked resource usage.
    pub fn use_resource(
        &self,
        resource: AccelerationStructureResource<'_>,
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        if !usage.is_valid() {
            return Err(Error::invalid_argument(
                "resource usage contains undeclared bits",
            ));
        }
        let encoder = self.as_inner();
        require_selector(encoder, sel!(useResource:usage:), "useResource")?;
        // SAFETY: The selector is present, usage bits are validated and the
        // borrowed resource remains alive for the message send.
        unsafe {
            let _: () =
                msg_send![encoder, useResource: resource.as_object(), usage: usage.as_raw()];
        }
        Ok(())
    }

    /// Declares multiple resource usages without exposing the native pointer-array ABI.
    pub fn use_resources(
        &self,
        resources: &[AccelerationStructureResource<'_>],
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        if resources.is_empty() {
            return Err(Error::invalid_argument(
                "resource usage requires at least one resource",
            ));
        }
        for resource in resources {
            self.use_resource(*resource, usage)?;
        }
        Ok(())
    }

    /// Declares a heap usage when available.
    pub fn use_heap(&self, heap: &Heap) -> Result<(), Error> {
        let encoder = self.as_inner();
        require_selector(encoder, sel!(useHeap:), "useHeap")?;
        // SAFETY: The selector is present and the borrowed heap remains alive.
        unsafe {
            let _: () = msg_send![encoder, useHeap: heap.as_inner()];
        }
        Ok(())
    }

    /// Declares multiple heap usages without exposing the native pointer-array ABI.
    pub fn use_heaps(&self, heaps: &[&Heap]) -> Result<(), Error> {
        if heaps.is_empty() {
            return Err(Error::invalid_argument(
                "heap usage requires at least one heap",
            ));
        }
        for heap in heaps {
            self.use_heap(heap)?;
        }
        Ok(())
    }

    /// Samples a counter buffer after validating the destination sample index.
    pub fn sample_counters(
        &self,
        sample_buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        if sample_index >= sample_buffer.sample_count()? {
            return Err(Error::invalid_argument(
                "counter sample index is out of bounds",
            ));
        }
        let encoder = self.as_inner();
        require_selector(
            encoder,
            sel!(sampleCountersInBuffer:atSampleIndex:withBarrier:),
            "sampleCountersInBuffer",
        )?;
        // SAFETY: Selector availability, sample-buffer lifetime and sample
        // index bounds are checked; bool/usize use the declared ABI.
        unsafe {
            let _: () = msg_send![
                encoder,
                sampleCountersInBuffer: sample_buffer.as_inner(),
                atSampleIndex: sample_index,
                withBarrier: barrier
            ];
        }
        Ok(())
    }

    /// Ends acceleration-structure encoding and releases the command-buffer borrow.
    pub fn end_encoding(mut self) {
        // SAFETY: MTLCommandEncoder guarantees endEncoding, and this owned
        // non-Clone wrapper calls it at most once.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), endEncoding];
        }
        self.ended = true;
    }
}

impl Drop for AccelerationStructureEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            // SAFETY: MTLCommandEncoder guarantees endEncoding, and `ended`
            // ensures this fallback runs only when explicit ending did not.
            unsafe {
                let _: () = msg_send![self.inner.as_inner(), endEncoding];
            }
            self.ended = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_indices_are_limited_to_proven_capacity() {
        assert!(checked_attachment_index(0).is_ok());
        assert!(checked_attachment_index(3).is_ok());
        assert!(checked_attachment_index(4).is_err());
        assert!(checked_attachment_index(usize::MAX).is_err());
    }
}
