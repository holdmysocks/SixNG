use crate::ThreadBound;
use crate::foundation::{Bundle, Error};
use crate::metal::generated_object_types::metal::{
    AccelerationStructure, Architecture, ArgumentEncoder, BinaryArchive, ComputePipelineReflection,
    CounterSampleBuffer, CounterSet, DepthStencilState, DynamicLibrary, Event, Fence,
    FunctionHandle, Heap, IndirectCommandBuffer, LogState, RasterizationRateMap,
    RenderPipelineReflection, ResidencySet, SamplerState, SharedEvent, SharedEventHandle,
    SharedTextureHandle, Tensor, TextureViewPool,
};
use crate::metal::generated_object_types::metal4::{
    CommandAllocator as Metal4CommandAllocator, Compiler as Metal4Compiler,
    PipelineDataSetSerializer,
};
use crate::metal::generated_struct_types::{
    AccelerationStructureSizes, SamplePosition, SizeAndAlign,
};
use crate::metal::generated_value_types::{
    ArgumentBuffersTier, CounterSamplingPoint, DeviceLocation, FeatureSet, GPUFamily,
    PipelineOption, ReadWriteTextureTier, SparsePageSize, SparseTextureRegionAlignmentMode,
};
use crate::metal::{
    Buffer, CheckedTensorBufferAttachments, CheckedTensorDescriptor, CommandQueue, CompileOptions,
    ComputePipelineState, IoSurface, Library, MAX_TIMESTAMP_COUNTERS, Metal4CommandQueue,
    Mtl4Archive, RenderPipelineDescriptor, RenderPipelineState, ResourceOptions, Texture,
    TextureDescriptor, TimestampCounterHeap,
};
use block2::RcBlock;
use dispatch2::DispatchData;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, MessageReceiver, NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSError, NSString, NSURL};
#[allow(deprecated)]
use objc2_metal::{
    MTL4CommandAllocatorDescriptor, MTL4CommandQueueDescriptor, MTL4CompilerDescriptor,
    MTL4CounterHeapDescriptor, MTL4CounterHeapType, MTL4PipelineDataSetSerializerDescriptor,
    MTLAccelerationStructureDescriptor, MTLArgumentDescriptor, MTLBinaryArchiveDescriptor,
    MTLCommandQueueDescriptor, MTLComputePipelineDescriptor, MTLCounterSampleBufferDescriptor,
    MTLCounterSamplingPoint, MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor, MTLDevice,
    MTLFeatureSet, MTLGPUFamily, MTLHeapDescriptor, MTLIndirectCommandBufferDescriptor,
    MTLLogStateDescriptor, MTLMeshRenderPipelineDescriptor, MTLPipelineOption,
    MTLRasterizationRateMapDescriptor, MTLRegion, MTLResidencySetDescriptor,
    MTLResourceViewPoolDescriptor, MTLSamplePosition, MTLSamplerDescriptor, MTLSharedEventHandle,
    MTLSparsePageSize, MTLSparseTextureRegionAlignmentMode, MTLStitchedLibraryDescriptor,
    MTLTensorDescriptor, MTLTileRenderPipelineDescriptor,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

fn region_from_objc(value: MTLRegion) -> crate::metal::Region {
    crate::metal::Region::new(
        crate::metal::Origin::new(value.origin.x, value.origin.y, value.origin.z),
        crate::metal::Size::new(value.size.width, value.size.height, value.size.depth),
    )
}

fn callback_object(
    value: *mut AnyObject,
    error: *mut NSError,
) -> Result<Retained<AnyObject>, Error> {
    if !error.is_null() {
        // SAFETY: Metal supplies an NSError valid for the callback duration;
        // retaining it makes the owned Rust error independent of that scope.
        if let Some(error) = unsafe { Retained::retain(error) } {
            return Err(crate::foundation::metal_error(&error));
        }
    }
    // SAFETY: Metal supplies the result object for the callback duration;
    // retaining transfers one owned reference into the wrapper.
    unsafe { Retained::retain(value) }
        .ok_or_else(|| Error::unsupported("Metal completed without a result or NSError"))
}

fn callback_optional_object(value: *mut AnyObject) -> Option<Retained<AnyObject>> {
    // SAFETY: nullable reflection objects supplied by Metal remain live for
    // the callback duration; retaining makes present values independently owned.
    unsafe { Retained::retain(value) }
}

/// Runtime-checked Boolean device capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum DeviceCapability {
    BarycentricCoordinates,
    ProgrammableSamplePositions,
    RasterOrderGroups,
    UnifiedMemory,
    Depth24Stencil8,
    Headless,
    LowPower,
    Removable,
    Float32Filtering,
    Float32Msaa,
    BcTextureCompression,
    DynamicLibraries,
    FunctionPointers,
    FunctionPointersFromRender,
    PlacementSparse,
    PrimitiveMotionBlur,
    PullModelInterpolation,
    QueryTextureLod,
    RayTracing,
    RayTracingFromRender,
    RenderDynamicLibraries,
    ShaderBarycentricCoordinates,
}

/// Runtime-checked numeric device properties represented without narrowing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum DeviceNumericProperty {
    CurrentAllocatedSize,
    LocationNumber,
    MaxArgumentBufferSamplerCount,
    MaxBufferLength,
    MaxThreadgroupMemoryLength,
    MaxTransferRate,
    MaximumConcurrentCompilationTaskCount,
    PeerCount,
    PeerGroupId,
    PeerIndex,
    RecommendedMaxWorkingSetSize,
    RegistryId,
    SparseTileSizeInBytes,
    TimestampFrequency,
}

#[derive(Clone)]
/// An owned Metal device protocol object.
pub struct Device {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLDevice>>,
    _thread_bound: ThreadBound,
}

impl Device {
    pub(crate) const fn from_inner(inner: Retained<ProtocolObject<dyn MTLDevice>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: generated callers use this only for selectors declared to
        // return an object conforming to MTLDevice.
        Ok(Self {
            inner: unsafe { Retained::cast_unchecked(inner) },
            _thread_bound: ThreadBound::new(),
        })
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: MTLDevice is backed by this same Objective-C object pointer.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }

    #[must_use]
    /// Returns the system default device, if one exists.
    pub fn system_default() -> Option<Self> {
        MTLCreateSystemDefaultDevice().map(Self::from_inner)
    }

    #[must_use]
    /// Returns the device name as an owned Rust string.
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// Returns owned architecture information after checking availability.
    pub fn architecture(&self) -> Result<Architecture, Error> {
        if !self.inner.respondsToSelector(sel!(architecture)) {
            return Err(Error::unsupported(
                "device architecture information is unavailable",
            ));
        }
        let inner = self.inner.architecture();
        // SAFETY: `MTLDevice::architecture` returns an owned Objective-C
        // `MTLArchitecture` object, which is representation-compatible with
        // the type-erased object retained by the private generated wrapper.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Architecture::from_inner(inner))
    }

    /// Returns the device's counter sets as owned safe wrappers.
    pub fn counter_sets(&self) -> Result<Vec<CounterSet>, Error> {
        if !self.inner.respondsToSelector(sel!(counterSets)) {
            return Err(Error::unsupported("counter-set queries are unavailable"));
        }
        let Some(counter_sets) = self.inner.counterSets() else {
            return Ok(Vec::new());
        };
        Ok(counter_sets
            .to_vec()
            .into_iter()
            .map(|inner| {
                // SAFETY: every element of the typed NSArray conforms to
                // `MTLCounterSet`; only the concrete pointer type is erased.
                CounterSet::from_inner(unsafe { Retained::cast_unchecked(inner) })
            })
            .collect())
    }

    /// Queries a Boolean capability after checking selector availability.
    pub fn capability(&self, capability: DeviceCapability) -> Result<bool, Error> {
        let selector = match capability {
            DeviceCapability::BarycentricCoordinates => sel!(areBarycentricCoordsSupported),
            DeviceCapability::ProgrammableSamplePositions => {
                sel!(areProgrammableSamplePositionsSupported)
            }
            DeviceCapability::RasterOrderGroups => sel!(areRasterOrderGroupsSupported),
            DeviceCapability::UnifiedMemory => sel!(hasUnifiedMemory),
            DeviceCapability::Depth24Stencil8 => sel!(isDepth24Stencil8PixelFormatSupported),
            DeviceCapability::Headless => sel!(isHeadless),
            DeviceCapability::LowPower => sel!(isLowPower),
            DeviceCapability::Removable => sel!(isRemovable),
            DeviceCapability::Float32Filtering => sel!(supports32BitFloatFiltering),
            DeviceCapability::Float32Msaa => sel!(supports32BitMSAA),
            DeviceCapability::BcTextureCompression => sel!(supportsBCTextureCompression),
            DeviceCapability::DynamicLibraries => sel!(supportsDynamicLibraries),
            DeviceCapability::FunctionPointers => sel!(supportsFunctionPointers),
            DeviceCapability::FunctionPointersFromRender => {
                sel!(supportsFunctionPointersFromRender)
            }
            DeviceCapability::PlacementSparse => sel!(supportsPlacementSparse),
            DeviceCapability::PrimitiveMotionBlur => sel!(supportsPrimitiveMotionBlur),
            DeviceCapability::PullModelInterpolation => sel!(supportsPullModelInterpolation),
            DeviceCapability::QueryTextureLod => sel!(supportsQueryTextureLOD),
            DeviceCapability::RayTracing => sel!(supportsRaytracing),
            DeviceCapability::RayTracingFromRender => sel!(supportsRaytracingFromRender),
            DeviceCapability::RenderDynamicLibraries => sel!(supportsRenderDynamicLibraries),
            DeviceCapability::ShaderBarycentricCoordinates => {
                sel!(supportsShaderBarycentricCoordinates)
            }
        };
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(format!(
                "device capability {capability:?} is unavailable"
            )));
        }
        // SAFETY: every closed enum variant selects a checked, no-argument
        // Objective-C selector whose SDK return type is BOOL.
        let value: Bool = unsafe { (&*self.inner).send_message(selector, ()) };
        Ok(value.as_bool())
    }

    /// Queries a numeric property after checking selector availability.
    pub fn numeric_property(&self, property: DeviceNumericProperty) -> Result<u64, Error> {
        macro_rules! value {
            ($selector:ident, $expression:expr) => {{
                if !self.inner.respondsToSelector(sel!($selector)) {
                    return Err(Error::unsupported(format!(
                        "device property {property:?} is unavailable"
                    )));
                }
                $expression as u64
            }};
        }
        Ok(match property {
            DeviceNumericProperty::CurrentAllocatedSize => {
                value!(currentAllocatedSize, self.inner.currentAllocatedSize())
            }
            DeviceNumericProperty::LocationNumber => {
                value!(locationNumber, self.inner.locationNumber())
            }
            DeviceNumericProperty::MaxArgumentBufferSamplerCount => value!(
                maxArgumentBufferSamplerCount,
                self.inner.maxArgumentBufferSamplerCount()
            ),
            DeviceNumericProperty::MaxBufferLength => {
                value!(maxBufferLength, self.inner.maxBufferLength())
            }
            DeviceNumericProperty::MaxThreadgroupMemoryLength => value!(
                maxThreadgroupMemoryLength,
                self.inner.maxThreadgroupMemoryLength()
            ),
            DeviceNumericProperty::MaxTransferRate => {
                value!(maxTransferRate, self.inner.maxTransferRate())
            }
            DeviceNumericProperty::MaximumConcurrentCompilationTaskCount => value!(
                maximumConcurrentCompilationTaskCount,
                self.inner.maximumConcurrentCompilationTaskCount()
            ),
            DeviceNumericProperty::PeerCount => value!(peerCount, self.inner.peerCount()),
            DeviceNumericProperty::PeerGroupId => value!(peerGroupID, self.inner.peerGroupID()),
            DeviceNumericProperty::PeerIndex => value!(peerIndex, self.inner.peerIndex()),
            DeviceNumericProperty::RecommendedMaxWorkingSetSize => value!(
                recommendedMaxWorkingSetSize,
                self.inner.recommendedMaxWorkingSetSize()
            ),
            DeviceNumericProperty::RegistryId => value!(registryID, self.inner.registryID()),
            DeviceNumericProperty::SparseTileSizeInBytes => {
                value!(sparseTileSizeInBytes, self.inner.sparseTileSizeInBytes())
            }
            DeviceNumericProperty::TimestampFrequency => value!(
                queryTimestampFrequency,
                self.inner.queryTimestampFrequency()
            ),
        })
    }

    /// Returns the maximum threads per threadgroup.
    #[must_use]
    pub fn max_threads_per_threadgroup(&self) -> crate::metal::Size {
        let value = self.inner.maxThreadsPerThreadgroup();
        crate::metal::Size::new(value.width, value.height, value.depth)
    }

    /// Returns the minimum linear-texture and texture-buffer alignments.
    #[must_use]
    pub fn texture_alignments(&self, format: crate::metal::PixelFormat) -> (usize, usize) {
        (
            self.inner
                .minimumLinearTextureAlignmentForPixelFormat(format.as_objc()),
            self.inner
                .minimumTextureBufferAlignmentForPixelFormat(format.as_objc()),
        )
    }

    /// Samples synchronized CPU and GPU timestamps into owned Rust values.
    pub fn sample_timestamps(&self) -> (u64, u64) {
        let mut cpu = 0_u64;
        let mut gpu = 0_u64;
        // SAFETY: both pointers refer to initialized writable u64 values for
        // the duration of the call, matching MTLTimestamp's SDK typedef.
        unsafe {
            self.inner
                .sampleTimestamps_gpuTimestamp((&mut cpu).into(), (&mut gpu).into())
        };
        (cpu, gpu)
    }

    /// Returns the physical location category reported by Metal.
    #[must_use]
    pub fn location(&self) -> DeviceLocation {
        DeviceLocation::from_system_raw(self.inner.location().0)
    }

    /// Returns the supported argument-buffer tier.
    #[must_use]
    pub fn argument_buffers_tier(&self) -> ArgumentBuffersTier {
        ArgumentBuffersTier::from_system_raw(self.inner.argumentBuffersSupport().0)
    }

    /// Returns the supported read/write texture tier.
    #[must_use]
    pub fn read_write_texture_tier(&self) -> ReadWriteTextureTier {
        ReadWriteTextureTier::from_system_raw(self.inner.readWriteTextureSupport().0)
    }

    /// Checks support for a declared GPU family.
    pub fn supports_family(&self, family: GPUFamily) -> Result<bool, Error> {
        if !self.inner.respondsToSelector(sel!(supportsFamily:)) {
            return Err(Error::unsupported("GPU-family queries are unavailable"));
        }
        Ok(self.inner.supportsFamily(MTLGPUFamily(family.as_raw())))
    }

    /// Checks support for a declared legacy feature set.
    #[allow(deprecated)]
    pub fn supports_feature_set(&self, feature_set: FeatureSet) -> Result<bool, Error> {
        if !self.inner.respondsToSelector(sel!(supportsFeatureSet:)) {
            return Err(Error::unsupported("feature-set queries are unavailable"));
        }
        Ok(self
            .inner
            .supportsFeatureSet(MTLFeatureSet(feature_set.as_raw())))
    }

    /// Checks support for a declared counter-sampling point.
    pub fn supports_counter_sampling(&self, point: CounterSamplingPoint) -> Result<bool, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(supportsCounterSampling:))
        {
            return Err(Error::unsupported(
                "counter-sampling queries are unavailable",
            ));
        }
        Ok(self
            .inner
            .supportsCounterSampling(MTLCounterSamplingPoint(point.as_raw())))
    }

    /// Checks whether a rasterization-rate map supports `layer_count` layers.
    pub fn supports_rasterization_rate_map(&self, layer_count: usize) -> Result<bool, Error> {
        if layer_count == 0 {
            return Err(Error::invalid_argument("layer count must be non-zero"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(supportsRasterizationRateMapWithLayerCount:))
        {
            return Err(Error::unsupported(
                "rasterization-rate-map queries are unavailable",
            ));
        }
        // SAFETY: the selector is present and the layer count is non-zero;
        // unsupported counts are reported by the Boolean result.
        Ok(unsafe {
            self.inner
                .supportsRasterizationRateMapWithLayerCount(layer_count)
        })
    }

    /// Checks whether a vertex-amplification count is supported.
    pub fn supports_vertex_amplification_count(&self, count: usize) -> Result<bool, Error> {
        if count == 0 {
            return Err(Error::invalid_argument(
                "vertex amplification count must be non-zero",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(supportsVertexAmplificationCount:))
        {
            return Err(Error::unsupported(
                "vertex-amplification queries are unavailable",
            ));
        }
        Ok(self.inner.supportsVertexAmplificationCount(count))
    }

    /// Returns whether Metal should maximize concurrent compilation.
    pub fn should_maximize_concurrent_compilation(&self) -> Result<bool, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(shouldMaximizeConcurrentCompilation))
        {
            return Err(Error::unsupported(
                "concurrent-compilation policy is unavailable",
            ));
        }
        Ok(self.inner.shouldMaximizeConcurrentCompilation())
    }

    /// Sets Metal's concurrent-compilation policy.
    pub fn set_should_maximize_concurrent_compilation(&self, value: bool) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(setShouldMaximizeConcurrentCompilation:))
        {
            return Err(Error::unsupported(
                "concurrent-compilation policy is unavailable",
            ));
        }
        self.inner.setShouldMaximizeConcurrentCompilation(value);
        Ok(())
    }

    /// Returns the device's default programmable sample positions.
    pub fn default_sample_positions(&self, count: usize) -> Result<Vec<SamplePosition>, Error> {
        if count > 32 {
            return Err(Error::invalid_argument(
                "Metal supports at most 32 programmable sample positions",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(getDefaultSamplePositions:count:))
        {
            return Err(Error::unsupported(
                "default programmable sample positions are unavailable",
            ));
        }
        let mut positions = vec![MTLSamplePosition { x: 0.0, y: 0.0 }; count];
        // SAFETY: the vector owns writable storage for exactly `count`
        // MTLSamplePosition values during this synchronous call.
        let pointer = NonNull::new(positions.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        // SAFETY: the non-null pointer references writable storage for exactly
        // `count` MTLSamplePosition values (or is dangling when count is zero).
        unsafe { self.inner.getDefaultSamplePositions_count(pointer, count) };
        Ok(positions
            .into_iter()
            .map(|position| SamplePosition {
                x: position.x,
                y: position.y,
            })
            .collect())
    }

    /// Returns heap size and alignment for a checked buffer allocation.
    pub fn heap_buffer_size_and_align(
        &self,
        length: usize,
        options: ResourceOptions,
    ) -> Result<SizeAndAlign, Error> {
        if length == 0 {
            return Err(Error::invalid_argument("buffer length must be non-zero"));
        }
        let value = self
            .inner
            .heapBufferSizeAndAlignWithLength_options(length, options.as_objc());
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Returns heap size and alignment for a validated texture descriptor.
    pub fn heap_texture_size_and_align(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<SizeAndAlign, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(heapTextureSizeAndAlignWithDescriptor:))
        {
            return Err(Error::unsupported("texture heap sizing is unavailable"));
        }
        let value = self
            .inner
            .heapTextureSizeAndAlignWithDescriptor(&descriptor.inner);
        if value.size == 0 || value.align == 0 {
            return Err(Error::invalid_argument(
                "the texture descriptor cannot be allocated from a heap",
            ));
        }
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Returns heap size and alignment for an acceleration structure size.
    pub fn heap_acceleration_structure_size_and_align(
        &self,
        size: usize,
    ) -> Result<SizeAndAlign, Error> {
        if size == 0 {
            return Err(Error::invalid_argument(
                "acceleration-structure size must be non-zero",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(heapAccelerationStructureSizeAndAlignWithSize:))
        {
            return Err(Error::unsupported(
                "acceleration-structure heap sizing is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the size is non-zero.
        let value = unsafe {
            self.inner
                .heapAccelerationStructureSizeAndAlignWithSize(size)
        };
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Returns build and scratch sizes for an acceleration-structure descriptor.
    pub fn acceleration_structure_sizes(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::AccelerationStructureDescriptor,
    ) -> Result<AccelerationStructureSizes, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(accelerationStructureSizesWithDescriptor:))
        {
            return Err(Error::unsupported(
                "acceleration-structure sizing is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLAccelerationStructureDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid acceleration-structure descriptor object")
            })?;
        let value = self
            .inner
            .accelerationStructureSizesWithDescriptor(descriptor);
        Ok(AccelerationStructureSizes {
            acceleration_structure_size: value.accelerationStructureSize,
            build_scratch_buffer_size: value.buildScratchBufferSize,
            refit_scratch_buffer_size: value.refitScratchBufferSize,
        })
    }

    /// Returns heap size and alignment for an acceleration-structure descriptor.
    pub fn heap_acceleration_structure_descriptor_size_and_align(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::AccelerationStructureDescriptor,
    ) -> Result<SizeAndAlign, Error> {
        if !self.inner.respondsToSelector(sel!(
            heapAccelerationStructureSizeAndAlignWithDescriptor:
        )) {
            return Err(Error::unsupported(
                "acceleration-structure descriptor heap sizing is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLAccelerationStructureDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid acceleration-structure descriptor object")
            })?;
        let value = self
            .inner
            .heapAccelerationStructureSizeAndAlignWithDescriptor(descriptor);
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Returns the sparse tile size for a texture configuration.
    pub fn sparse_tile_size(
        &self,
        texture_type: crate::metal::TextureType,
        pixel_format: crate::metal::PixelFormat,
        sample_count: usize,
        page_size: Option<SparsePageSize>,
    ) -> Result<crate::metal::Size, Error> {
        if sample_count == 0 {
            return Err(Error::invalid_argument("sample count must be non-zero"));
        }
        if !self.inner.supportsTextureSampleCount(sample_count) {
            return Err(Error::invalid_argument(
                "sample count is unsupported by this device",
            ));
        }
        let value = match page_size {
            None => {
                if !self.inner.respondsToSelector(
                    sel!(sparseTileSizeWithTextureType:pixelFormat:sampleCount:),
                ) {
                    return Err(Error::unsupported(
                        "sparse tile-size queries are unavailable",
                    ));
                }
                // SAFETY: selector availability, sample-count support, and the
                // checked public enum inputs were established above.
                unsafe {
                    self.inner
                        .sparseTileSizeWithTextureType_pixelFormat_sampleCount(
                            texture_type.as_objc(),
                            pixel_format.as_objc(),
                            sample_count,
                        )
                }
            }
            Some(page_size) => {
                if !self.inner.respondsToSelector(
                    sel!(sparseTileSizeWithTextureType:pixelFormat:sampleCount:sparsePageSize:),
                ) {
                    return Err(Error::unsupported(
                        "sparse page-size selection is unavailable",
                    ));
                }
                // SAFETY: selector availability, sample-count support, and all
                // checked public enum inputs were established above.
                unsafe {
                    self.inner
                        .sparseTileSizeWithTextureType_pixelFormat_sampleCount_sparsePageSize(
                            texture_type.as_objc(),
                            pixel_format.as_objc(),
                            sample_count,
                            MTLSparsePageSize(page_size.as_raw()),
                        )
                }
            }
        };
        Ok(crate::metal::Size::new(
            value.width,
            value.height,
            value.depth,
        ))
    }

    /// Converts sparse pixel regions to tile regions using owned Rust slices.
    pub fn sparse_pixel_regions_to_tiles(
        &self,
        pixel_regions: &[crate::metal::Region],
        tile_size: crate::metal::Size,
        mode: SparseTextureRegionAlignmentMode,
    ) -> Result<Vec<crate::metal::Region>, Error> {
        if pixel_regions.is_empty() {
            return Ok(Vec::new());
        }
        if tile_size.width == 0 || tile_size.height == 0 || tile_size.depth == 0 {
            return Err(Error::invalid_argument(
                "sparse tile dimensions must be non-zero",
            ));
        }
        if pixel_regions.iter().any(|region| {
            region.size.width == 0 || region.size.height == 0 || region.size.depth == 0
        }) {
            return Err(Error::invalid_argument(
                "sparse pixel regions must have non-zero dimensions",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            convertSparsePixelRegions:toTileRegions:withTileSize:alignmentMode:numRegions:
        )) {
            return Err(Error::unsupported(
                "sparse pixel-region conversion is unavailable",
            ));
        }
        let mut source: Vec<MTLRegion> = pixel_regions.iter().copied().map(Into::into).collect();
        let mut destination: Vec<MTLRegion> =
            vec![crate::metal::Region::default().into(); source.len()];
        let source_pointer = NonNull::new(source.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        let destination_pointer =
            NonNull::new(destination.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        // SAFETY: both vectors contain exactly `source.len()` MTLRegion values,
        // their writable storage remains alive for the synchronous call, the
        // tile dimensions are non-zero, and the selector was checked.
        unsafe {
            self.inner
                .convertSparsePixelRegions_toTileRegions_withTileSize_alignmentMode_numRegions(
                    source_pointer,
                    destination_pointer,
                    tile_size.into(),
                    MTLSparseTextureRegionAlignmentMode(mode.as_raw()),
                    source.len(),
                )
        };
        Ok(destination.into_iter().map(region_from_objc).collect())
    }

    /// Converts sparse tile regions to pixel regions using owned Rust slices.
    pub fn sparse_tile_regions_to_pixels(
        &self,
        tile_regions: &[crate::metal::Region],
        tile_size: crate::metal::Size,
    ) -> Result<Vec<crate::metal::Region>, Error> {
        if tile_regions.is_empty() {
            return Ok(Vec::new());
        }
        if tile_size.width == 0 || tile_size.height == 0 || tile_size.depth == 0 {
            return Err(Error::invalid_argument(
                "sparse tile dimensions must be non-zero",
            ));
        }
        if tile_regions.iter().any(|region| {
            region.size.width == 0 || region.size.height == 0 || region.size.depth == 0
        }) {
            return Err(Error::invalid_argument(
                "sparse tile regions must have non-zero dimensions",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            convertSparseTileRegions:toPixelRegions:withTileSize:numRegions:
        )) {
            return Err(Error::unsupported(
                "sparse tile-region conversion is unavailable",
            ));
        }
        let mut source: Vec<MTLRegion> = tile_regions.iter().copied().map(Into::into).collect();
        let mut destination: Vec<MTLRegion> =
            vec![crate::metal::Region::default().into(); source.len()];
        let source_pointer = NonNull::new(source.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        let destination_pointer =
            NonNull::new(destination.as_mut_ptr()).unwrap_or_else(NonNull::dangling);
        // SAFETY: both vectors contain exactly `source.len()` MTLRegion values,
        // their writable storage remains alive for the synchronous call, the
        // tile dimensions are non-zero, and the selector was checked.
        unsafe {
            self.inner
                .convertSparseTileRegions_toPixelRegions_withTileSize_numRegions(
                    source_pointer,
                    destination_pointer,
                    tile_size.into(),
                    source.len(),
                )
        };
        Ok(destination.into_iter().map(region_from_objc).collect())
    }

    /// Creates a command queue, optionally with a checked upper bound.
    pub fn new_command_queue(
        &self,
        max_command_buffers: Option<usize>,
    ) -> Result<CommandQueue, Error> {
        let inner = match max_command_buffers {
            None => self.inner.newCommandQueue(),
            Some(count) if count > 0 => self.inner.newCommandQueueWithMaxCommandBufferCount(count),
            Some(_) => {
                return Err(Error::invalid_argument(
                    "command buffer count must be non-zero",
                ));
            }
        };
        inner
            .map(CommandQueue::new)
            .ok_or_else(|| Error::unsupported("Metal could not create a command queue"))
    }

    /// Creates a command queue from a validated descriptor object.
    pub fn new_command_queue_from_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::CommandQueueDescriptor,
    ) -> Result<CommandQueue, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newCommandQueueWithDescriptor:))
        {
            return Err(Error::unsupported(
                "command queue descriptors are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLCommandQueueDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid command queue descriptor object"))?;
        self.inner
            .newCommandQueueWithDescriptor(descriptor)
            .map(CommandQueue::new)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the command queue descriptor"))
    }

    /// Creates an argument encoder from an owned slice of descriptors.
    pub fn new_argument_encoder(
        &self,
        arguments: &[crate::metal::generated_object_types::metal::ArgumentDescriptor],
    ) -> Result<ArgumentEncoder, Error> {
        if arguments.is_empty() {
            return Err(Error::invalid_argument(
                "argument encoder descriptors must be non-empty",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newArgumentEncoderWithArguments:))
        {
            return Err(Error::unsupported("argument encoders are unavailable"));
        }
        let arguments: Result<Vec<&MTLArgumentDescriptor>, Error> = arguments
            .iter()
            .map(|argument| {
                argument
                    .as_inner()
                    .downcast_ref::<MTLArgumentDescriptor>()
                    .ok_or_else(|| Error::invalid_argument("invalid argument descriptor object"))
            })
            .collect();
        let arguments = NSArray::from_slice(&arguments?);
        let inner = self
            .inner
            .newArgumentEncoderWithArguments(&arguments)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the argument descriptors"))?;
        // SAFETY: the typed factory returned an owned MTLArgumentEncoder.
        Ok(ArgumentEncoder::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates an argument encoder from a reflected buffer binding.
    pub fn new_argument_encoder_from_buffer_binding(
        &self,
        buffer_binding: &crate::metal::generated_object_types::metal::BufferBinding,
    ) -> Result<ArgumentEncoder, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newArgumentEncoderWithBufferBinding:))
        {
            return Err(Error::unsupported(
                "buffer-binding argument encoders are unavailable",
            ));
        }
        // SAFETY: generated BufferBinding values can only originate from
        // selectors declared to return MTLBufferBinding objects. The checked
        // selector takes one Objective-C object and returns a retained object.
        let inner: Option<Retained<AnyObject>> = unsafe {
            msg_send![&*self.inner,
                newArgumentEncoderWithBufferBinding: buffer_binding.as_inner()
            ]
        };
        inner
            .map(ArgumentEncoder::from_inner)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the buffer binding"))
    }

    /// Allocates a buffer with the selected storage mode.
    pub fn new_buffer(&self, length: usize, options: ResourceOptions) -> Result<Buffer, Error> {
        if length == 0 {
            return Err(Error::invalid_argument("buffer length must be non-zero"));
        }
        self.inner
            .newBufferWithLength_options(length, options.as_objc())
            .map(|inner| Buffer::new(inner, options.storage_mode()))
            .ok_or_else(|| Error::unsupported("Metal could not allocate the buffer"))
    }

    /// Allocates a buffer and synchronously copies owned Rust bytes into it.
    pub fn new_buffer_with_bytes(
        &self,
        bytes: &[u8],
        options: ResourceOptions,
    ) -> Result<Buffer, Error> {
        if bytes.is_empty() {
            return Err(Error::invalid_argument("buffer contents must be non-empty"));
        }
        if bytes.len() > self.inner.maxBufferLength() {
            return Err(Error::invalid_argument(
                "buffer contents exceed the device maximum buffer length",
            ));
        }
        let pointer = NonNull::from(&bytes[0]).cast();
        // SAFETY: `pointer` references `bytes.len()` initialized bytes and
        // Metal's `newBufferWithBytes` copies them before returning, so no
        // caller-provided pointer or lifetime escapes this safe method.
        unsafe {
            self.inner
                .newBufferWithBytes_length_options(pointer, bytes.len(), options.as_objc())
        }
        .map(|inner| Buffer::new(inner, options.storage_mode()))
        .ok_or_else(|| Error::unsupported("Metal could not allocate the initialized buffer"))
    }

    /// Allocates a placement-sparse buffer after capability and size checks.
    pub fn new_placement_sparse_buffer(
        &self,
        length: usize,
        options: ResourceOptions,
        page_size: SparsePageSize,
    ) -> Result<Buffer, Error> {
        if length == 0 || length > self.inner.maxBufferLength() {
            return Err(Error::invalid_argument(
                "sparse buffer length must be within the device buffer limit",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newBufferWithLength:options:placementSparsePageSize:))
            || !self.capability(DeviceCapability::PlacementSparse)?
        {
            return Err(Error::unsupported(
                "placement-sparse buffers are unavailable",
            ));
        }
        // SAFETY: the length is non-zero and bounded by maxBufferLength, the
        // device reports placement-sparse support, and page/options values are
        // checked Rust wrappers rather than caller-controlled raw integers.
        unsafe {
            self.inner
                .newBufferWithLength_options_placementSparsePageSize(
                    length,
                    options.as_objc(),
                    MTLSparsePageSize(page_size.as_raw()),
                )
        }
        .map(|inner| Buffer::new(inner, options.storage_mode()))
        .ok_or_else(|| Error::invalid_argument("Metal rejected the sparse buffer configuration"))
    }

    /// Allocates a texture from a validated descriptor.
    pub fn new_texture(&self, descriptor: &TextureDescriptor) -> Result<Texture, Error> {
        self.inner
            .newTextureWithDescriptor(&descriptor.inner)
            .map(Texture::new)
            .ok_or_else(|| Error::unsupported("Metal could not allocate the texture"))
    }

    /// Creates a texture backed by an owned IOSurface identity.
    pub fn new_texture_from_iosurface(
        &self,
        descriptor: &TextureDescriptor,
        iosurface: &IoSurface,
        plane: usize,
    ) -> Result<Texture, Error> {
        if !self.inner.respondsToSelector(sel!(
            newTextureWithDescriptor:iosurface:plane:
        )) {
            return Err(Error::unsupported(
                "IOSurface-backed textures are unavailable",
            ));
        }
        // SAFETY: the selector was checked above, `descriptor` owns the
        // concrete SDK descriptor, and `IoSurface` can only contain a retained
        // IOSurface returned by Metal. Objective-C and Core Foundation object
        // references use the same pointer ABI for this selector. Metal checks
        // the plane against the surface layout and returns nil when invalid.
        let inner: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                self.as_any_object(),
                newTextureWithDescriptor: &*descriptor.inner,
                iosurface: iosurface.as_inner(),
                plane: plane
            ]
        };
        inner
            .ok_or_else(|| {
                Error::invalid_argument("Metal rejected the IOSurface texture configuration")
            })
            .and_then(Texture::from_any_object)
    }

    /// Allocates a cross-process shareable texture.
    pub fn new_shared_texture(&self, descriptor: &TextureDescriptor) -> Result<Texture, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newSharedTextureWithDescriptor:))
        {
            return Err(Error::unsupported("shared textures are unavailable"));
        }
        self.inner
            .newSharedTextureWithDescriptor(&descriptor.inner)
            .map(Texture::new)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the shared texture descriptor"))
    }

    /// Recreates a cross-process texture from an owned shared handle.
    pub fn new_shared_texture_from_handle(
        &self,
        handle: &SharedTextureHandle,
    ) -> Result<Texture, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newSharedTextureWithHandle:))
        {
            return Err(Error::unsupported("shared texture handles are unavailable"));
        }
        let handle = handle
            .as_inner()
            .downcast_ref::<objc2_metal::MTLSharedTextureHandle>()
            .ok_or_else(|| Error::invalid_argument("invalid shared texture handle object"))?;
        self.inner
            .newSharedTextureWithHandle(handle)
            .map(Texture::new)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the shared texture handle"))
    }

    /// Creates an indirect command buffer with a conservatively bounded count.
    pub fn new_indirect_command_buffer(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::IndirectCommandBufferDescriptor,
        max_count: usize,
        options: ResourceOptions,
    ) -> Result<IndirectCommandBuffer, Error> {
        if max_count == 0 || max_count > u32::MAX as usize {
            return Err(Error::invalid_argument(
                "indirect command count must be between 1 and u32::MAX",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            newIndirectCommandBufferWithDescriptor:maxCommandCount:options:
        )) {
            return Err(Error::unsupported(
                "indirect command buffers are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLIndirectCommandBufferDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid indirect command buffer descriptor object")
            })?;
        // SAFETY: max_count is non-zero and conservatively bounded to u32,
        // the descriptor's runtime class and selector availability were
        // checked, and ResourceOptions cannot contain unvalidated raw bits.
        let inner = unsafe {
            self.inner
                .newIndirectCommandBufferWithDescriptor_maxCommandCount_options(
                    descriptor,
                    max_count,
                    options.as_objc(),
                )
        }
        .ok_or_else(|| {
            Error::invalid_argument("Metal rejected the indirect command buffer configuration")
        })?;
        // SAFETY: the typed factory returned an owned MTLIndirectCommandBuffer.
        Ok(IndirectCommandBuffer::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates the default library from the application's main bundle.
    pub fn new_default_library(&self) -> Result<Library, Error> {
        if !self.inner.respondsToSelector(sel!(newDefaultLibrary)) {
            return Err(Error::unsupported(
                "default Metal libraries are unavailable",
            ));
        }
        self.inner
            .newDefaultLibrary()
            .map(Library::new)
            .ok_or_else(|| Error::unsupported("the main bundle contains no default Metal library"))
    }

    /// Creates the default Metal library from a retained Foundation bundle.
    pub fn new_default_library_from_bundle(&self, bundle: &Bundle) -> Result<Library, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newDefaultLibraryWithBundle:error:))
        {
            return Err(Error::unsupported(
                "bundle-based default Metal libraries are unavailable",
            ));
        }
        self.inner
            .newDefaultLibraryWithBundle_error(bundle.as_inner())
            .map(Library::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Loads a Metal library from a UTF-8 filesystem path.
    pub fn new_library_from_path(&self, path: &str) -> Result<Library, Error> {
        if path.is_empty() || path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("Metal library path is invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newLibraryWithURL:error:))
        {
            return Err(Error::unsupported(
                "file-based Metal libraries are unavailable",
            ));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        self.inner
            .newLibraryWithURL_error(&url)
            .map(Library::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Loads a Metal library from owned dispatch data copied from Rust bytes.
    pub fn new_library_from_bytes(&self, bytes: &[u8]) -> Result<Library, Error> {
        if bytes.is_empty() {
            return Err(Error::invalid_argument(
                "Metal library data must not be empty",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newLibraryWithData:error:))
        {
            return Err(Error::unsupported(
                "data-based Metal libraries are unavailable",
            ));
        }
        // DispatchData::from_bytes copies the complete slice before returning,
        // so Metal never borrows caller-owned Rust memory. The retained data
        // remains alive through the synchronous factory call.
        let data = DispatchData::from_bytes(bytes);
        self.inner
            .newLibraryWithData_error(&data)
            .map(Library::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Creates a depth/stencil state from a validated descriptor object.
    pub fn new_depth_stencil_state(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::DepthStencilDescriptor,
    ) -> Result<DepthStencilState, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newDepthStencilStateWithDescriptor:))
        {
            return Err(Error::unsupported("depth/stencil states are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLDepthStencilDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid depth/stencil descriptor object"))?;
        let inner = self
            .inner
            .newDepthStencilStateWithDescriptor(descriptor)
            .ok_or_else(|| {
                Error::invalid_argument("Metal rejected the depth/stencil descriptor")
            })?;
        // SAFETY: the typed factory returned an owned object conforming to
        // MTLDepthStencilState; only the pointer's static type is erased.
        Ok(DepthStencilState::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a heap from a validated descriptor object.
    pub fn new_heap(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::HeapDescriptor,
    ) -> Result<Heap, Error> {
        if !self.inner.respondsToSelector(sel!(newHeapWithDescriptor:)) {
            return Err(Error::unsupported("Metal heaps are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLHeapDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid heap descriptor object"))?;
        let inner = self
            .inner
            .newHeapWithDescriptor(descriptor)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the heap descriptor"))?;
        // SAFETY: the typed factory returned an owned MTLHeap protocol object.
        Ok(Heap::from_inner(unsafe { Retained::cast_unchecked(inner) }))
    }

    /// Creates a sampler state from a validated descriptor object.
    pub fn new_sampler_state(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::SamplerDescriptor,
    ) -> Result<SamplerState, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newSamplerStateWithDescriptor:))
        {
            return Err(Error::unsupported("sampler states are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLSamplerDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid sampler descriptor object"))?;
        let inner = self
            .inner
            .newSamplerStateWithDescriptor(descriptor)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the sampler descriptor"))?;
        // SAFETY: the typed factory returned an owned MTLSamplerState object.
        Ok(SamplerState::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a single-device event.
    pub fn new_event(&self) -> Result<Event, Error> {
        if !self.inner.respondsToSelector(sel!(newEvent)) {
            return Err(Error::unsupported("Metal events are unavailable"));
        }
        let inner = self
            .inner
            .newEvent()
            .ok_or_else(|| Error::unsupported("Metal could not create an event"))?;
        // SAFETY: the typed factory returned an owned MTLEvent protocol object.
        Ok(Event::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a single-device fence.
    pub fn new_fence(&self) -> Result<Fence, Error> {
        if !self.inner.respondsToSelector(sel!(newFence)) {
            return Err(Error::unsupported("Metal fences are unavailable"));
        }
        let inner = self
            .inner
            .newFence()
            .ok_or_else(|| Error::unsupported("Metal could not create a fence"))?;
        // SAFETY: the typed factory returned an owned MTLFence protocol object.
        Ok(Fence::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a cross-device shareable event.
    pub fn new_shared_event(&self) -> Result<SharedEvent, Error> {
        if !self.inner.respondsToSelector(sel!(newSharedEvent)) {
            return Err(Error::unsupported("shared Metal events are unavailable"));
        }
        let inner = self
            .inner
            .newSharedEvent()
            .ok_or_else(|| Error::unsupported("Metal could not create a shared event"))?;
        // SAFETY: the typed factory returned an owned MTLSharedEvent object.
        Ok(SharedEvent::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Recreates a shareable event from an owned handle.
    pub fn new_shared_event_from_handle(
        &self,
        handle: &SharedEventHandle,
    ) -> Result<SharedEvent, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newSharedEventWithHandle:))
        {
            return Err(Error::unsupported("shared event handles are unavailable"));
        }
        let handle = handle
            .as_inner()
            .downcast_ref::<MTLSharedEventHandle>()
            .ok_or_else(|| Error::invalid_argument("invalid shared event handle object"))?;
        let inner = self
            .inner
            .newSharedEventWithHandle(handle)
            .ok_or_else(|| Error::invalid_argument("Metal rejected the shared event handle"))?;
        // SAFETY: the typed factory returned an owned MTLSharedEvent object.
        Ok(SharedEvent::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Compiles Metal Shading Language source into an owned library.
    pub fn new_library_from_source(
        &self,
        source: &str,
        options: Option<&CompileOptions>,
    ) -> Result<Library, Error> {
        if source.is_empty() || source.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("Metal source is invalid"));
        }
        let source = NSString::from_str(source);
        self.inner
            .newLibraryWithSource_options_error(&source, options.map(|value| &*value.inner))
            .map(Library::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous Metal source compilation.
    pub fn new_library_from_source_async(
        &self,
        source: &str,
        options: Option<&CompileOptions>,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if source.is_empty() || source.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("Metal source is invalid"));
        }
        if !self.inner.respondsToSelector(sel!(
            newLibraryWithSource:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous Metal source compilation is unavailable",
            ));
        }
        let source = NSString::from_str(source);
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_object(value, error).and_then(Library::from_any_object);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; source/options remain
        // alive for the call and RcBlock has the documented two-object ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newLibraryWithSource: &*source,
                options: options.map(|value| &*value.inner),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a stitched library from a validated descriptor.
    pub fn new_stitched_library(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::StitchedLibraryDescriptor,
    ) -> Result<Library, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newLibraryWithStitchedDescriptor:error:))
        {
            return Err(Error::unsupported("stitched libraries are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLStitchedLibraryDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid stitched library descriptor object"))?;
        self.inner
            .newLibraryWithStitchedDescriptor_error(descriptor)
            .map(Library::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous stitched-library compilation.
    pub fn new_stitched_library_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::StitchedLibraryDescriptor,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(
            newLibraryWithStitchedDescriptor:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous stitched-library compilation is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLStitchedLibraryDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid stitched library descriptor object"))?;
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_object(value, error).and_then(Library::from_any_object);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector and descriptor class were checked; the block has
        // the documented library/error ABI and Metal copies it for delivery.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newLibraryWithStitchedDescriptor: descriptor,
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Creates a binary archive and preserves Metal's NSError diagnostics.
    pub fn new_binary_archive(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::BinaryArchiveDescriptor,
    ) -> Result<BinaryArchive, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newBinaryArchiveWithDescriptor:error:))
        {
            return Err(Error::unsupported("binary archives are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLBinaryArchiveDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid binary archive descriptor object"))?;
        let inner = self
            .inner
            .newBinaryArchiveWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLBinaryArchive object.
        Ok(BinaryArchive::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a counter sample buffer and preserves Metal's NSError diagnostics.
    pub fn new_counter_sample_buffer(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::CounterSampleBufferDescriptor,
    ) -> Result<CounterSampleBuffer, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newCounterSampleBufferWithDescriptor:error:))
        {
            return Err(Error::unsupported("counter sample buffers are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLCounterSampleBufferDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid counter sample buffer descriptor object")
            })?;
        let inner = self
            .inner
            .newCounterSampleBufferWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLCounterSampleBuffer.
        Ok(CounterSampleBuffer::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a Metal log state and preserves NSError diagnostics.
    pub fn new_log_state(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::LogStateDescriptor,
    ) -> Result<LogState, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newLogStateWithDescriptor:error:))
        {
            return Err(Error::unsupported("Metal log states are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLLogStateDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid log state descriptor object"))?;
        let inner = self
            .inner
            .newLogStateWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLLogState.
        Ok(LogState::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a dynamic library from a compiled Metal library.
    pub fn new_dynamic_library(&self, library: &Library) -> Result<DynamicLibrary, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newDynamicLibrary:error:))
        {
            return Err(Error::unsupported(
                "dynamic Metal libraries are unavailable",
            ));
        }
        let inner = self
            .inner
            .newDynamicLibrary_error(&library.inner)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLDynamicLibrary.
        Ok(DynamicLibrary::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Loads a dynamic Metal library from a UTF-8 filesystem path.
    pub fn new_dynamic_library_from_path(&self, path: &str) -> Result<DynamicLibrary, Error> {
        if path.is_empty() || path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("dynamic library path is invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newDynamicLibraryWithURL:error:))
        {
            return Err(Error::unsupported(
                "file-based dynamic Metal libraries are unavailable",
            ));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let inner = self
            .inner
            .newDynamicLibraryWithURL_error(&url)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLDynamicLibrary.
        Ok(DynamicLibrary::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Returns a pipeline-independent handle for a compiled function.
    pub fn function_handle(
        &self,
        function: &crate::metal::Function,
    ) -> Result<FunctionHandle, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(functionHandleWithFunction:))
        {
            return Err(Error::unsupported("Metal function handles are unavailable"));
        }
        // SAFETY: `Function` privately owns a valid MTLFunction protocol
        // object, and the call only derives a retained handle from it.
        let inner =
            unsafe { self.inner.functionHandleWithFunction(&function.inner) }.ok_or_else(|| {
                Error::invalid_argument(
                    "the function was not compiled for pipeline-independent use",
                )
            })?;
        // SAFETY: the typed method returned an owned MTLFunctionHandle.
        Ok(FunctionHandle::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Returns a pipeline-independent handle for a Metal 4 binary function.
    pub fn function_handle_from_binary(
        &self,
        function: &crate::metal::generated_object_types::metal4::BinaryFunction,
    ) -> Result<FunctionHandle, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(functionHandleWithBinaryFunction:))
        {
            return Err(Error::unsupported(
                "Metal 4 binary function handles are unavailable",
            ));
        }
        // SAFETY: BinaryFunction is a private owned wrapper produced only by
        // MTL4 binary-function selectors; the checked method returns an owned
        // MTLFunctionHandle or nil without exposing either object pointer.
        let inner: Option<Retained<AnyObject>> = unsafe {
            msg_send![self.as_any_object(), functionHandleWithBinaryFunction: function.as_inner()]
        };
        inner
            .map(FunctionHandle::from_inner)
            .ok_or_else(|| Error::invalid_argument("the binary function has no function handle"))
    }

    /// Creates a rasterization-rate map from a validated descriptor.
    pub fn new_rasterization_rate_map(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::RasterizationRateMapDescriptor,
    ) -> Result<RasterizationRateMap, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newRasterizationRateMapWithDescriptor:))
        {
            return Err(Error::unsupported(
                "rasterization-rate maps are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLRasterizationRateMapDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid rasterization-rate-map descriptor object")
            })?;
        let inner = self
            .inner
            .newRasterizationRateMapWithDescriptor(descriptor)
            .ok_or_else(|| {
                Error::invalid_argument("Metal rejected the rasterization-rate-map descriptor")
            })?;
        // SAFETY: the typed factory returned an owned MTLRasterizationRateMap.
        Ok(RasterizationRateMap::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Allocates an acceleration structure with a checked non-zero size.
    pub fn new_acceleration_structure(&self, size: usize) -> Result<AccelerationStructure, Error> {
        if size == 0 {
            return Err(Error::invalid_argument(
                "acceleration-structure size must be non-zero",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newAccelerationStructureWithSize:))
        {
            return Err(Error::unsupported(
                "acceleration structures are unavailable",
            ));
        }
        let inner = self
            .inner
            .newAccelerationStructureWithSize(size)
            .ok_or_else(|| {
                Error::unsupported("Metal could not allocate the acceleration structure")
            })?;
        // SAFETY: the typed factory returned an owned MTLAccelerationStructure.
        Ok(AccelerationStructure::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Allocates an acceleration structure from a validated descriptor.
    pub fn new_acceleration_structure_from_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::AccelerationStructureDescriptor,
    ) -> Result<AccelerationStructure, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newAccelerationStructureWithDescriptor:))
        {
            return Err(Error::unsupported(
                "acceleration structures are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLAccelerationStructureDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid acceleration-structure descriptor object")
            })?;
        let inner = self
            .inner
            .newAccelerationStructureWithDescriptor(descriptor)
            .ok_or_else(|| {
                Error::invalid_argument("Metal rejected the acceleration-structure descriptor")
            })?;
        // SAFETY: the typed factory returned an owned MTLAccelerationStructure.
        Ok(AccelerationStructure::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Builds a render pipeline from a validated descriptor.
    pub fn new_render_pipeline(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<RenderPipelineState, Error> {
        self.inner
            .newRenderPipelineStateWithDescriptor_error(&descriptor.inner)
            .map(RenderPipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous render pipeline compilation.
    pub fn new_render_pipeline_async(
        &self,
        descriptor: &RenderPipelineDescriptor,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithDescriptor:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous render pipeline compilation is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_object(value, error).map(|inner| {
                // SAFETY: this selector returns MTLRenderPipelineState.
                RenderPipelineState::new(unsafe { Retained::cast_unchecked(inner) })
            });
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; descriptor remains alive
        // for the call and the block has the documented state/error ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newRenderPipelineStateWithDescriptor: &*descriptor.inner,
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a render pipeline and returns owned reflection information.
    pub fn new_render_pipeline_with_reflection(
        &self,
        descriptor: &RenderPipelineDescriptor,
        options: PipelineOption,
    ) -> Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithDescriptor:options:reflection:error:
        )) {
            return Err(Error::unsupported(
                "render pipeline reflection is unavailable",
            ));
        }
        let mut reflection = None;
        let state = self
            .inner
            .newRenderPipelineStateWithDescriptor_options_reflection_error(
                &descriptor.inner,
                MTLPipelineOption(options.as_raw()),
                Some(&mut reflection),
            )
            .map(RenderPipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        let reflection = reflection.map(|inner| {
            // SAFETY: the out parameter is declared as
            // MTLRenderPipelineReflection and is retained by objc2.
            RenderPipelineReflection::from_inner(unsafe { Retained::cast_unchecked(inner) })
        });
        Ok((state, reflection))
    }

    /// Starts render pipeline compilation with owned reflection delivery.
    pub fn new_render_pipeline_with_reflection_async(
        &self,
        descriptor: &RenderPipelineDescriptor,
        options: PipelineOption,
        handler: impl FnOnce(Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error>)
        + Send
        + 'static,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithDescriptor:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous render reflection is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(
            move |value: *mut AnyObject, reflection: *mut AnyObject, error: *mut NSError| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                let result = callback_object(value, error).map(|inner| {
                    // SAFETY: this selector returns MTLRenderPipelineState.
                    let state =
                        RenderPipelineState::new(unsafe { Retained::cast_unchecked(inner) });
                    let reflection = callback_optional_object(reflection)
                        .map(RenderPipelineReflection::from_inner);
                    (state, reflection)
                });
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector and option bits were checked; descriptor is valid
        // and the block matches state/reflection/error exactly.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newRenderPipelineStateWithDescriptor: &*descriptor.inner,
                options: MTLPipelineOption(options.as_raw()),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a compute pipeline from a named library function.
    pub fn new_compute_pipeline(
        &self,
        function: &crate::metal::Function,
    ) -> Result<ComputePipelineState, Error> {
        self.inner
            .newComputePipelineStateWithFunction_error(&function.inner)
            .map(ComputePipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous compute pipeline compilation.
    pub fn new_compute_pipeline_async(
        &self,
        function: &crate::metal::Function,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithFunction:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous compute pipeline compilation is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_object(value, error).map(|inner| {
                // SAFETY: this selector returns MTLComputePipelineState.
                ComputePipelineState::new(unsafe { Retained::cast_unchecked(inner) })
            });
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability and the MTLFunction wrapper invariant
        // were established; Metal copies the exact state/error block.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newComputePipelineStateWithFunction: &*function.inner,
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Starts compute pipeline compilation with owned reflection delivery.
    pub fn new_compute_pipeline_with_reflection_async(
        &self,
        function: &crate::metal::Function,
        options: PipelineOption,
        handler: impl FnOnce(Result<(ComputePipelineState, Option<ComputePipelineReflection>), Error>)
        + Send
        + 'static,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithFunction:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous compute reflection is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(
            move |value: *mut AnyObject, reflection: *mut AnyObject, error: *mut NSError| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                let result = callback_object(value, error).map(|inner| {
                    // SAFETY: this selector returns MTLComputePipelineState.
                    let state =
                        ComputePipelineState::new(unsafe { Retained::cast_unchecked(inner) });
                    let reflection = callback_optional_object(reflection)
                        .map(ComputePipelineReflection::from_inner);
                    (state, reflection)
                });
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector and option bits were checked, Function owns a valid
        // MTLFunction, and the block exactly matches state/reflection/error.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newComputePipelineStateWithFunction: &*function.inner,
                options: MTLPipelineOption(options.as_raw()),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a compute pipeline and returns owned reflection information.
    pub fn new_compute_pipeline_with_reflection(
        &self,
        function: &crate::metal::Function,
        options: PipelineOption,
    ) -> Result<(ComputePipelineState, Option<ComputePipelineReflection>), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithFunction:options:reflection:error:
        )) {
            return Err(Error::unsupported(
                "compute pipeline reflection is unavailable",
            ));
        }
        let mut reflection = None;
        // SAFETY: Function privately owns a valid MTLFunction object; option
        // bits are validated and objc2 owns the optional out parameter.
        let state = unsafe {
            self.inner
                .newComputePipelineStateWithFunction_options_reflection_error(
                    &function.inner,
                    MTLPipelineOption(options.as_raw()),
                    Some(&mut reflection),
                )
        }
        .map(ComputePipelineState::new)
        .map_err(|error| crate::foundation::metal_error(&error))?;
        let reflection = reflection.map(|inner| {
            // SAFETY: the out parameter is declared as
            // MTLComputePipelineReflection and is retained by objc2.
            ComputePipelineReflection::from_inner(unsafe { Retained::cast_unchecked(inner) })
        });
        Ok((state, reflection))
    }

    /// Builds a compute pipeline from a generated descriptor with reflection.
    pub fn new_compute_pipeline_from_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::ComputePipelineDescriptor,
        options: PipelineOption,
    ) -> Result<(ComputePipelineState, Option<ComputePipelineReflection>), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithDescriptor:options:reflection:error:
        )) {
            return Err(Error::unsupported(
                "descriptor compute pipeline compilation is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLComputePipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid compute pipeline descriptor object"))?;
        let mut reflection = None;
        let state = self
            .inner
            .newComputePipelineStateWithDescriptor_options_reflection_error(
                descriptor,
                MTLPipelineOption(options.as_raw()),
                Some(&mut reflection),
            )
            .map(ComputePipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        let reflection = reflection.map(|inner| {
            // SAFETY: the out parameter is a retained compute reflection.
            ComputePipelineReflection::from_inner(unsafe { Retained::cast_unchecked(inner) })
        });
        Ok((state, reflection))
    }

    /// Starts descriptor-based compute compilation with reflection delivery.
    pub fn new_compute_pipeline_from_descriptor_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::ComputePipelineDescriptor,
        options: PipelineOption,
        handler: impl FnOnce(Result<(ComputePipelineState, Option<ComputePipelineReflection>), Error>)
        + Send
        + 'static,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithDescriptor:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous descriptor compute compilation is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLComputePipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid compute pipeline descriptor object"))?;
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(
            move |value: *mut AnyObject, reflection: *mut AnyObject, error: *mut NSError| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                let result = callback_object(value, error).map(|inner| {
                    // SAFETY: this selector returns MTLComputePipelineState.
                    let state =
                        ComputePipelineState::new(unsafe { Retained::cast_unchecked(inner) });
                    let reflection = callback_optional_object(reflection)
                        .map(ComputePipelineReflection::from_inner);
                    (state, reflection)
                });
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector, descriptor class and options were checked; the
        // block exactly matches the documented three-object completion ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newComputePipelineStateWithDescriptor: descriptor,
                options: MTLPipelineOption(options.as_raw()),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a tile render pipeline with reflection.
    pub fn new_tile_render_pipeline_with_reflection(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::TileRenderPipelineDescriptor,
        options: PipelineOption,
    ) -> Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithTileDescriptor:options:reflection:error:
        )) {
            return Err(Error::unsupported("tile render pipelines are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLTileRenderPipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid tile pipeline descriptor object"))?;
        let mut reflection = None;
        let state = self
            .inner
            .newRenderPipelineStateWithTileDescriptor_options_reflection_error(
                descriptor,
                MTLPipelineOption(options.as_raw()),
                Some(&mut reflection),
            )
            .map(RenderPipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        let reflection = reflection.map(|inner| {
            // SAFETY: the out parameter is a retained render reflection.
            RenderPipelineReflection::from_inner(unsafe { Retained::cast_unchecked(inner) })
        });
        Ok((state, reflection))
    }

    /// Starts tile pipeline compilation with owned reflection delivery.
    pub fn new_tile_render_pipeline_with_reflection_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::TileRenderPipelineDescriptor,
        options: PipelineOption,
        handler: impl FnOnce(Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error>)
        + Send
        + 'static,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithTileDescriptor:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous tile pipeline compilation is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLTileRenderPipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid tile pipeline descriptor object"))?;
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(
            move |value: *mut AnyObject, reflection: *mut AnyObject, error: *mut NSError| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else { return };
                let result = callback_object(value, error).map(|inner| {
                    // SAFETY: this selector returns MTLRenderPipelineState.
                    let state =
                        RenderPipelineState::new(unsafe { Retained::cast_unchecked(inner) });
                    let reflection = callback_optional_object(reflection)
                        .map(RenderPipelineReflection::from_inner);
                    (state, reflection)
                });
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector, class and options were checked and the block has
        // the exact documented state/reflection/error ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newRenderPipelineStateWithTileDescriptor: descriptor,
                options: MTLPipelineOption(options.as_raw()),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Builds a mesh render pipeline with reflection.
    pub fn new_mesh_render_pipeline_with_reflection(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::MeshRenderPipelineDescriptor,
        options: PipelineOption,
    ) -> Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithMeshDescriptor:options:reflection:error:
        )) {
            return Err(Error::unsupported("mesh render pipelines are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLMeshRenderPipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid mesh pipeline descriptor object"))?;
        let mut reflection = None;
        let state = self
            .inner
            .newRenderPipelineStateWithMeshDescriptor_options_reflection_error(
                descriptor,
                MTLPipelineOption(options.as_raw()),
                Some(&mut reflection),
            )
            .map(RenderPipelineState::new)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        let reflection = reflection.map(|inner| {
            // SAFETY: the out parameter is a retained render reflection.
            RenderPipelineReflection::from_inner(unsafe { Retained::cast_unchecked(inner) })
        });
        Ok((state, reflection))
    }

    /// Starts mesh pipeline compilation with owned reflection delivery.
    pub fn new_mesh_render_pipeline_with_reflection_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::MeshRenderPipelineDescriptor,
        options: PipelineOption,
        handler: impl FnOnce(Result<(RenderPipelineState, Option<RenderPipelineReflection>), Error>)
        + Send
        + 'static,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument("invalid pipeline option bits"));
        }
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithMeshDescriptor:options:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous mesh pipeline compilation is unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLMeshRenderPipelineDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid mesh pipeline descriptor object"))?;
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(
            move |value: *mut AnyObject, reflection: *mut AnyObject, error: *mut NSError| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else { return };
                let result = callback_object(value, error).map(|inner| {
                    // SAFETY: this selector returns MTLRenderPipelineState.
                    let state =
                        RenderPipelineState::new(unsafe { Retained::cast_unchecked(inner) });
                    let reflection = callback_optional_object(reflection)
                        .map(RenderPipelineReflection::from_inner);
                    (state, reflection)
                });
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector, class and options were checked and the block has
        // the exact documented state/reflection/error ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newRenderPipelineStateWithMeshDescriptor: descriptor,
                options: MTLPipelineOption(options.as_raw()),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Returns whether a texture sample count is supported by the device.
    #[must_use]
    pub fn supports_texture_sample_count(&self, sample_count: usize) -> bool {
        sample_count > 0 && self.inner.supportsTextureSampleCount(sample_count)
    }

    /// Creates a residency set and preserves NSError diagnostics.
    pub fn new_residency_set(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::ResidencySetDescriptor,
    ) -> Result<ResidencySet, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newResidencySetWithDescriptor:error:))
        {
            return Err(Error::unsupported("Metal residency sets are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLResidencySetDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid residency set descriptor"))?;
        let inner = self
            .inner
            .newResidencySetWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLResidencySet.
        Ok(ResidencySet::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Returns the required size and alignment for a checked tensor descriptor.
    pub fn tensor_size_and_align(
        &self,
        descriptor: &CheckedTensorDescriptor,
    ) -> Result<SizeAndAlign, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(tensorSizeAndAlignWithDescriptor:))
        {
            return Err(Error::unsupported("Metal tensor sizing is unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLTensorDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid checked tensor descriptor"))?;
        let value = self.inner.tensorSizeAndAlignWithDescriptor(descriptor);
        if value.size == 0 || value.align == 0 {
            return Err(Error::invalid_argument(
                "Metal rejected the tensor descriptor for allocation",
            ));
        }
        Ok(SizeAndAlign {
            size: value.size,
            align: value.align,
        })
    }

    /// Allocates a tensor from a fully checked descriptor.
    pub fn new_tensor(&self, descriptor: &CheckedTensorDescriptor) -> Result<Tensor, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newTensorWithDescriptor:error:))
        {
            return Err(Error::unsupported("Metal tensors are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLTensorDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid checked tensor descriptor"))?;
        let inner = self
            .inner
            .newTensorWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLTensor object.
        Ok(Tensor::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a tensor backed by checked, same-device buffer attachments.
    pub fn new_tensor_with_attachments(
        &self,
        descriptor: &CheckedTensorDescriptor,
        attachments: &CheckedTensorBufferAttachments,
    ) -> Result<Tensor, Error> {
        if !self.inner.respondsToSelector(sel!(
            newTensorWithDescriptor:bufferAttachments:error:
        )) {
            return Err(Error::unsupported(
                "buffer-attached Metal tensors are unavailable",
            ));
        }
        if attachments
            .device()
            .is_some_and(|device| !std::ptr::eq(device, self.as_any_object()))
        {
            return Err(Error::invalid_argument(
                "tensor attachments belong to a different device",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLTensorDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid checked tensor descriptor"))?;
        let attachments = attachments.as_inner();
        // SAFETY: selector availability, descriptor and attachment classes,
        // checked buffer ranges, and same-device identity were established.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_any_object(),
                newTensorWithDescriptor: descriptor,
                bufferAttachments: attachments,
                error: _
            ]
        };
        result
            .map(Tensor::from_inner)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Creates a texture view pool from a validated descriptor.
    pub fn new_texture_view_pool(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::ResourceViewPoolDescriptor,
    ) -> Result<TextureViewPool, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newTextureViewPoolWithDescriptor:error:))
        {
            return Err(Error::unsupported("texture view pools are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTLResourceViewPoolDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid resource view pool descriptor"))?;
        let inner = self
            .inner
            .newTextureViewPoolWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTLTextureViewPool.
        Ok(TextureViewPool::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a Metal 4 timestamp counter heap after capability checks.
    pub fn new_timestamp_counter_heap(&self, count: usize) -> Result<TimestampCounterHeap, Error> {
        if !(1..=MAX_TIMESTAMP_COUNTERS).contains(&count) {
            return Err(Error::unsupported(format!(
                "timestamp counter count must be between 1 and {MAX_TIMESTAMP_COUNTERS}"
            )));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newCounterHeapWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "MTL4CounterHeap requires macOS 26 or newer",
            ));
        }

        let descriptor = MTL4CounterHeapDescriptor::new();
        descriptor.setType(MTL4CounterHeapType::Timestamp);
        // SAFETY: The public facade restricts count to the range validated by
        // Metal's current timestamp CounterHeap implementation.
        unsafe { descriptor.setCount(count) };
        self.inner
            .newCounterHeapWithDescriptor_error(&descriptor)
            .map(|inner| TimestampCounterHeap::new(inner, count))
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Loads a Metal 4 archive from a UTF-8 filesystem path.
    pub fn new_mtl4_archive_from_path(&self, path: &str) -> Result<Mtl4Archive, Error> {
        if path.is_empty() || path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("Metal 4 archive path is invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newArchiveWithURL:error:))
        {
            return Err(Error::unsupported("Metal 4 archives are unavailable"));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let inner = self
            .inner
            .newArchiveWithURL_error(&url)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTL4Archive object.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Mtl4Archive::from_generated(
            crate::metal::generated_object_types::metal4::Archive::from_inner(inner),
        ))
    }

    /// Creates a default Metal 4 command allocator.
    pub fn new_mtl4_command_allocator(&self) -> Result<Metal4CommandAllocator, Error> {
        if !self.inner.respondsToSelector(sel!(newCommandAllocator)) {
            return Err(Error::unsupported(
                "Metal 4 command allocators are unavailable",
            ));
        }
        let inner = self.inner.newCommandAllocator().ok_or_else(|| {
            Error::unsupported("Metal could not create a Metal 4 command allocator")
        })?;
        // SAFETY: the typed factory returned an owned MTL4CommandAllocator.
        Ok(Metal4CommandAllocator::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a Metal 4 command allocator from a descriptor.
    pub fn new_mtl4_command_allocator_from_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal4::CommandAllocatorDescriptor,
    ) -> Result<Metal4CommandAllocator, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newCommandAllocatorWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "descriptor-based Metal 4 command allocators are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTL4CommandAllocatorDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid Metal 4 command allocator descriptor")
            })?;
        let inner = self
            .inner
            .newCommandAllocatorWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTL4CommandAllocator.
        Ok(Metal4CommandAllocator::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a reusable Metal 4 command queue from a descriptor.
    pub fn new_mtl4_command_queue_from_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal4::CommandQueueDescriptor,
    ) -> Result<Metal4CommandQueue, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newMTL4CommandQueueWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "descriptor-based Metal 4 command queues are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTL4CommandQueueDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid Metal 4 command queue descriptor"))?;
        let inner = self
            .inner
            .newMTL4CommandQueueWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTL4CommandQueue.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Metal4CommandQueue::from_generated(
            crate::metal::generated_object_types::metal4::CommandQueue::from_inner(inner),
        ))
    }

    /// Creates a Metal 4 compiler from a descriptor.
    pub fn new_mtl4_compiler(
        &self,
        descriptor: &crate::metal::generated_object_types::metal4::CompilerDescriptor,
    ) -> Result<Metal4Compiler, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newCompilerWithDescriptor:error:))
        {
            return Err(Error::unsupported("Metal 4 compilers are unavailable"));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTL4CompilerDescriptor>()
            .ok_or_else(|| Error::invalid_argument("invalid Metal 4 compiler descriptor"))?;
        let inner = self
            .inner
            .newCompilerWithDescriptor_error(descriptor)
            .map_err(|error| crate::foundation::metal_error(&error))?;
        // SAFETY: the typed factory returned an owned MTL4Compiler.
        Ok(Metal4Compiler::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Creates a Metal 4 pipeline data-set serializer.
    pub fn new_pipeline_data_set_serializer(
        &self,
        descriptor: &crate::metal::generated_object_types::metal4::PipelineDataSetSerializerDescriptor,
    ) -> Result<PipelineDataSetSerializer, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newPipelineDataSetSerializerWithDescriptor:))
        {
            return Err(Error::unsupported(
                "pipeline data-set serializers are unavailable",
            ));
        }
        let descriptor = descriptor
            .as_inner()
            .downcast_ref::<MTL4PipelineDataSetSerializerDescriptor>()
            .ok_or_else(|| {
                Error::invalid_argument("invalid pipeline data-set serializer descriptor")
            })?;
        let inner = self
            .inner
            .newPipelineDataSetSerializerWithDescriptor(descriptor);
        // SAFETY: the typed factory returned an owned serializer object.
        Ok(PipelineDataSetSerializer::from_inner(unsafe {
            Retained::cast_unchecked(inner)
        }))
    }

    /// Returns the byte size of one declared Metal 4 counter-heap entry.
    pub fn size_of_counter_heap_entry(
        &self,
        counter_type: crate::metal::generated_value_types::CounterHeapType,
    ) -> Result<usize, Error> {
        if counter_type.as_raw() == 0 {
            return Err(Error::invalid_argument(
                "counter heap type must not be invalid",
            ));
        }
        if !self.inner.respondsToSelector(sel!(sizeOfCounterHeapEntry:)) {
            return Err(Error::unsupported(
                "Metal 4 counter heap entry sizing is unavailable",
            ));
        }
        Ok(self
            .inner
            .sizeOfCounterHeapEntry(MTL4CounterHeapType(counter_type.as_raw())))
    }
}
