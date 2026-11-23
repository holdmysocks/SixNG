//! Checked tensor layouts and descriptor construction.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_object_types::metal as object_types;
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::generated_value_types::{TensorDataType, TensorPlaneType, TensorUsage};
use crate::metal::{Buffer, ResourceOptions};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use std::ops::Range;

/// The largest rank accepted by Metal tensor extents.
pub const MAX_TENSOR_RANK: usize = 16;

/// One checked buffer attachment retained by a tensor attachment map.
#[derive(Clone)]
pub struct TensorBufferAttachment {
    buffer: Buffer,
    offset: usize,
    layout: TensorLayout,
}

impl TensorBufferAttachment {
    /// Returns the retained backing buffer.
    #[must_use]
    pub const fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Returns the checked byte offset into the buffer.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the layout whose complete span was checked against the buffer.
    #[must_use]
    pub const fn layout(&self) -> &TensorLayout {
        &self.layout
    }
}

/// A safe tensor buffer-attachment map keyed by declared plane types.
pub struct CheckedTensorBufferAttachments {
    inner: Retained<AnyObject>,
    entries: [Option<TensorBufferAttachment>; 2],
    device: Option<Retained<AnyObject>>,
    _thread_bound: ThreadBound,
}

impl CheckedTensorBufferAttachments {
    /// Creates an empty attachment map after checking framework availability.
    pub fn new() -> Result<Self, Error> {
        let class = AnyClass::get(c"MTLTensorBufferAttachments").ok_or_else(|| {
            Error::unsupported("MTLTensorBufferAttachments is unavailable on this system")
        })?;
        // SAFETY: Objective-C class objects implement respondsToSelector: with
        // the declared selector/bool ABI.
        let available: bool = unsafe { msg_send![class, respondsToSelector: sel!(new)] };
        if !available {
            return Err(Error::unsupported(
                "MTLTensorBufferAttachments constructor is unavailable",
            ));
        }
        // SAFETY: new follows retained-return conventions for the checked class.
        let inner: Retained<AnyObject> = unsafe { msg_send![class, new] };
        Ok(Self {
            inner,
            entries: std::array::from_fn(|_| None),
            device: None,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Associates a buffer range with a declared tensor plane.
    pub fn set_buffer(
        &mut self,
        plane: TensorPlaneType,
        buffer: &Buffer,
        offset: usize,
        layout: &TensorLayout,
    ) -> Result<(), Error> {
        let index = plane_index(plane)?;
        // Multi-plane tensors cannot use machine-learning usage, so the
        // compute context applies the common data-type alignment and span checks.
        layout.checked_buffer_range(buffer.length(), offset, TensorUsage::TensorUsageCompute)?;
        require_selector(buffer.as_any_object(), sel!(device), "MTLBuffer.device")?;
        // SAFETY: MTLResource.device is a non-null object property and selector
        // availability was checked immediately above.
        let device: Retained<AnyObject> = unsafe { msg_send![buffer.as_any_object(), device] };
        if self
            .device
            .as_ref()
            .is_some_and(|existing| !std::ptr::eq(&**existing, &*device))
        {
            return Err(Error::invalid_argument(
                "all tensor plane attachments must belong to the same device",
            ));
        }
        require_selector(
            &self.inner,
            sel!(setBuffer:offset:forPlane:),
            "MTLTensorBufferAttachments.setBuffer",
        )?;
        // SAFETY: selector availability is checked; the buffer wrapper retains
        // MTLBuffer identity, the plane is declared, and the full range is valid.
        unsafe {
            let _: () = msg_send![&*self.inner, setBuffer: buffer.as_any_object(), offset: offset, forPlane: plane.as_raw()];
        }
        self.entries[index] = Some(TensorBufferAttachment {
            buffer: buffer.clone(),
            offset,
            layout: layout.clone(),
        });
        self.device.get_or_insert(device);
        Ok(())
    }

    /// Returns the retained buffer for a plane, if it was configured.
    pub fn buffer(&self, plane: TensorPlaneType) -> Result<Option<Buffer>, Error> {
        Ok(self
            .entries
            .get(plane_index(plane)?)
            .and_then(Option::as_ref)
            .map(|attachment| attachment.buffer.clone()))
    }

    /// Returns the checked byte offset for a plane, if it was configured.
    pub fn offset(&self, plane: TensorPlaneType) -> Result<Option<usize>, Error> {
        Ok(self
            .entries
            .get(plane_index(plane)?)
            .and_then(Option::as_ref)
            .map(TensorBufferAttachment::offset))
    }

    /// Returns all checked attachment information for a plane.
    pub fn attachment(
        &self,
        plane: TensorPlaneType,
    ) -> Result<Option<&TensorBufferAttachment>, Error> {
        Ok(self
            .entries
            .get(plane_index(plane)?)
            .and_then(Option::as_ref))
    }

    /// Removes every native and Rust-side attachment.
    pub fn reset(&mut self) -> Result<(), Error> {
        require_selector(&self.inner, sel!(reset), "MTLTensorBufferAttachments.reset")?;
        // SAFETY: selector availability and zero-argument ABI are checked.
        unsafe {
            let _: () = msg_send![&*self.inner, reset];
        }
        self.entries.fill(None);
        self.device = None;
        Ok(())
    }

    pub(crate) fn as_inner(&self) -> &AnyObject {
        &self.inner
    }

    pub(crate) fn device(&self) -> Option<&AnyObject> {
        self.device.as_deref()
    }
}

fn plane_index(plane: TensorPlaneType) -> Result<usize, Error> {
    match plane.as_raw() {
        0 => Ok(0),
        1 => Ok(1),
        _ => Err(Error::invalid_argument("tensor plane type is undeclared")),
    }
}

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    context: &str,
) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements respondsToSelector: with a stable ABI.
    let supported: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if supported {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{context} is unavailable")))
    }
}

impl object_types::TensorAuxiliaryPlaneDescriptorMap {
    /// Associates a checked scale-plane descriptor with the map.
    pub fn set_descriptor(
        &self,
        plane: TensorPlaneType,
        descriptor: &object_types::TensorAuxiliaryPlaneDescriptor,
    ) -> Result<(), Error> {
        if plane != TensorPlaneType::TensorPlaneTypeScales {
            return Err(Error::invalid_argument(
                "only the scales plane accepts an auxiliary descriptor",
            ));
        }
        let data_type = descriptor.data_type()?;
        if data_type != TensorDataType::TensorDataTypeMetalFloat8UE8M0 {
            return Err(Error::invalid_argument(
                "the scales plane requires Float8UE8M0 data",
            ));
        }
        let factors = descriptor
            .block_factors()?
            .ok_or_else(|| Error::invalid_argument("auxiliary block factors are required"))?;
        let factors = read_extents(factors.as_inner(), "auxiliary block factors")?;
        if factors.as_slice().first() != Some(&32)
            || factors.as_slice().iter().skip(1).any(|&value| value != 1)
        {
            return Err(Error::invalid_argument(
                "auxiliary block factors must be [32, 1, ...]",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setDescriptor:forPlane:),
            "MTLTensorAuxiliaryPlaneDescriptorMap.setDescriptor",
        )?;
        // SAFETY: selector availability, descriptor identity, plane, data type,
        // and block-factor contract were checked above.
        unsafe {
            let _: () = msg_send![self.as_inner(), setDescriptor: descriptor.as_inner(), forPlane: plane.as_raw()];
        }
        Ok(())
    }

    /// Returns the retained descriptor for a declared auxiliary plane.
    pub fn descriptor(
        &self,
        plane: TensorPlaneType,
    ) -> Result<Option<object_types::TensorAuxiliaryPlaneDescriptor>, Error> {
        if plane != TensorPlaneType::TensorPlaneTypeScales {
            return Err(Error::invalid_argument(
                "only the scales plane has an auxiliary descriptor",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(descriptorForPlane:),
            "MTLTensorAuxiliaryPlaneDescriptorMap.descriptor",
        )?;
        // SAFETY: selector availability and plane value were checked; objc2
        // retains the nullable descriptor result.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), descriptorForPlane: plane.as_raw()] };
        Ok(value.map(object_types::TensorAuxiliaryPlaneDescriptor::from_inner))
    }

    /// Removes every auxiliary descriptor from the native map.
    pub fn reset(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTLTensorAuxiliaryPlaneDescriptorMap.reset",
        )?;
        // SAFETY: selector availability and its zero-argument ABI are checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

/// Owned, checked dimension or stride values for a Metal tensor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedTensorExtents {
    values: Box<[usize]>,
}

impl CheckedTensorExtents {
    /// Copies extent values after checking Metal's maximum rank and integer ABI.
    pub fn new(values: &[usize]) -> Result<Self, Error> {
        if values.len() > MAX_TENSOR_RANK {
            return Err(Error::invalid_argument(
                "tensor rank exceeds Metal's maximum of 16",
            ));
        }
        if values.iter().any(|&value| value > isize::MAX as usize) {
            return Err(Error::invalid_argument(
                "tensor extent cannot be represented by NSInteger",
            ));
        }
        Ok(Self {
            values: values.into(),
        })
    }

    /// Returns the number of dimensions.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.values.len()
    }

    /// Returns the extent at a checked dimension index.
    #[must_use]
    pub fn get(&self, dimension: usize) -> Option<usize> {
        self.values.get(dimension).copied()
    }

    /// Returns all extent values in innermost-to-outermost order.
    #[must_use]
    pub fn as_slice(&self) -> &[usize] {
        &self.values
    }
}

/// A tensor layout whose complete backing byte span has been checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorLayout {
    dimensions: CheckedTensorExtents,
    strides: CheckedTensorExtents,
    data_type: TensorDataType,
    byte_span: usize,
}

impl TensorLayout {
    /// Builds a dense tensor layout.
    pub fn dense(dimensions: &[usize], data_type: TensorDataType) -> Result<Self, Error> {
        let dimensions = validate_dimensions(dimensions, data_type)?;
        let mut strides = Vec::with_capacity(dimensions.rank());
        let mut stride = 1_usize;
        for &dimension in dimensions.as_slice() {
            strides.push(stride);
            stride = stride
                .checked_mul(dimension)
                .ok_or_else(|| Error::invalid_argument("tensor dense stride overflow"))?;
        }
        Self::new(dimensions.as_slice(), &strides, data_type, None)
    }

    /// Builds a tensor layout from explicit element strides.
    ///
    /// Strides must describe a non-overlapping, monotonically increasing
    /// layout. This safe substitute intentionally rejects aliased layouts.
    pub fn strided(
        dimensions: &[usize],
        strides: &[usize],
        data_type: TensorDataType,
    ) -> Result<Self, Error> {
        Self::new(dimensions, strides, data_type, None)
    }

    /// Builds a layout and applies the additional machine-learning stride rules.
    pub fn machine_learning(
        dimensions: &[usize],
        strides: &[usize],
        data_type: TensorDataType,
    ) -> Result<Self, Error> {
        Self::new(
            dimensions,
            strides,
            data_type,
            Some(TensorUsage::TensorUsageMachineLearning),
        )
    }

    fn new(
        dimensions: &[usize],
        strides: &[usize],
        data_type: TensorDataType,
        usage: Option<TensorUsage>,
    ) -> Result<Self, Error> {
        let dimensions = validate_dimensions(dimensions, data_type)?;
        let strides = CheckedTensorExtents::new(strides)?;
        if dimensions.rank() != strides.rank() {
            return Err(Error::invalid_argument(
                "tensor dimensions and strides must have the same rank",
            ));
        }
        validate_strides(&dimensions, &strides, data_type, usage)?;
        let byte_span = calculate_byte_span(&dimensions, &strides, data_type)?;
        Ok(Self {
            dimensions,
            strides,
            data_type,
            byte_span,
        })
    }

    /// Returns the dimensions in innermost-to-outermost order.
    #[must_use]
    pub const fn dimensions(&self) -> &CheckedTensorExtents {
        &self.dimensions
    }

    /// Returns element strides in innermost-to-outermost order.
    #[must_use]
    pub const fn strides(&self) -> &CheckedTensorExtents {
        &self.strides
    }

    /// Returns the tensor element type.
    #[must_use]
    pub const fn data_type(&self) -> TensorDataType {
        self.data_type
    }

    /// Returns the complete number of backing bytes reachable by this layout.
    #[must_use]
    pub const fn byte_span(&self) -> usize {
        self.byte_span
    }

    /// Checks the exact byte range used when this layout is backed by a buffer.
    pub fn checked_buffer_range(
        &self,
        buffer_length: usize,
        offset: usize,
        usage: TensorUsage,
    ) -> Result<Range<usize>, Error> {
        if !usage.is_valid() {
            return Err(Error::invalid_argument(
                "tensor usage contains unknown bits",
            ));
        }
        if usage.as_raw() & TensorUsage::TensorUsageMachineLearning.as_raw() != 0 && offset != 0 {
            return Err(Error::invalid_argument(
                "machine-learning tensors backed by buffers require offset zero",
            ));
        }
        let alignment = if is_format_type(self.data_type) {
            128
        } else {
            element_bits(self.data_type)? / 8
        };
        if !offset.is_multiple_of(alignment) {
            return Err(Error::invalid_argument(
                "tensor buffer offset is not aligned for its data type",
            ));
        }
        let end = offset
            .checked_add(self.byte_span)
            .ok_or_else(|| Error::invalid_argument("tensor buffer range overflow"))?;
        if end > buffer_length {
            return Err(Error::invalid_argument(
                "tensor layout exceeds the backing buffer",
            ));
        }
        Ok(offset..end)
    }
}

/// An audited Objective-C tensor descriptor paired with its checked layout.
#[derive(Clone)]
pub struct CheckedTensorDescriptor {
    inner: Retained<AnyObject>,
    layout: TensorLayout,
    usage: TensorUsage,
    resource_options: ResourceOptions,
    _thread_bound: ThreadBound,
}

impl CheckedTensorDescriptor {
    /// Creates a Metal tensor descriptor after validating every layout input.
    pub fn new(
        layout: TensorLayout,
        usage: TensorUsage,
        resource_options: ResourceOptions,
    ) -> Result<Self, Error> {
        if !usage.is_valid() || usage.as_raw() == 0 {
            return Err(Error::invalid_argument(
                "tensor usage must contain at least one declared context",
            ));
        }
        if !resource_options.is_valid() {
            return Err(Error::invalid_argument(
                "tensor resource options contain unknown bits",
            ));
        }
        if usage.as_raw() & TensorUsage::TensorUsageMachineLearning.as_raw() != 0 {
            validate_strides(
                layout.dimensions(),
                layout.strides(),
                layout.data_type(),
                Some(usage),
            )?;
        }

        let dimensions = ObjectiveCTensorExtents::new(layout.dimensions())?;
        let strides = ObjectiveCTensorExtents::new(layout.strides())?;
        let class = AnyClass::get(c"MTLTensorDescriptor").ok_or_else(|| {
            Error::unsupported("MTLTensorDescriptor is unavailable on this system")
        })?;
        // SAFETY: Objective-C class objects implement respondsToSelector: with
        // the declared selector/bool ABI.
        let can_create: bool = unsafe { msg_send![class, respondsToSelector: sel!(new)] };
        if !can_create {
            return Err(Error::unsupported(
                "MTLTensorDescriptor constructor is unavailable",
            ));
        }
        // SAFETY: `new` is a retained-return Objective-C convention and the
        // runtime class identity was checked immediately above.
        let inner: Retained<AnyObject> = unsafe { msg_send![class, new] };
        for (selector, name) in [
            (sel!(setDimensions:), "setDimensions:"),
            (sel!(setStrides:), "setStrides:"),
            (sel!(setDataType:), "setDataType:"),
            (sel!(setUsage:), "setUsage:"),
            (sel!(setResourceOptions:), "setResourceOptions:"),
        ] {
            // SAFETY: NSObject implements respondsToSelector: with the declared ABI.
            let available: bool = unsafe { msg_send![&*inner, respondsToSelector: selector] };
            if !available {
                return Err(Error::unsupported(format!(
                    "MTLTensorDescriptor selector {name} is unavailable"
                )));
            }
        }
        // SAFETY: all selectors were checked, object arguments have the exact
        // MTLTensorExtents identity, and scalar values were validated above.
        unsafe {
            let _: () = msg_send![&*inner, setDimensions: &*dimensions.inner];
            let _: () = msg_send![&*inner, setStrides: &*strides.inner];
            let _: () = msg_send![&*inner, setDataType: layout.data_type().as_raw()];
            let _: () = msg_send![&*inner, setUsage: usage.as_raw()];
            let _: () = msg_send![&*inner, setResourceOptions: resource_options.as_raw()];
        }
        Ok(Self {
            inner,
            layout,
            usage,
            resource_options,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Returns the checked layout used to create this descriptor.
    #[must_use]
    pub const fn layout(&self) -> &TensorLayout {
        &self.layout
    }

    /// Returns the validated tensor usage bits.
    #[must_use]
    pub const fn usage(&self) -> TensorUsage {
        self.usage
    }

    /// Returns the validated resource allocation options.
    #[must_use]
    pub const fn resource_options(&self) -> ResourceOptions {
        self.resource_options
    }

    /// Checks the exact backing range using this descriptor's own usage bits.
    pub fn checked_buffer_range(
        &self,
        buffer_length: usize,
        offset: usize,
    ) -> Result<Range<usize>, Error> {
        self.layout
            .checked_buffer_range(buffer_length, offset, self.usage)
    }

    pub(crate) fn as_inner(&self) -> &AnyObject {
        &self.inner
    }
}

pub(crate) struct ObjectiveCTensorExtents {
    pub(crate) inner: Retained<AnyObject>,
}

impl ObjectiveCTensorExtents {
    pub(crate) fn new(extents: &CheckedTensorExtents) -> Result<Self, Error> {
        let class = AnyClass::get(c"MTLTensorExtents")
            .ok_or_else(|| Error::unsupported("MTLTensorExtents is unavailable on this system"))?;
        let values: Vec<isize> = extents
            .as_slice()
            .iter()
            .map(|&value| value as isize)
            .collect();
        // SAFETY: Objective-C class objects implement instancesRespondToSelector:
        // with the declared selector/bool ABI.
        let available: bool =
            unsafe { msg_send![class, instancesRespondToSelector: sel!(initWithRank:values:)] };
        if !available {
            return Err(Error::unsupported(
                "MTLTensorExtents initializer is unavailable",
            ));
        }
        // SAFETY: `alloc` returns a retained uninitialized instance of the
        // checked MTLTensorExtents class.
        let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        let pointer = if values.is_empty() {
            std::ptr::null()
        } else {
            values.as_ptr()
        };
        // SAFETY: the pointer covers exactly rank NSInteger values for the
        // duration of the call; Metal copies them during initialization.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![allocated, initWithRank: values.len(), values: pointer] };
        inner
            .map(|inner| Self { inner })
            .ok_or_else(|| Error::invalid_argument("Metal rejected tensor extents"))
    }

    pub(crate) fn from_values(values: &[usize]) -> Result<Self, Error> {
        Self::new(&CheckedTensorExtents::new(values)?)
    }
}

pub(crate) fn read_extents(
    object: &AnyObject,
    context: &str,
) -> Result<CheckedTensorExtents, Error> {
    require_selector(object, sel!(rank), &format!("{context}.rank"))?;
    require_selector(
        object,
        sel!(extentAtDimensionIndex:),
        &format!("{context}.extentAtDimensionIndex"),
    )?;
    // SAFETY: selector availability is checked and rank returns NSUInteger.
    let rank: usize = unsafe { msg_send![object, rank] };
    if rank > MAX_TENSOR_RANK {
        return Err(Error::unsupported("Metal returned tensor rank above 16"));
    }
    let mut values = Vec::with_capacity(rank);
    for index in 0..rank {
        // SAFETY: index is below rank and the selector returns NSInteger.
        let value: isize = unsafe { msg_send![object, extentAtDimensionIndex: index] };
        if value <= 0 {
            return Err(Error::unsupported(
                "Metal returned a non-positive concrete tensor extent",
            ));
        }
        values.push(value as usize);
    }
    CheckedTensorExtents::new(&values)
}

impl object_types::Tensor {
    /// Returns the opaque GPU resource identifier as an owned Rust value.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        require_selector(
            self.as_inner(),
            sel!(gpuResourceID),
            "MTLTensor.gpuResourceID",
        )?;
        // SAFETY: selector availability is checked and MTLResourceID is the
        // SDK-declared one-u64 value returned by this property.
        let raw: objc2_metal::MTLResourceID = unsafe { msg_send![self.as_inner(), gpuResourceID] };
        // SAFETY: MTLResourceID is repr(C) and contains exactly one u64 field.
        let raw = unsafe { std::ptr::read_unaligned(std::ptr::from_ref(&raw).cast::<u64>()) };
        Ok(ResourceID { _impl: raw })
    }

    /// Returns all auxiliary planes as retained, typed wrappers.
    pub fn auxiliary_plane_objects(
        &self,
    ) -> Result<Vec<object_types::TensorAuxiliaryPlane>, Error> {
        require_selector(
            self.as_inner(),
            sel!(auxiliaryPlanes),
            "MTLTensor.auxiliaryPlanes",
        )?;
        // SAFETY: the property is declared to return a non-null NSArray.
        let array: Retained<AnyObject> = unsafe { msg_send![self.as_inner(), auxiliaryPlanes] };
        require_selector(&array, sel!(count), "tensor auxiliary plane count")?;
        require_selector(
            &array,
            sel!(objectAtIndex:),
            "tensor auxiliary plane indexing",
        )?;
        // SAFETY: NSArray count has the standard NSUInteger ABI.
        let count: usize = unsafe { msg_send![&*array, count] };
        (0..count)
            .map(|index| {
                // SAFETY: index is below count and objc2 retains the result.
                let value: Retained<AnyObject> =
                    unsafe { msg_send![&*array, objectAtIndex: index] };
                Ok(object_types::TensorAuxiliaryPlane::from_inner(value))
            })
            .collect()
    }

    /// Copies a checked data-plane slice into an owned byte vector.
    pub fn read_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
    ) -> Result<Vec<u8>, Error> {
        self.read_slice_for_plane(origin, dimensions, memory_layout, None)
    }

    /// Copies a checked plane slice into an owned byte vector.
    pub fn read_plane_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        plane: TensorPlaneType,
    ) -> Result<Vec<u8>, Error> {
        plane_index(plane)?;
        self.read_slice_for_plane(origin, dimensions, memory_layout, Some(plane))
    }

    fn read_slice_for_plane(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        plane: Option<TensorPlaneType>,
    ) -> Result<Vec<u8>, Error> {
        validate_tensor_slice(self, origin, dimensions, memory_layout, plane)?;
        let origin = ObjectiveCTensorExtents::new(&CheckedTensorExtents::new(origin)?)?;
        let dimensions = ObjectiveCTensorExtents::new(&CheckedTensorExtents::new(dimensions)?)?;
        let strides = ObjectiveCTensorExtents::new(memory_layout.strides())?;
        let mut bytes = vec![0_u8; memory_layout.byte_span()];
        if let Some(plane) = plane {
            require_selector(
                self.as_inner(),
                sel!(getBytes:strides:fromSliceOrigin:sliceDimensions:plane:),
                "MTLTensor.getBytes plane overload",
            )?;
            // SAFETY: the output vector covers the proven layout span and all
            // checked extent objects remain live for this synchronous call.
            unsafe {
                let _: () = msg_send![self.as_inner(), getBytes: bytes.as_mut_ptr(), strides: &*strides.inner, fromSliceOrigin: &*origin.inner, sliceDimensions: &*dimensions.inner, plane: plane.as_raw()];
            }
        } else {
            require_selector(
                self.as_inner(),
                sel!(getBytes:strides:fromSliceOrigin:sliceDimensions:),
                "MTLTensor.getBytes",
            )?;
            // SAFETY: the output vector covers the proven layout span and all
            // checked extent objects remain live for this synchronous call.
            unsafe {
                let _: () = msg_send![self.as_inner(), getBytes: bytes.as_mut_ptr(), strides: &*strides.inner, fromSliceOrigin: &*origin.inner, sliceDimensions: &*dimensions.inner];
            }
        }
        Ok(bytes)
    }

    /// Replaces a checked data-plane slice from a Rust byte slice.
    pub fn write_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.write_slice_for_plane(origin, dimensions, memory_layout, bytes, None)
    }

    /// Replaces a checked plane slice from a Rust byte slice.
    pub fn write_plane_slice(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        plane: TensorPlaneType,
        bytes: &[u8],
    ) -> Result<(), Error> {
        plane_index(plane)?;
        self.write_slice_for_plane(origin, dimensions, memory_layout, bytes, Some(plane))
    }

    fn write_slice_for_plane(
        &self,
        origin: &[usize],
        dimensions: &[usize],
        memory_layout: &TensorLayout,
        bytes: &[u8],
        plane: Option<TensorPlaneType>,
    ) -> Result<(), Error> {
        validate_tensor_slice(self, origin, dimensions, memory_layout, plane)?;
        if bytes.len() != memory_layout.byte_span() {
            return Err(Error::invalid_argument(
                "tensor slice byte length must equal its checked layout span",
            ));
        }
        let origin = ObjectiveCTensorExtents::new(&CheckedTensorExtents::new(origin)?)?;
        let dimensions = ObjectiveCTensorExtents::new(&CheckedTensorExtents::new(dimensions)?)?;
        let strides = ObjectiveCTensorExtents::new(memory_layout.strides())?;
        if let Some(plane) = plane {
            require_selector(
                self.as_inner(),
                sel!(replaceSliceOrigin:sliceDimensions:plane:withBytes:strides:),
                "MTLTensor.replaceSliceOrigin plane overload",
            )?;
            // SAFETY: the byte slice exactly covers the proven layout span and
            // all checked extent objects remain live for this synchronous call.
            unsafe {
                let _: () = msg_send![self.as_inner(), replaceSliceOrigin: &*origin.inner, sliceDimensions: &*dimensions.inner, plane: plane.as_raw(), withBytes: bytes.as_ptr(), strides: &*strides.inner];
            }
        } else {
            require_selector(
                self.as_inner(),
                sel!(replaceSliceOrigin:sliceDimensions:withBytes:strides:),
                "MTLTensor.replaceSliceOrigin",
            )?;
            // SAFETY: the byte slice exactly covers the proven layout span and
            // all checked extent objects remain live for this synchronous call.
            unsafe {
                let _: () = msg_send![self.as_inner(), replaceSliceOrigin: &*origin.inner, sliceDimensions: &*dimensions.inner, withBytes: bytes.as_ptr(), strides: &*strides.inner];
            }
        }
        Ok(())
    }
}

fn validate_tensor_slice(
    tensor: &object_types::Tensor,
    origin: &[usize],
    dimensions: &[usize],
    memory_layout: &TensorLayout,
    plane: Option<TensorPlaneType>,
) -> Result<(), Error> {
    require_selector(
        tensor.as_inner(),
        sel!(storageMode),
        "MTLTensor.storageMode",
    )?;
    // SAFETY: selector availability is checked and storageMode returns NSUInteger.
    let storage_mode: usize = unsafe { msg_send![tensor.as_inner(), storageMode] };
    if storage_mode != 0 {
        return Err(Error::unsupported(
            "tensor CPU slice access requires shared storage",
        ));
    }
    let data_dimensions = tensor
        .dimensions()?
        .ok_or_else(|| Error::unsupported("tensor dimensions are unavailable"))?;
    let data_dimensions = read_extents(data_dimensions.as_inner(), "tensor dimensions")?;
    let (limits, data_type) = if plane == Some(TensorPlaneType::TensorPlaneTypeData) {
        (data_dimensions.as_slice().to_vec(), tensor.data_type()?)
    } else if let Some(plane) = plane {
        let selected = tensor
            .auxiliary_plane_objects()?
            .into_iter()
            .find(|candidate| candidate.plane_type().is_ok_and(|value| value == plane))
            .ok_or_else(|| Error::invalid_argument("tensor does not contain this plane"))?;
        let factors = selected
            .block_factors()?
            .ok_or_else(|| Error::unsupported("tensor plane block factors are unavailable"))?;
        let factors = read_extents(factors.as_inner(), "tensor plane block factors")?;
        if factors.rank() != data_dimensions.rank() {
            return Err(Error::unsupported(
                "tensor plane block-factor rank is inconsistent",
            ));
        }
        if data_dimensions
            .as_slice()
            .iter()
            .zip(factors.as_slice())
            .any(|(&extent, &factor)| !extent.is_multiple_of(factor))
        {
            return Err(Error::unsupported(
                "tensor dimensions are not divisible by plane block factors",
            ));
        }
        let limits = data_dimensions
            .as_slice()
            .iter()
            .zip(factors.as_slice())
            .map(|(&extent, &factor)| extent / factor)
            .collect::<Vec<_>>();
        (limits, selected.data_type()?)
    } else {
        (data_dimensions.as_slice().to_vec(), tensor.data_type()?)
    };
    if data_type != memory_layout.data_type()
        || origin.len() != limits.len()
        || dimensions.len() != limits.len()
        || memory_layout.dimensions().as_slice() != dimensions
        || dimensions.contains(&0)
    {
        return Err(Error::invalid_argument(
            "tensor slice type, rank, dimensions, or memory layout is incompatible",
        ));
    }
    for ((&start, &length), &limit) in origin.iter().zip(dimensions).zip(&limits) {
        if start.checked_add(length).is_none_or(|end| end > limit) {
            return Err(Error::invalid_argument("tensor slice is out of bounds"));
        }
    }
    let bits = element_bits(data_type)?;
    if origin.first().is_some_and(|value| {
        value
            .checked_mul(bits)
            .is_none_or(|bits| !bits.is_multiple_of(8))
    }) || dimensions.first().is_some_and(|value| {
        value
            .checked_mul(bits)
            .is_none_or(|bits| !bits.is_multiple_of(8))
    }) {
        return Err(Error::invalid_argument(
            "tensor innermost slice origin and dimension must be byte aligned",
        ));
    }
    Ok(())
}

fn validate_dimensions(
    dimensions: &[usize],
    data_type: TensorDataType,
) -> Result<CheckedTensorExtents, Error> {
    element_bits(data_type)?;
    let dimensions = CheckedTensorExtents::new(dimensions)?;
    if dimensions.as_slice().contains(&0) {
        return Err(Error::invalid_argument(
            "tensor dimensions must be greater than zero",
        ));
    }
    if is_format_type(data_type) {
        let first = dimensions.get(0).ok_or_else(|| {
            Error::invalid_argument("format tensors must have rank one or higher")
        })?;
        if first % 32 != 0 {
            return Err(Error::invalid_argument(
                "format tensor innermost dimension must be a multiple of 32",
            ));
        }
    }
    Ok(dimensions)
}

fn validate_strides(
    dimensions: &CheckedTensorExtents,
    strides: &CheckedTensorExtents,
    data_type: TensorDataType,
    usage: Option<TensorUsage>,
) -> Result<(), Error> {
    if dimensions.rank() != strides.rank() {
        return Err(Error::invalid_argument(
            "tensor dimensions and strides must have the same rank",
        ));
    }
    if let Some(&first) = strides.as_slice().first()
        && first != 1
    {
        return Err(Error::invalid_argument("tensor first stride must be one"));
    }
    for index in 1..strides.rank() {
        let minimum = strides.as_slice()[index - 1]
            .checked_mul(dimensions.as_slice()[index - 1])
            .ok_or_else(|| Error::invalid_argument("tensor stride overflow"))?;
        if strides.as_slice()[index] < minimum {
            return Err(Error::invalid_argument(
                "tensor strides must describe a non-overlapping layout",
            ));
        }
    }
    let bits = element_bits(data_type)?;
    if is_format_type(data_type) {
        for &stride in strides.as_slice().iter().skip(1) {
            let stride_bits = stride
                .checked_mul(bits)
                .ok_or_else(|| Error::invalid_argument("tensor stride byte offset overflow"))?;
            if stride_bits % (128 * 8) != 0 {
                return Err(Error::invalid_argument(
                    "format tensor outer strides must be 128-byte aligned",
                ));
            }
        }
    }
    if usage
        .is_some_and(|value| value.as_raw() & TensorUsage::TensorUsageMachineLearning.as_raw() != 0)
        && strides.rank() > 1
    {
        let second_bits = strides.as_slice()[1]
            .checked_mul(bits)
            .ok_or_else(|| Error::invalid_argument("tensor stride byte offset overflow"))?;
        if second_bits % (64 * 8) != 0 {
            return Err(Error::invalid_argument(
                "machine-learning tensor second stride must be 64-byte aligned",
            ));
        }
        for index in 2..strides.rank() {
            let required = strides.as_slice()[index - 1]
                .checked_mul(dimensions.as_slice()[index - 1])
                .ok_or_else(|| Error::invalid_argument("tensor stride overflow"))?;
            if strides.as_slice()[index] != required {
                return Err(Error::invalid_argument(
                    "machine-learning tensor outer strides must be contiguous",
                ));
            }
        }
    }
    Ok(())
}

fn calculate_byte_span(
    dimensions: &CheckedTensorExtents,
    strides: &CheckedTensorExtents,
    data_type: TensorDataType,
) -> Result<usize, Error> {
    let mut last_element = 0_usize;
    for (&dimension, &stride) in dimensions.as_slice().iter().zip(strides.as_slice()) {
        let contribution = (dimension - 1)
            .checked_mul(stride)
            .ok_or_else(|| Error::invalid_argument("tensor element span overflow"))?;
        last_element = last_element
            .checked_add(contribution)
            .ok_or_else(|| Error::invalid_argument("tensor element span overflow"))?;
    }
    let elements = last_element
        .checked_add(1)
        .ok_or_else(|| Error::invalid_argument("tensor element span overflow"))?;
    let bits = elements
        .checked_mul(element_bits(data_type)?)
        .ok_or_else(|| Error::invalid_argument("tensor byte span overflow"))?;
    bits.checked_add(7)
        .map(|rounded| rounded / 8)
        .ok_or_else(|| Error::invalid_argument("tensor byte span overflow"))
}

fn element_bits(data_type: TensorDataType) -> Result<usize, Error> {
    match data_type.as_raw() {
        3 | 29 | 33 => Ok(32),
        16 | 37 | 41 | 121 => Ok(16),
        45 | 49 | 141 | 142 | 145 => Ok(8),
        143 | 144 | 148 => Ok(4),
        149 | 150 => Ok(2),
        _ => Err(Error::invalid_argument(
            "tensor data type is invalid or has unknown element size",
        )),
    }
}

const fn is_format_type(data_type: TensorDataType) -> bool {
    matches!(
        data_type.as_raw(),
        141 | 142 | 143 | 144 | 145 | 148 | 149 | 150
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_layout_calculates_span() {
        let layout =
            TensorLayout::dense(&[3, 4, 5], TensorDataType::TensorDataTypeFloat32).unwrap();
        assert_eq!(layout.strides().as_slice(), &[1, 3, 12]);
        assert_eq!(layout.byte_span(), 3 * 4 * 5 * 4);
    }

    #[test]
    fn strided_layout_includes_padding() {
        let layout =
            TensorLayout::strided(&[3, 2], &[1, 8], TensorDataType::TensorDataTypeUInt16).unwrap();
        assert_eq!(layout.byte_span(), 22);
    }

    #[test]
    fn format_layout_uses_bit_packing() {
        let layout = TensorLayout::dense(&[32], TensorDataType::TensorDataTypeUInt2).unwrap();
        assert_eq!(layout.byte_span(), 8);
        assert!(
            layout
                .checked_buffer_range(136, 128, TensorUsage::TensorUsageCompute)
                .is_ok()
        );
        assert!(
            layout
                .checked_buffer_range(136, 64, TensorUsage::TensorUsageCompute)
                .is_err()
        );
    }

    #[test]
    fn invalid_rank_dimensions_and_overlap_are_rejected() {
        assert!(CheckedTensorExtents::new(&[1; 17]).is_err());
        assert!(TensorLayout::dense(&[0], TensorDataType::TensorDataTypeFloat32).is_err());
        assert!(
            TensorLayout::strided(&[4, 2], &[1, 3], TensorDataType::TensorDataTypeFloat32,)
                .is_err()
        );
    }

    #[test]
    fn machine_learning_alignment_and_contiguity_are_checked() {
        assert!(
            TensorLayout::machine_learning(
                &[16, 2, 3],
                &[1, 16, 32],
                TensorDataType::TensorDataTypeFloat32,
            )
            .is_ok()
        );
        assert!(TensorLayout::machine_learning(
            &[8, 2],
            &[1, 8],
            TensorDataType::TensorDataTypeFloat32,
        )
        .is_err());
    }

    #[test]
    fn buffer_range_checks_overflow_bounds_and_ml_offset() {
        let layout = TensorLayout::dense(&[4], TensorDataType::TensorDataTypeFloat32).unwrap();
        assert_eq!(
            layout
                .checked_buffer_range(32, 16, TensorUsage::TensorUsageCompute)
                .unwrap(),
            16..32
        );
        assert!(
            layout
                .checked_buffer_range(31, 16, TensorUsage::TensorUsageCompute)
                .is_err()
        );
        assert!(
            layout
                .checked_buffer_range(32, 16, TensorUsage::TensorUsageMachineLearning)
                .is_err()
        );
        assert!(
            layout
                .checked_buffer_range(usize::MAX, usize::MAX - 3, TensorUsage::TensorUsageCompute)
                .is_err()
        );
    }

    #[test]
    fn attachment_planes_have_fixed_checked_slots() {
        assert_eq!(
            plane_index(TensorPlaneType::TensorPlaneTypeData).unwrap(),
            0
        );
        assert_eq!(
            plane_index(TensorPlaneType::TensorPlaneTypeScales).unwrap(),
            1
        );
        assert!(TensorPlaneType::try_from(2).is_err());
    }
}
