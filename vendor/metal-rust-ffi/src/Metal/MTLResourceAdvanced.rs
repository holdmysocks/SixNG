//! Audited heap, residency-set, and rasterization-rate operations.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    AccelerationStructure, AccelerationStructureDescriptor, Allocation, Heap,
    RasterizationRateLayerArray, RasterizationRateLayerDescriptor, RasterizationRateMap,
    RasterizationRateMapDescriptor, RasterizationRateSampleArray, ResidencySet,
};
use crate::metal::generated_struct_types::{SamplePosition, SizeAndAlign};
use crate::metal::generated_value_types::PurgeableState;
use crate::metal::{Buffer, ResourceOptions, Size, StorageMode, Texture, TextureDescriptor};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2::{AnyThread, msg_send, sel};
use objc2_foundation::{NSArray, NSNumber};
use objc2_metal::{
    MTLCoordinate2D, MTLRasterizationRateLayerDescriptor,
    MTLRasterizationRateMapDescriptor as NativeRasterizationRateMapDescriptor, MTLSize,
    MTLSizeAndAlign,
};

fn responds_to(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: every Objective-C object implements respondsToSelector:, and
    // Sel/bool use the runtime's declared ABI encodings.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

fn class_responds_to(class: &AnyClass, selector: Sel) -> bool {
    // SAFETY: Objective-C class objects implement respondsToSelector:, and
    // Sel/bool use the runtime's declared ABI encodings.
    unsafe { msg_send![class, respondsToSelector: selector] }
}

fn class_instances_respond_to(class: &AnyClass, selector: Sel) -> bool {
    // SAFETY: Objective-C class objects implement instancesRespondToSelector:,
    // and Sel/bool use the runtime's declared ABI encodings.
    unsafe { msg_send![class, instancesRespondToSelector: selector] }
}

fn require_selector(object: &AnyObject, selector: Sel, message: &'static str) -> Result<(), Error> {
    if responds_to(object, selector) {
        Ok(())
    } else {
        Err(Error::unsupported(message))
    }
}

fn checked_size(size: Size, subject: &'static str) -> Result<MTLSize, Error> {
    if size.width == 0 || size.height == 0 {
        return Err(Error::invalid_argument(format!(
            "{subject} width and height must be non-zero"
        )));
    }
    Ok(MTLSize {
        width: size.width,
        height: size.height,
        depth: size.depth,
    })
}

fn from_mtl_size(value: MTLSize) -> Size {
    Size::new(value.width, value.height, value.depth)
}

fn checked_layer_index(layer_count: usize, layer_index: usize) -> Result<(), Error> {
    if layer_index >= layer_count {
        Err(Error::invalid_argument(
            "rasterization-rate layer index is out of bounds",
        ))
    } else {
        Ok(())
    }
}

fn validate_rate(value: f32) -> Result<(), Error> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(Error::invalid_argument(
            "rasterization-rate samples must be finite values in 0.0..=1.0",
        ))
    }
}

impl RasterizationRateSampleArray {
    fn sample(&self, index: usize) -> Result<f32, Error> {
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "rasterization-rate sample access is unavailable",
        )?;
        // SAFETY: callers validate the index against the owning layer's sample
        // count, and the selector returns an owned NSNumber.
        let value: Retained<NSNumber> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(value.as_f32())
    }

    fn set_sample(&self, index: usize, value: f32) -> Result<(), Error> {
        validate_rate(value)?;
        require_selector(
            self.as_inner(),
            sel!(setObject:atIndexedSubscript:),
            "rasterization-rate sample mutation is unavailable",
        )?;
        let value = NSNumber::new_f32(value);
        // SAFETY: callers validate the index against the owning layer's sample
        // count, and the NSNumber remains alive for the synchronous message.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                setObject: &*value,
                atIndexedSubscript: index
            ];
        }
        Ok(())
    }
}

impl RasterizationRateLayerDescriptor {
    /// Creates a layer descriptor with checked horizontal and vertical sample counts.
    pub fn with_sample_count(sample_count: Size) -> Result<Self, Error> {
        let sample_count = checked_size(sample_count, "sample count")?;
        let class = AnyClass::get(c"MTLRasterizationRateLayerDescriptor").ok_or_else(|| {
            Error::unsupported("MTLRasterizationRateLayerDescriptor is unavailable")
        })?;
        if !class_responds_to(class, sel!(alloc)) {
            return Err(Error::unsupported(
                "rasterization-rate layer allocation is unavailable",
            ));
        }
        if !class_instances_respond_to(class, sel!(initWithSampleCount:)) {
            return Err(Error::unsupported(
                "rasterization-rate layer initialization is unavailable",
            ));
        }
        let allocated = MTLRasterizationRateLayerDescriptor::alloc();
        // SAFETY: width and height are non-zero, the concrete class exists,
        // and Metal initializes and owns its copied sample storage.
        let inner = unsafe {
            MTLRasterizationRateLayerDescriptor::initWithSampleCount(allocated, sample_count)
        };
        // SAFETY: the concrete descriptor and AnyObject are the same Objective-C
        // allocation; only the static Rust view changes.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Self::from_inner(inner))
    }

    /// Creates a layer descriptor from owned Rust sample slices.
    pub fn with_samples(horizontal: &[f32], vertical: &[f32]) -> Result<Self, Error> {
        if horizontal.is_empty() || vertical.is_empty() {
            return Err(Error::invalid_argument(
                "horizontal and vertical samples must be non-empty",
            ));
        }
        for &sample in horizontal.iter().chain(vertical) {
            validate_rate(sample)?;
        }
        let descriptor = Self::with_sample_count(Size::new(horizontal.len(), vertical.len(), 0))?;
        for (index, &sample) in horizontal.iter().enumerate() {
            descriptor.set_horizontal_sample(index, sample)?;
        }
        for (index, &sample) in vertical.iter().enumerate() {
            descriptor.set_vertical_sample(index, sample)?;
        }
        Ok(descriptor)
    }

    /// Returns the active horizontal and vertical sample counts.
    pub fn sample_count(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(sampleCount),
            "rasterization-rate sampleCount is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSize by value.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), sampleCount]
        }))
    }

    /// Returns the maximum horizontal and vertical sample counts.
    pub fn max_sample_count(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(maxSampleCount),
            "rasterization-rate maxSampleCount is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSize by value.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), maxSampleCount]
        }))
    }

    /// Changes the active sample counts within the descriptor's allocation.
    pub fn set_sample_count(&self, sample_count: Size) -> Result<(), Error> {
        let value = checked_size(sample_count, "sample count")?;
        let maximum = self.max_sample_count()?;
        if sample_count.width > maximum.width || sample_count.height > maximum.height {
            return Err(Error::invalid_argument(
                "sample count exceeds the layer descriptor's maximum",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setSampleCount:),
            "rasterization-rate sampleCount mutation is unavailable",
        )?;
        // SAFETY: both dimensions are non-zero and no larger than maxSampleCount.
        unsafe {
            let _: () = msg_send![self.as_inner(), setSampleCount: value];
        }
        Ok(())
    }

    /// Copies horizontal samples into an owned Rust vector.
    pub fn horizontal_samples(&self) -> Result<Vec<f32>, Error> {
        let count = self.sample_count()?.width;
        let samples = self.horizontal()?.ok_or_else(|| {
            Error::unsupported("Metal returned no horizontal rasterization-rate samples")
        })?;
        (0..count).map(|index| samples.sample(index)).collect()
    }

    /// Copies vertical samples into an owned Rust vector.
    pub fn vertical_samples(&self) -> Result<Vec<f32>, Error> {
        let count = self.sample_count()?.height;
        let samples = self.vertical()?.ok_or_else(|| {
            Error::unsupported("Metal returned no vertical rasterization-rate samples")
        })?;
        (0..count).map(|index| samples.sample(index)).collect()
    }

    /// Sets one checked horizontal sample.
    pub fn set_horizontal_sample(&self, index: usize, value: f32) -> Result<(), Error> {
        if index >= self.sample_count()?.width {
            return Err(Error::invalid_argument(
                "horizontal sample index is out of bounds",
            ));
        }
        self.horizontal()?
            .ok_or_else(|| Error::unsupported("horizontal sample access is unavailable"))?
            .set_sample(index, value)
    }

    /// Sets one checked vertical sample.
    pub fn set_vertical_sample(&self, index: usize, value: f32) -> Result<(), Error> {
        if index >= self.sample_count()?.height {
            return Err(Error::invalid_argument(
                "vertical sample index is out of bounds",
            ));
        }
        self.vertical()?
            .ok_or_else(|| Error::unsupported("vertical sample access is unavailable"))?
            .set_sample(index, value)
    }
}

impl RasterizationRateLayerArray {
    fn layer(&self, index: usize) -> Result<Option<RasterizationRateLayerDescriptor>, Error> {
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "rasterization-rate layer array access is unavailable",
        )?;
        // SAFETY: the descriptor owner validates index against layerCount.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(value.map(RasterizationRateLayerDescriptor::from_inner))
    }
}

impl RasterizationRateMapDescriptor {
    /// Creates a checked descriptor with no layers.
    pub fn with_screen_size(screen_size: Size) -> Result<Self, Error> {
        let screen_size = checked_size(screen_size, "screen size")?;
        let class = AnyClass::get(c"MTLRasterizationRateMapDescriptor").ok_or_else(|| {
            Error::unsupported("MTLRasterizationRateMapDescriptor is unavailable")
        })?;
        if !class_responds_to(class, sel!(rasterizationRateMapDescriptorWithScreenSize:)) {
            return Err(Error::unsupported(
                "rasterization-rate map descriptor factory is unavailable",
            ));
        }
        // SAFETY: the class and selector are present and dimensions are non-zero.
        let inner = unsafe {
            NativeRasterizationRateMapDescriptor::rasterizationRateMapDescriptorWithScreenSize(
                screen_size,
            )
        };
        // SAFETY: the concrete descriptor and AnyObject are the same Objective-C
        // allocation; only the static Rust view changes.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Self::from_inner(inner))
    }

    /// Creates a descriptor with one layer.
    pub fn with_layer(
        screen_size: Size,
        layer: &RasterizationRateLayerDescriptor,
    ) -> Result<Self, Error> {
        Self::with_layers(screen_size, &[layer])
    }

    /// Creates a descriptor from a checked Rust slice of layers.
    pub fn with_layers(
        screen_size: Size,
        layers: &[&RasterizationRateLayerDescriptor],
    ) -> Result<Self, Error> {
        let descriptor = Self::with_screen_size(screen_size)?;
        for (index, layer) in layers.iter().enumerate() {
            descriptor.set_layer(index, Some(layer))?;
        }
        Ok(descriptor)
    }

    /// Returns the descriptor's screen-space size.
    pub fn screen_size(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(screenSize),
            "rasterization-rate screenSize is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSize by value.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), screenSize]
        }))
    }

    /// Sets a non-zero screen-space size.
    pub fn set_screen_size(&self, screen_size: Size) -> Result<(), Error> {
        let value = checked_size(screen_size, "screen size")?;
        require_selector(
            self.as_inner(),
            sel!(setScreenSize:),
            "rasterization-rate screenSize mutation is unavailable",
        )?;
        // SAFETY: the selector is present and dimensions are non-zero.
        unsafe {
            let _: () = msg_send![self.as_inner(), setScreenSize: value];
        }
        Ok(())
    }

    /// Returns one checked layer.
    pub fn layer(&self, index: usize) -> Result<Option<RasterizationRateLayerDescriptor>, Error> {
        checked_layer_index(self.layer_count()?, index)?;
        require_selector(
            self.as_inner(),
            sel!(layerAtIndex:),
            "rasterization-rate layer access is unavailable",
        )?;
        // SAFETY: index is strictly below layerCount and the selector is present.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), layerAtIndex: index] };
        Ok(value.map(RasterizationRateLayerDescriptor::from_inner))
    }

    /// Returns all contiguous layers as owned wrappers.
    pub fn layer_vec(&self) -> Result<Vec<RasterizationRateLayerDescriptor>, Error> {
        let count = self.layer_count()?;
        let layers = self
            .layers()?
            .ok_or_else(|| Error::unsupported("rasterization-rate layer array is unavailable"))?;
        (0..count)
            .map(|index| {
                layers.layer(index)?.ok_or_else(|| {
                    Error::unsupported("Metal returned a gap in rasterization-rate layers")
                })
            })
            .collect()
    }

    /// Replaces, appends, or removes one layer without exposing array pointers.
    pub fn set_layer(
        &self,
        index: usize,
        layer: Option<&RasterizationRateLayerDescriptor>,
    ) -> Result<(), Error> {
        let count = self.layer_count()?;
        if index > count || (layer.is_none() && index == count) {
            return Err(Error::invalid_argument(
                "rasterization-rate layer mutation index is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setLayer:atIndex:),
            "rasterization-rate layer mutation is unavailable",
        )?;
        // SAFETY: replacement/removal indices are below layerCount and an
        // append is allowed exactly at layerCount; borrowed objects stay live.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                setLayer: layer.map(RasterizationRateLayerDescriptor::as_inner),
                atIndex: index
            ];
        }
        Ok(())
    }
}

impl RasterizationRateMap {
    /// Returns the map's screen-space size.
    pub fn screen_size(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(screenSize),
            "rasterization-rate map screenSize is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSize by value.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), screenSize]
        }))
    }

    /// Returns the physical rasterization granularity.
    pub fn physical_granularity(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(physicalGranularity),
            "rasterization-rate physicalGranularity is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSize by value.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), physicalGranularity]
        }))
    }

    /// Returns the physical size for one checked layer.
    pub fn physical_size(&self, layer_index: usize) -> Result<Size, Error> {
        checked_layer_index(self.layer_count()?, layer_index)?;
        require_selector(
            self.as_inner(),
            sel!(physicalSizeForLayer:),
            "rasterization-rate physicalSize is unavailable",
        )?;
        // SAFETY: layer_index is strictly below layerCount.
        Ok(from_mtl_size(unsafe {
            msg_send![self.as_inner(), physicalSizeForLayer: layer_index]
        }))
    }

    /// Returns the parameter-buffer size and alignment requirements.
    pub fn parameter_buffer_size_and_align(&self) -> Result<SizeAndAlign, Error> {
        require_selector(
            self.as_inner(),
            sel!(parameterBufferSizeAndAlign),
            "rasterization-rate parameter buffer requirements are unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLSizeAndAlign by value.
        let value: MTLSizeAndAlign =
            unsafe { msg_send![self.as_inner(), parameterBufferSizeAndAlign] };
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Copies parameter data into a checked shared buffer range.
    pub fn copy_parameter_data_to_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        if buffer.storage_mode() != StorageMode::Shared {
            return Err(Error::invalid_argument(
                "rasterization-rate parameter buffers must use shared storage",
            ));
        }
        let requirements = self.parameter_buffer_size_and_align()?;
        if requirements.align == 0 || !offset.is_multiple_of(requirements.align) {
            return Err(Error::invalid_argument(
                "rasterization-rate parameter buffer offset is misaligned",
            ));
        }
        let end = offset.checked_add(requirements.size).ok_or_else(|| {
            Error::invalid_argument("rasterization-rate parameter buffer range overflow")
        })?;
        if end > buffer.length() {
            return Err(Error::invalid_argument(
                "rasterization-rate parameter buffer range is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(copyParameterDataToBuffer:offset:),
            "rasterization-rate parameter copy is unavailable",
        )?;
        // SAFETY: the shared buffer range is in bounds and aligned to the
        // exact requirements returned by this map; the borrow covers the call.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                copyParameterDataToBuffer: buffer.as_any_object(),
                offset: offset
            ];
        }
        Ok(())
    }

    /// Maps a finite screen-space coordinate to physical fragment space.
    pub fn map_screen_to_physical_coordinates(
        &self,
        coordinate: SamplePosition,
        layer_index: usize,
    ) -> Result<SamplePosition, Error> {
        self.map_coordinate(
            coordinate,
            layer_index,
            sel!(mapScreenToPhysicalCoordinates:forLayer:),
            "screen-to-physical coordinate mapping is unavailable",
        )
    }

    /// Maps a finite physical-fragment coordinate to screen space.
    pub fn map_physical_to_screen_coordinates(
        &self,
        coordinate: SamplePosition,
        layer_index: usize,
    ) -> Result<SamplePosition, Error> {
        self.map_coordinate(
            coordinate,
            layer_index,
            sel!(mapPhysicalToScreenCoordinates:forLayer:),
            "physical-to-screen coordinate mapping is unavailable",
        )
    }

    fn map_coordinate(
        &self,
        coordinate: SamplePosition,
        layer_index: usize,
        selector: Sel,
        unavailable: &'static str,
    ) -> Result<SamplePosition, Error> {
        if !coordinate.x.is_finite() || !coordinate.y.is_finite() {
            return Err(Error::invalid_argument(
                "rasterization-rate coordinates must be finite",
            ));
        }
        checked_layer_index(self.layer_count()?, layer_index)?;
        require_selector(self.as_inner(), selector, unavailable)?;
        let coordinate = MTLCoordinate2D {
            x: coordinate.x,
            y: coordinate.y,
        };
        // SAFETY: the selector is checked, coordinate is finite, and the layer
        // index is strictly below layerCount. Both selectors share this ABI.
        let mapped: MTLCoordinate2D = unsafe {
            if selector == sel!(mapScreenToPhysicalCoordinates:forLayer:) {
                msg_send![
                    self.as_inner(),
                    mapScreenToPhysicalCoordinates: coordinate,
                    forLayer: layer_index
                ]
            } else {
                msg_send![
                    self.as_inner(),
                    mapPhysicalToScreenCoordinates: coordinate,
                    forLayer: layer_index
                ]
            }
        };
        Ok(SamplePosition {
            x: mapped.x,
            y: mapped.y,
        })
    }
}

impl Heap {
    /// Returns the maximum allocatable block for a zero or power-of-two alignment.
    pub fn max_available_size(&self, alignment: usize) -> Result<usize, Error> {
        if alignment != 0 && !alignment.is_power_of_two() {
            return Err(Error::invalid_argument(
                "heap allocation alignment must be zero or a power of two",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(maxAvailableSizeWithAlignment:),
            "heap maximum available size query is unavailable",
        )?;
        // SAFETY: alignment is zero or a power of two as required by Metal.
        Ok(unsafe { msg_send![self.as_inner(), maxAvailableSizeWithAlignment: alignment] })
    }

    /// Creates a non-empty buffer using compatible heap resource options.
    pub fn new_buffer(&self, length: usize, options: ResourceOptions) -> Result<Buffer, Error> {
        if length == 0 || !options.is_valid() {
            return Err(Error::invalid_argument(
                "heap buffer length must be non-zero and options valid",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(newBufferWithLength:options:),
            "heap buffer allocation is unavailable",
        )?;
        // SAFETY: selector and value representations are checked; the result
        // follows new-family retained ownership.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newBufferWithLength: length,
                options: options.as_raw()
            ]
        };
        value
            .map(Buffer::from_any_object)
            .transpose()?
            .ok_or_else(|| Error::unsupported("Metal could not allocate the heap buffer"))
    }

    /// Creates a placement buffer after validating size, alignment, and heap range.
    pub fn new_buffer_at_offset(
        &self,
        length: usize,
        options: ResourceOptions,
        offset: usize,
    ) -> Result<Buffer, Error> {
        let device = self
            .device()?
            .ok_or_else(|| Error::unsupported("heap device is unavailable"))?;
        let requirements = device.heap_buffer_size_and_align(length, options)?;
        self.validate_placement(offset, &requirements)?;
        require_selector(
            self.as_inner(),
            sel!(newBufferWithLength:options:offset:),
            "placement heap buffer allocation is unavailable",
        )?;
        // SAFETY: offset alignment and the complete required allocation range
        // were checked against this heap.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newBufferWithLength: length,
                options: options.as_raw(),
                offset: offset
            ]
        };
        value
            .map(Buffer::from_any_object)
            .transpose()?
            .ok_or_else(|| Error::unsupported("Metal rejected the placement heap buffer"))
    }

    /// Creates a texture from a validated descriptor.
    pub fn new_texture(&self, descriptor: &TextureDescriptor) -> Result<Texture, Error> {
        require_selector(
            self.as_inner(),
            sel!(newTextureWithDescriptor:),
            "heap texture allocation is unavailable",
        )?;
        // SAFETY: descriptor is the canonical validated descriptor and the
        // returned object follows new-family retained ownership.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newTextureWithDescriptor: descriptor.as_any_object()
            ]
        };
        value
            .map(Texture::from_any_object)
            .transpose()?
            .ok_or_else(|| Error::unsupported("Metal could not allocate the heap texture"))
    }

    /// Creates a placement texture after validating its required heap range.
    pub fn new_texture_at_offset(
        &self,
        descriptor: &TextureDescriptor,
        offset: usize,
    ) -> Result<Texture, Error> {
        let device = self
            .device()?
            .ok_or_else(|| Error::unsupported("heap device is unavailable"))?;
        let requirements = device.heap_texture_size_and_align(descriptor)?;
        self.validate_placement(offset, &requirements)?;
        require_selector(
            self.as_inner(),
            sel!(newTextureWithDescriptor:offset:),
            "placement heap texture allocation is unavailable",
        )?;
        // SAFETY: the canonical descriptor is live and offset alignment plus
        // the complete required allocation range were checked.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newTextureWithDescriptor: descriptor.as_any_object(),
                offset: offset
            ]
        };
        value
            .map(Texture::from_any_object)
            .transpose()?
            .ok_or_else(|| Error::unsupported("Metal rejected the placement heap texture"))
    }

    /// Creates an acceleration-structure allocation with a checked non-zero size.
    pub fn new_acceleration_structure(&self, size: usize) -> Result<AccelerationStructure, Error> {
        if size == 0 {
            return Err(Error::invalid_argument(
                "acceleration structure size must be non-zero",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(newAccelerationStructureWithSize:),
            "heap acceleration-structure allocation is unavailable",
        )?;
        // SAFETY: size is non-zero and the result uses new-family ownership.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), newAccelerationStructureWithSize: size] };
        value.map(AccelerationStructure::from_inner).ok_or_else(|| {
            Error::unsupported("Metal could not allocate the acceleration structure")
        })
    }

    /// Creates a placement acceleration structure after range and alignment checks.
    pub fn new_acceleration_structure_at_offset(
        &self,
        size: usize,
        offset: usize,
    ) -> Result<AccelerationStructure, Error> {
        let device = self
            .device()?
            .ok_or_else(|| Error::unsupported("heap device is unavailable"))?;
        let requirements = device.heap_acceleration_structure_size_and_align(size)?;
        self.validate_placement(offset, &requirements)?;
        require_selector(
            self.as_inner(),
            sel!(newAccelerationStructureWithSize:offset:),
            "placement heap acceleration-structure allocation is unavailable",
        )?;
        // SAFETY: offset alignment and complete allocation range were checked.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newAccelerationStructureWithSize: size,
                offset: offset
            ]
        };
        value.map(AccelerationStructure::from_inner).ok_or_else(|| {
            Error::unsupported("Metal rejected the placement acceleration structure")
        })
    }

    /// Creates an acceleration structure whose size is inferred from a descriptor.
    pub fn new_acceleration_structure_with_descriptor(
        &self,
        descriptor: &AccelerationStructureDescriptor,
    ) -> Result<AccelerationStructure, Error> {
        require_selector(
            self.as_inner(),
            sel!(newAccelerationStructureWithDescriptor:),
            "descriptor-based heap acceleration-structure allocation is unavailable",
        )?;
        // SAFETY: the descriptor wrapper owns a live Metal descriptor and the
        // result follows new-family retained ownership.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newAccelerationStructureWithDescriptor: descriptor.as_inner()
            ]
        };
        value.map(AccelerationStructure::from_inner).ok_or_else(|| {
            Error::unsupported(
                "Metal could not allocate the descriptor-based acceleration structure",
            )
        })
    }

    /// Creates a descriptor-based placement acceleration structure.
    pub fn new_acceleration_structure_with_descriptor_at_offset(
        &self,
        descriptor: &AccelerationStructureDescriptor,
        offset: usize,
    ) -> Result<AccelerationStructure, Error> {
        let device = self
            .device()?
            .ok_or_else(|| Error::unsupported("heap device is unavailable"))?;
        require_selector(
            device.as_any_object(),
            sel!(heapAccelerationStructureSizeAndAlignWithDescriptor:),
            "descriptor-based acceleration-structure size query is unavailable",
        )?;
        // SAFETY: selector is present and descriptor remains live for the call.
        let value: MTLSizeAndAlign = unsafe {
            msg_send![
                device.as_any_object(),
                heapAccelerationStructureSizeAndAlignWithDescriptor: descriptor.as_inner()
            ]
        };
        let requirements = SizeAndAlign {
            size: value.size,
            align: value.align,
        };
        self.validate_placement(offset, &requirements)?;
        require_selector(
            self.as_inner(),
            sel!(newAccelerationStructureWithDescriptor:offset:),
            "descriptor-based placement acceleration-structure allocation is unavailable",
        )?;
        // SAFETY: the descriptor is live and the placement range satisfies the
        // exact size and alignment returned by this heap's device.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_inner(),
                newAccelerationStructureWithDescriptor: descriptor.as_inner(),
                offset: offset
            ]
        };
        value.map(AccelerationStructure::from_inner).ok_or_else(|| {
            Error::unsupported(
                "Metal rejected the descriptor-based placement acceleration structure",
            )
        })
    }

    /// Changes purgeability after validating the requested enumeration value.
    pub fn set_purgeable_state(&self, state: PurgeableState) -> Result<PurgeableState, Error> {
        if !state.is_valid() {
            return Err(Error::invalid_argument("invalid heap purgeable state"));
        }
        require_selector(
            self.as_inner(),
            sel!(setPurgeableState:),
            "heap purgeable-state mutation is unavailable",
        )?;
        // SAFETY: selector is present and the input has a declared raw value.
        let raw: usize = unsafe { msg_send![self.as_inner(), setPurgeableState: state.as_raw()] };
        PurgeableState::try_from(raw)
            .map_err(|()| Error::unsupported("Metal returned an unknown purgeable state"))
    }

    fn validate_placement(&self, offset: usize, requirements: &SizeAndAlign) -> Result<(), Error> {
        if requirements.align == 0 || !offset.is_multiple_of(requirements.align) {
            return Err(Error::invalid_argument(
                "placement heap offset does not satisfy the required alignment",
            ));
        }
        let end = offset
            .checked_add(requirements.size)
            .ok_or_else(|| Error::invalid_argument("placement heap range overflow"))?;
        if end > self.size()? {
            return Err(Error::invalid_argument(
                "placement heap allocation range exceeds heap size",
            ));
        }
        Ok(())
    }
}

/// A borrowed allocation accepted by a residency set without exposing protocol objects.
#[derive(Clone, Copy)]
pub enum ResidencyAllocation<'a> {
    /// A Metal buffer allocation.
    Buffer(&'a Buffer),
    /// A Metal texture allocation.
    Texture(&'a Texture),
    /// A Metal heap allocation.
    Heap(&'a Heap),
    /// A Metal acceleration-structure allocation.
    AccelerationStructure(&'a AccelerationStructure),
    /// Another safe generated allocation wrapper.
    Other(&'a Allocation),
}

impl<'a> ResidencyAllocation<'a> {
    fn as_inner(&self) -> &'a AnyObject {
        match *self {
            Self::Buffer(value) => value.as_any_object(),
            Self::Texture(value) => value.as_any_object(),
            Self::Heap(value) => value.as_inner(),
            Self::AccelerationStructure(value) => value.as_inner(),
            Self::Other(value) => value.as_inner(),
        }
    }
}

impl ResidencySet {
    /// Adds one allocation to the set's pending changes.
    pub fn add_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<(), Error> {
        self.mutate_allocation(sel!(addAllocation:), allocation, true)
    }

    /// Adds allocations from a Rust slice without exposing pointer arrays.
    pub fn add_allocations(&self, allocations: &[ResidencyAllocation<'_>]) -> Result<(), Error> {
        for &allocation in allocations {
            self.add_allocation(allocation)?;
        }
        Ok(())
    }

    /// Marks one allocation for removal.
    pub fn remove_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<(), Error> {
        self.mutate_allocation(sel!(removeAllocation:), allocation, false)
    }

    /// Removes allocations from a Rust slice without exposing pointer arrays.
    pub fn remove_allocations(&self, allocations: &[ResidencyAllocation<'_>]) -> Result<(), Error> {
        for &allocation in allocations {
            self.remove_allocation(allocation)?;
        }
        Ok(())
    }

    /// Returns whether a checked allocation is present, including pending changes.
    pub fn contains_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<bool, Error> {
        require_selector(
            self.as_inner(),
            sel!(containsAllocation:),
            "residency-set allocation lookup is unavailable",
        )?;
        // SAFETY: the borrowed allocation remains live during the synchronous call.
        Ok(unsafe {
            msg_send![
                self.as_inner(),
                containsAllocation: allocation.as_inner()
            ]
        })
    }

    /// Copies all allocations into owned safe wrappers.
    pub fn allocation_vec(&self) -> Result<Vec<Allocation>, Error> {
        require_selector(
            self.as_inner(),
            sel!(allAllocations),
            "residency-set allocation enumeration is unavailable",
        )?;
        // SAFETY: the selector is present and returns an owned NSArray.
        let values: Retained<NSArray<AnyObject>> =
            unsafe { msg_send![self.as_inner(), allAllocations] };
        let count = values.len();
        let mut result = Vec::with_capacity(count);
        for index in 0..count {
            let value = values.objectAtIndex(index);
            result.push(Allocation::from_inner(value));
        }
        Ok(result)
    }

    /// Commits pending allocation changes.
    pub fn commit(&self) -> Result<(), Error> {
        self.send_void(sel!(commit), "residency-set commit is unavailable")
    }

    /// Requests residency for the committed set.
    pub fn request_residency(&self) -> Result<(), Error> {
        self.send_void(
            sel!(requestResidency),
            "residency-set requestResidency is unavailable",
        )
    }

    /// Ends residency for the committed set.
    pub fn end_residency(&self) -> Result<(), Error> {
        self.send_void(
            sel!(endResidency),
            "residency-set endResidency is unavailable",
        )
    }

    /// Marks all allocations for removal.
    pub fn remove_all_allocations(&self) -> Result<(), Error> {
        self.send_void(
            sel!(removeAllAllocations),
            "residency-set removeAllAllocations is unavailable",
        )
    }

    fn mutate_allocation(
        &self,
        selector: Sel,
        allocation: ResidencyAllocation<'_>,
        add: bool,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            selector,
            "residency-set allocation mutation is unavailable",
        )?;
        // SAFETY: selector is either addAllocation: or removeAllocation:, both
        // have the same one-object ABI, and the borrow covers the call.
        unsafe {
            if add {
                let _: () = msg_send![self.as_inner(), addAllocation: allocation.as_inner()];
            } else {
                let _: () = msg_send![self.as_inner(), removeAllocation: allocation.as_inner()];
            }
        }
        Ok(())
    }

    fn send_void(&self, selector: Sel, unavailable: &'static str) -> Result<(), Error> {
        require_selector(self.as_inner(), selector, unavailable)?;
        // SAFETY: selector is one of the zero-argument void selectors selected
        // by the public methods above.
        unsafe {
            if selector == sel!(commit) {
                let _: () = msg_send![self.as_inner(), commit];
            } else if selector == sel!(requestResidency) {
                let _: () = msg_send![self.as_inner(), requestResidency];
            } else if selector == sel!(endResidency) {
                let _: () = msg_send![self.as_inner(), endResidency];
            } else {
                let _: () = msg_send![self.as_inner(), removeAllAllocations];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterization_rates_reject_non_finite_and_out_of_range_values() {
        assert!(validate_rate(0.0).is_ok());
        assert!(validate_rate(1.0).is_ok());
        assert!(validate_rate(-0.01).is_err());
        assert!(validate_rate(1.01).is_err());
        assert!(validate_rate(f32::NAN).is_err());
        assert!(validate_rate(f32::INFINITY).is_err());
    }

    #[test]
    fn rasterization_sizes_require_non_zero_planar_dimensions() {
        assert!(checked_size(Size::new(1, 1, 0), "test").is_ok());
        assert!(checked_size(Size::new(0, 1, 0), "test").is_err());
        assert!(checked_size(Size::new(1, 0, 0), "test").is_err());
    }

    #[test]
    fn layer_indices_are_strictly_bounded() {
        assert!(checked_layer_index(2, 0).is_ok());
        assert!(checked_layer_index(2, 1).is_ok());
        assert!(checked_layer_index(2, 2).is_err());
    }
}
