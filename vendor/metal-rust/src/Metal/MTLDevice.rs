use super::{
    Buffer, CommandQueue, CompileOptions, ComputePipelineState, Function, Library,
    RenderPipelineDescriptor, RenderPipelineState, ResourceOptions, Texture, TextureDescriptor,
    TimestampCounterHeap,
};
use crate::Error;

pub use metal_rust_ffi::{DeviceCapability, DeviceNumericProperty};

/// An owned reference to an Apple Metal device.
#[derive(Clone)]
pub struct Device {
    pub(crate) inner: metal_rust_ffi::Device,
}

impl Device {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Device) -> Self {
        Self { inner }
    }

    /// Returns the preferred system Metal device, if one is available.
    #[must_use]
    pub fn system_default() -> Option<Self> {
        metal_rust_ffi::Device::system_default().map(|inner| Self { inner })
    }

    /// Returns the device's user-visible name as an owned Rust string.
    #[must_use]
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// Returns owned architecture information with runtime availability checks.
    pub fn architecture(&self) -> Result<super::Architecture, Error> {
        self.inner
            .architecture()
            .map(super::Architecture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the device's counter sets as owned safe wrappers.
    pub fn counter_sets(&self) -> Result<Vec<super::CounterSet>, Error> {
        self.inner
            .counter_sets()
            .map(|values| {
                values
                    .into_iter()
                    .map(super::CounterSet::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Queries a Boolean capability with runtime availability checks.
    pub fn capability(&self, capability: DeviceCapability) -> Result<bool, Error> {
        self.inner.capability(capability).map_err(Error::from_ffi)
    }

    /// Queries a numeric property without narrowing its system value.
    pub fn numeric_property(&self, property: DeviceNumericProperty) -> Result<u64, Error> {
        self.inner
            .numeric_property(property)
            .map_err(Error::from_ffi)
    }

    /// Returns the maximum threads per threadgroup.
    #[must_use]
    pub fn max_threads_per_threadgroup(&self) -> super::Size {
        self.inner.max_threads_per_threadgroup()
    }

    /// Returns linear-texture and texture-buffer alignments for a format.
    #[must_use]
    pub fn texture_alignments(&self, format: super::PixelFormat) -> (usize, usize) {
        self.inner.texture_alignments(format)
    }

    /// Samples synchronized CPU and GPU timestamps.
    #[must_use]
    pub fn sample_timestamps(&self) -> (u64, u64) {
        self.inner.sample_timestamps()
    }

    /// Returns the physical location category reported by Metal.
    #[must_use]
    pub fn location(&self) -> super::DeviceLocation {
        self.inner.location()
    }

    /// Returns the supported argument-buffer tier.
    #[must_use]
    pub fn argument_buffers_tier(&self) -> super::ArgumentBuffersTier {
        self.inner.argument_buffers_tier()
    }

    /// Returns the supported read/write texture tier.
    #[must_use]
    pub fn read_write_texture_tier(&self) -> super::ReadWriteTextureTier {
        self.inner.read_write_texture_tier()
    }

    /// Checks support for a declared GPU family.
    pub fn supports_family(&self, family: super::GPUFamily) -> Result<bool, Error> {
        self.inner.supports_family(family).map_err(Error::from_ffi)
    }

    /// Checks support for a declared legacy feature set.
    pub fn supports_feature_set(&self, feature_set: super::FeatureSet) -> Result<bool, Error> {
        self.inner
            .supports_feature_set(feature_set)
            .map_err(Error::from_ffi)
    }

    /// Checks support for a declared counter-sampling point.
    pub fn supports_counter_sampling(
        &self,
        point: super::CounterSamplingPoint,
    ) -> Result<bool, Error> {
        self.inner
            .supports_counter_sampling(point)
            .map_err(Error::from_ffi)
    }

    /// Checks rasterization-rate-map layer-count support.
    pub fn supports_rasterization_rate_map(&self, layer_count: usize) -> Result<bool, Error> {
        self.inner
            .supports_rasterization_rate_map(layer_count)
            .map_err(Error::from_ffi)
    }

    /// Checks vertex-amplification-count support.
    pub fn supports_vertex_amplification_count(&self, count: usize) -> Result<bool, Error> {
        self.inner
            .supports_vertex_amplification_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns Metal's concurrent-compilation policy.
    pub fn should_maximize_concurrent_compilation(&self) -> Result<bool, Error> {
        self.inner
            .should_maximize_concurrent_compilation()
            .map_err(Error::from_ffi)
    }

    /// Sets Metal's concurrent-compilation policy.
    pub fn set_should_maximize_concurrent_compilation(&self, value: bool) -> Result<(), Error> {
        self.inner
            .set_should_maximize_concurrent_compilation(value)
            .map_err(Error::from_ffi)
    }

    /// Returns default programmable sample positions as owned Rust values.
    pub fn default_sample_positions(
        &self,
        count: usize,
    ) -> Result<Vec<super::SamplePosition>, Error> {
        self.inner
            .default_sample_positions(count)
            .map_err(Error::from_ffi)
    }

    /// Returns heap size and alignment for a buffer allocation.
    pub fn heap_buffer_size_and_align(
        &self,
        length: usize,
        options: ResourceOptions,
    ) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .heap_buffer_size_and_align(length, options)
            .map_err(Error::from_ffi)
    }

    /// Returns heap size and alignment for a texture descriptor.
    pub fn heap_texture_size_and_align(
        &self,
        descriptor: &TextureDescriptor,
    ) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .heap_texture_size_and_align(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns heap size and alignment for an acceleration structure size.
    pub fn heap_acceleration_structure_size_and_align(
        &self,
        size: usize,
    ) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .heap_acceleration_structure_size_and_align(size)
            .map_err(Error::from_ffi)
    }

    /// Returns build and scratch sizes for an acceleration-structure descriptor.
    pub fn acceleration_structure_sizes(
        &self,
        descriptor: &super::AccelerationStructureDescriptor,
    ) -> Result<super::AccelerationStructureSizes, Error> {
        self.inner
            .acceleration_structure_sizes(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns heap size and alignment for an acceleration-structure descriptor.
    pub fn heap_acceleration_structure_descriptor_size_and_align(
        &self,
        descriptor: &super::AccelerationStructureDescriptor,
    ) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .heap_acceleration_structure_descriptor_size_and_align(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns sparse tile dimensions for a texture configuration.
    pub fn sparse_tile_size(
        &self,
        texture_type: super::TextureType,
        pixel_format: super::PixelFormat,
        sample_count: usize,
        page_size: Option<super::SparsePageSize>,
    ) -> Result<super::Size, Error> {
        self.inner
            .sparse_tile_size(texture_type, pixel_format, sample_count, page_size)
            .map_err(Error::from_ffi)
    }

    /// Converts sparse pixel regions to tile regions without exposing pointers.
    pub fn sparse_pixel_regions_to_tiles(
        &self,
        pixel_regions: &[super::Region],
        tile_size: super::Size,
        mode: super::SparseTextureRegionAlignmentMode,
    ) -> Result<Vec<super::Region>, Error> {
        self.inner
            .sparse_pixel_regions_to_tiles(pixel_regions, tile_size, mode)
            .map_err(Error::from_ffi)
    }

    /// Converts sparse tile regions to pixel regions without exposing pointers.
    pub fn sparse_tile_regions_to_pixels(
        &self,
        tile_regions: &[super::Region],
        tile_size: super::Size,
    ) -> Result<Vec<super::Region>, Error> {
        self.inner
            .sparse_tile_regions_to_pixels(tile_regions, tile_size)
            .map_err(Error::from_ffi)
    }

    /// Creates a command queue, optionally with a checked upper bound.
    pub fn new_command_queue(
        &self,
        max_command_buffers: Option<usize>,
    ) -> Result<CommandQueue, Error> {
        self.inner
            .new_command_queue(max_command_buffers)
            .map(CommandQueue::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a command queue from a descriptor.
    pub fn new_command_queue_from_descriptor(
        &self,
        descriptor: &super::CommandQueueDescriptor,
    ) -> Result<CommandQueue, Error> {
        self.inner
            .new_command_queue_from_descriptor(&descriptor.inner)
            .map(CommandQueue::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an argument encoder from a non-empty descriptor slice.
    pub fn new_argument_encoder(
        &self,
        arguments: &[super::ArgumentDescriptor],
    ) -> Result<super::ArgumentEncoder, Error> {
        let arguments: Vec<_> = arguments.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .new_argument_encoder(&arguments)
            .map(super::ArgumentEncoder::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an argument encoder from a reflected buffer binding.
    pub fn new_argument_encoder_from_buffer_binding(
        &self,
        buffer_binding: &super::BufferBinding,
    ) -> Result<super::ArgumentEncoder, Error> {
        self.inner
            .new_argument_encoder_from_buffer_binding(&buffer_binding.inner)
            .map(super::ArgumentEncoder::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates a buffer with the selected resource options.
    pub fn new_buffer(&self, length: usize, options: ResourceOptions) -> Result<Buffer, Error> {
        self.inner
            .new_buffer(length, options)
            .map(Buffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates a buffer and synchronously copies Rust bytes into it.
    pub fn new_buffer_with_bytes(
        &self,
        bytes: &[u8],
        options: ResourceOptions,
    ) -> Result<Buffer, Error> {
        self.inner
            .new_buffer_with_bytes(bytes, options)
            .map(Buffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates a placement-sparse buffer after capability and size checks.
    pub fn new_placement_sparse_buffer(
        &self,
        length: usize,
        options: ResourceOptions,
        page_size: super::SparsePageSize,
    ) -> Result<Buffer, Error> {
        self.inner
            .new_placement_sparse_buffer(length, options, page_size)
            .map(Buffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates a texture from a validated descriptor.
    pub fn new_texture(&self, descriptor: &TextureDescriptor) -> Result<Texture, Error> {
        self.inner
            .new_texture(&descriptor.inner)
            .map(Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a texture backed by an owned IOSurface identity.
    pub fn new_texture_from_iosurface(
        &self,
        descriptor: &TextureDescriptor,
        iosurface: &super::IoSurface,
        plane: usize,
    ) -> Result<Texture, Error> {
        self.inner
            .new_texture_from_iosurface(&descriptor.inner, iosurface.as_ffi(), plane)
            .map(Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates a cross-process shareable texture.
    pub fn new_shared_texture(&self, descriptor: &TextureDescriptor) -> Result<Texture, Error> {
        self.inner
            .new_shared_texture(&descriptor.inner)
            .map(Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Recreates a cross-process texture from a shared handle.
    pub fn new_shared_texture_from_handle(
        &self,
        handle: &super::SharedTextureHandle,
    ) -> Result<Texture, Error> {
        self.inner
            .new_shared_texture_from_handle(&handle.inner)
            .map(Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an indirect command buffer with a checked command count.
    pub fn new_indirect_command_buffer(
        &self,
        descriptor: &super::IndirectCommandBufferDescriptor,
        max_count: usize,
        options: ResourceOptions,
    ) -> Result<super::IndirectCommandBuffer, Error> {
        self.inner
            .new_indirect_command_buffer(&descriptor.inner, max_count, options)
            .map(super::IndirectCommandBuffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates the default library from the application's main bundle.
    pub fn new_default_library(&self) -> Result<Library, Error> {
        self.inner
            .new_default_library()
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates the default Metal library from a retained Foundation bundle.
    pub fn new_default_library_from_bundle(
        &self,
        bundle: &crate::foundation::Bundle,
    ) -> Result<Library, Error> {
        self.inner
            .new_default_library_from_bundle(bundle.as_ffi())
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Loads a Metal library from a UTF-8 filesystem path.
    pub fn new_library_from_path(&self, path: &str) -> Result<Library, Error> {
        self.inner
            .new_library_from_path(path)
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Loads a Metal library from bytes copied into private dispatch storage.
    pub fn new_library_from_bytes(&self, bytes: &[u8]) -> Result<Library, Error> {
        self.inner
            .new_library_from_bytes(bytes)
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a depth/stencil state from a descriptor.
    pub fn new_depth_stencil_state(
        &self,
        descriptor: &super::DepthStencilDescriptor,
    ) -> Result<super::DepthStencilState, Error> {
        self.inner
            .new_depth_stencil_state(&descriptor.inner)
            .map(super::DepthStencilState::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a heap from a descriptor.
    pub fn new_heap(&self, descriptor: &super::HeapDescriptor) -> Result<super::Heap, Error> {
        self.inner
            .new_heap(&descriptor.inner)
            .map(super::Heap::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a sampler state from a descriptor.
    pub fn new_sampler_state(
        &self,
        descriptor: &super::SamplerDescriptor,
    ) -> Result<super::SamplerState, Error> {
        self.inner
            .new_sampler_state(&descriptor.inner)
            .map(super::SamplerState::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a single-device event.
    pub fn new_event(&self) -> Result<super::Event, Error> {
        self.inner
            .new_event()
            .map(super::Event::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a single-device fence.
    pub fn new_fence(&self) -> Result<super::Fence, Error> {
        self.inner
            .new_fence()
            .map(super::Fence::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a cross-device shareable event.
    pub fn new_shared_event(&self) -> Result<super::SharedEvent, Error> {
        self.inner
            .new_shared_event()
            .map(super::SharedEvent::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Recreates a shareable event from an owned handle.
    pub fn new_shared_event_from_handle(
        &self,
        handle: &super::SharedEventHandle,
    ) -> Result<super::SharedEvent, Error> {
        self.inner
            .new_shared_event_from_handle(&handle.inner)
            .map(super::SharedEvent::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Compiles Metal Shading Language source.
    pub fn new_library_from_source(
        &self,
        source: &str,
        options: Option<&CompileOptions>,
    ) -> Result<Library, Error> {
        self.inner
            .new_library_from_source(source, options.map(|value| &value.inner))
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous Metal source compilation.
    pub fn new_library_from_source_async(
        &self,
        source: &str,
        options: Option<&CompileOptions>,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_library_from_source_async(
                source,
                options.map(|value| &value.inner),
                move |result| handler(result.map(Library::from_ffi).map_err(Error::from_ffi)),
            )
            .map_err(Error::from_ffi)
    }

    /// Builds a stitched library from a descriptor.
    pub fn new_stitched_library(
        &self,
        descriptor: &super::StitchedLibraryDescriptor,
    ) -> Result<Library, Error> {
        self.inner
            .new_stitched_library(&descriptor.inner)
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous stitched-library compilation.
    pub fn new_stitched_library_async(
        &self,
        descriptor: &super::StitchedLibraryDescriptor,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_stitched_library_async(&descriptor.inner, move |result| {
                handler(result.map(Library::from_ffi).map_err(Error::from_ffi))
            })
            .map_err(Error::from_ffi)
    }

    /// Creates a binary archive and preserves Metal diagnostics.
    pub fn new_binary_archive(
        &self,
        descriptor: &super::BinaryArchiveDescriptor,
    ) -> Result<super::BinaryArchive, Error> {
        self.inner
            .new_binary_archive(&descriptor.inner)
            .map(super::BinaryArchive::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a counter sample buffer and preserves Metal diagnostics.
    pub fn new_counter_sample_buffer(
        &self,
        descriptor: &super::CounterSampleBufferDescriptor,
    ) -> Result<super::CounterSampleBuffer, Error> {
        self.inner
            .new_counter_sample_buffer(&descriptor.inner)
            .map(super::CounterSampleBuffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a Metal log state and preserves NSError diagnostics.
    pub fn new_log_state(
        &self,
        descriptor: &super::LogStateDescriptor,
    ) -> Result<super::LogState, Error> {
        self.inner
            .new_log_state(&descriptor.inner)
            .map(super::LogState::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a dynamic library from a compiled Metal library.
    pub fn new_dynamic_library(&self, library: &Library) -> Result<super::DynamicLibrary, Error> {
        self.inner
            .new_dynamic_library(&library.inner)
            .map(super::DynamicLibrary::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Loads a dynamic Metal library from a UTF-8 filesystem path.
    pub fn new_dynamic_library_from_path(
        &self,
        path: &str,
    ) -> Result<super::DynamicLibrary, Error> {
        self.inner
            .new_dynamic_library_from_path(path)
            .map(super::DynamicLibrary::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns a pipeline-independent handle for a compiled function.
    pub fn function_handle(&self, function: &Function) -> Result<super::FunctionHandle, Error> {
        self.inner
            .function_handle(&function.inner)
            .map(super::FunctionHandle::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns a pipeline-independent handle for a Metal 4 binary function.
    pub fn function_handle_from_binary(
        &self,
        function: &crate::metal4::BinaryFunction,
    ) -> Result<super::FunctionHandle, Error> {
        self.inner
            .function_handle_from_binary(&function.inner)
            .map(super::FunctionHandle::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a rasterization-rate map from a descriptor.
    pub fn new_rasterization_rate_map(
        &self,
        descriptor: &super::RasterizationRateMapDescriptor,
    ) -> Result<super::RasterizationRateMap, Error> {
        self.inner
            .new_rasterization_rate_map(&descriptor.inner)
            .map(super::RasterizationRateMap::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates an acceleration structure with a checked size.
    pub fn new_acceleration_structure(
        &self,
        size: usize,
    ) -> Result<super::AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure(size)
            .map(super::AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Allocates an acceleration structure from a descriptor.
    pub fn new_acceleration_structure_from_descriptor(
        &self,
        descriptor: &super::AccelerationStructureDescriptor,
    ) -> Result<super::AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure_from_descriptor(&descriptor.inner)
            .map(super::AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Builds a render pipeline.
    pub fn new_render_pipeline(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<RenderPipelineState, Error> {
        self.inner
            .new_render_pipeline(&descriptor.inner)
            .map(|inner| RenderPipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous render pipeline compilation.
    pub fn new_render_pipeline_async(
        &self,
        descriptor: &RenderPipelineDescriptor,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_render_pipeline_async(&descriptor.inner, move |result| {
                handler(
                    result
                        .map(|inner| RenderPipelineState { inner })
                        .map_err(Error::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Builds a render pipeline and returns owned reflection information.
    pub fn new_render_pipeline_with_reflection(
        &self,
        descriptor: &RenderPipelineDescriptor,
        options: super::PipelineOption,
    ) -> Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error> {
        self.inner
            .new_render_pipeline_with_reflection(&descriptor.inner, options)
            .map(|(state, reflection)| {
                (
                    RenderPipelineState { inner: state },
                    reflection.map(super::RenderPipelineReflection::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Starts render pipeline compilation with owned reflection delivery.
    pub fn new_render_pipeline_with_reflection_async(
        &self,
        descriptor: &RenderPipelineDescriptor,
        options: super::PipelineOption,
        handler: impl FnOnce(
            Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error>,
        ) + Send
        + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_render_pipeline_with_reflection_async(&descriptor.inner, options, move |result| {
                handler(
                    result
                        .map(|(state, reflection)| {
                            (
                                RenderPipelineState { inner: state },
                                reflection.map(super::RenderPipelineReflection::from_ffi),
                            )
                        })
                        .map_err(Error::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Builds a compute pipeline from a library function.
    pub fn new_compute_pipeline(&self, function: &Function) -> Result<ComputePipelineState, Error> {
        self.inner
            .new_compute_pipeline(&function.inner)
            .map(|inner| ComputePipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous compute pipeline compilation.
    pub fn new_compute_pipeline_async(
        &self,
        function: &Function,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_compute_pipeline_async(&function.inner, move |result| {
                handler(
                    result
                        .map(|inner| ComputePipelineState { inner })
                        .map_err(Error::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Builds a compute pipeline and returns owned reflection information.
    pub fn new_compute_pipeline_with_reflection(
        &self,
        function: &Function,
        options: super::PipelineOption,
    ) -> Result<
        (
            ComputePipelineState,
            Option<super::ComputePipelineReflection>,
        ),
        Error,
    > {
        self.inner
            .new_compute_pipeline_with_reflection(&function.inner, options)
            .map(|(state, reflection)| {
                (
                    ComputePipelineState { inner: state },
                    reflection.map(super::ComputePipelineReflection::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Starts compute pipeline compilation with owned reflection delivery.
    pub fn new_compute_pipeline_with_reflection_async(
        &self,
        function: &Function,
        options: super::PipelineOption,
        handler: impl FnOnce(
            Result<
                (
                    ComputePipelineState,
                    Option<super::ComputePipelineReflection>,
                ),
                Error,
            >,
        ) + Send
        + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_compute_pipeline_with_reflection_async(&function.inner, options, move |result| {
                handler(
                    result
                        .map(|(state, reflection)| {
                            (
                                ComputePipelineState { inner: state },
                                reflection.map(super::ComputePipelineReflection::from_ffi),
                            )
                        })
                        .map_err(Error::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Builds a descriptor-based compute pipeline with reflection.
    pub fn new_compute_pipeline_from_descriptor(
        &self,
        descriptor: &super::ComputePipelineDescriptor,
        options: super::PipelineOption,
    ) -> Result<
        (
            ComputePipelineState,
            Option<super::ComputePipelineReflection>,
        ),
        Error,
    > {
        self.inner
            .new_compute_pipeline_from_descriptor(&descriptor.inner, options)
            .map(|(state, reflection)| {
                (
                    ComputePipelineState { inner: state },
                    reflection.map(super::ComputePipelineReflection::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Starts descriptor-based compute compilation with reflection delivery.
    pub fn new_compute_pipeline_from_descriptor_async(
        &self,
        descriptor: &super::ComputePipelineDescriptor,
        options: super::PipelineOption,
        handler: impl FnOnce(
            Result<
                (
                    ComputePipelineState,
                    Option<super::ComputePipelineReflection>,
                ),
                Error,
            >,
        ) + Send
        + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_compute_pipeline_from_descriptor_async(&descriptor.inner, options, move |result| {
                handler(
                    result
                        .map(|(state, reflection)| {
                            (
                                ComputePipelineState { inner: state },
                                reflection.map(super::ComputePipelineReflection::from_ffi),
                            )
                        })
                        .map_err(Error::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Builds a tile render pipeline with reflection.
    pub fn new_tile_render_pipeline_with_reflection(
        &self,
        descriptor: &super::TileRenderPipelineDescriptor,
        options: super::PipelineOption,
    ) -> Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error> {
        self.inner
            .new_tile_render_pipeline_with_reflection(&descriptor.inner, options)
            .map(|(state, reflection)| {
                (
                    RenderPipelineState { inner: state },
                    reflection.map(super::RenderPipelineReflection::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Starts tile pipeline compilation with owned reflection delivery.
    pub fn new_tile_render_pipeline_with_reflection_async(
        &self,
        descriptor: &super::TileRenderPipelineDescriptor,
        options: super::PipelineOption,
        handler: impl FnOnce(
            Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error>,
        ) + Send
        + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_tile_render_pipeline_with_reflection_async(
                &descriptor.inner,
                options,
                move |result| {
                    handler(
                        result
                            .map(|(state, reflection)| {
                                (
                                    RenderPipelineState { inner: state },
                                    reflection.map(super::RenderPipelineReflection::from_ffi),
                                )
                            })
                            .map_err(Error::from_ffi),
                    )
                },
            )
            .map_err(Error::from_ffi)
    }

    /// Builds a mesh render pipeline with reflection.
    pub fn new_mesh_render_pipeline_with_reflection(
        &self,
        descriptor: &super::MeshRenderPipelineDescriptor,
        options: super::PipelineOption,
    ) -> Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error> {
        self.inner
            .new_mesh_render_pipeline_with_reflection(&descriptor.inner, options)
            .map(|(state, reflection)| {
                (
                    RenderPipelineState { inner: state },
                    reflection.map(super::RenderPipelineReflection::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }

    /// Starts mesh pipeline compilation with owned reflection delivery.
    pub fn new_mesh_render_pipeline_with_reflection_async(
        &self,
        descriptor: &super::MeshRenderPipelineDescriptor,
        options: super::PipelineOption,
        handler: impl FnOnce(
            Result<(RenderPipelineState, Option<super::RenderPipelineReflection>), Error>,
        ) + Send
        + 'static,
    ) -> Result<(), Error> {
        self.inner
            .new_mesh_render_pipeline_with_reflection_async(
                &descriptor.inner,
                options,
                move |result| {
                    handler(
                        result
                            .map(|(state, reflection)| {
                                (
                                    RenderPipelineState { inner: state },
                                    reflection.map(super::RenderPipelineReflection::from_ffi),
                                )
                            })
                            .map_err(Error::from_ffi),
                    )
                },
            )
            .map_err(Error::from_ffi)
    }

    /// Returns whether a texture sample count is supported.
    #[must_use]
    pub fn supports_texture_sample_count(&self, sample_count: usize) -> bool {
        self.inner.supports_texture_sample_count(sample_count)
    }

    /// Creates a residency set and preserves NSError diagnostics.
    pub fn new_residency_set(
        &self,
        descriptor: &super::ResidencySetDescriptor,
    ) -> Result<super::ResidencySet, Error> {
        self.inner
            .new_residency_set(&descriptor.inner)
            .map(super::ResidencySet::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the required size and alignment for a checked tensor descriptor.
    pub fn tensor_size_and_align(
        &self,
        descriptor: &super::CheckedTensorDescriptor,
    ) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .tensor_size_and_align(descriptor.as_ffi())
            .map_err(Error::from_ffi)
    }

    /// Allocates a tensor from a checked descriptor.
    pub fn new_tensor(
        &self,
        descriptor: &super::CheckedTensorDescriptor,
    ) -> Result<super::Tensor, Error> {
        self.inner
            .new_tensor(descriptor.as_ffi())
            .map(super::Tensor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a tensor backed by checked, same-device attachments.
    pub fn new_tensor_with_attachments(
        &self,
        descriptor: &super::CheckedTensorDescriptor,
        attachments: &super::CheckedTensorBufferAttachments,
    ) -> Result<super::Tensor, Error> {
        self.inner
            .new_tensor_with_attachments(descriptor.as_ffi(), attachments.as_ffi())
            .map(super::Tensor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a texture view pool from a descriptor.
    pub fn new_texture_view_pool(
        &self,
        descriptor: &super::ResourceViewPoolDescriptor,
    ) -> Result<super::TextureViewPool, Error> {
        self.inner
            .new_texture_view_pool(&descriptor.inner)
            .map(super::TextureViewPool::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a Metal 4 timestamp counter heap.
    ///
    /// The count is validated before the Objective-C boundary is entered.
    pub fn new_timestamp_counter_heap(&self, count: usize) -> Result<TimestampCounterHeap, Error> {
        if count == 0 || count > crate::MAX_TIMESTAMP_COUNTERS {
            return Err(Error::invalid_argument(format!(
                "timestamp counter count must be between 1 and {}",
                crate::MAX_TIMESTAMP_COUNTERS
            )));
        }
        self.inner
            .new_timestamp_counter_heap(count)
            .map(TimestampCounterHeap::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Loads a Metal 4 archive from a UTF-8 filesystem path.
    pub fn new_mtl4_archive_from_path(&self, path: &str) -> Result<crate::metal4::Archive, Error> {
        self.inner
            .new_mtl4_archive_from_path(path)
            .map(crate::metal4::Archive::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a default Metal 4 command allocator.
    pub fn new_mtl4_command_allocator(&self) -> Result<crate::metal4::CommandAllocator, Error> {
        self.inner
            .new_mtl4_command_allocator()
            .map(crate::metal4::CommandAllocator::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a Metal 4 command allocator from a descriptor.
    pub fn new_mtl4_command_allocator_from_descriptor(
        &self,
        descriptor: &crate::metal4::CommandAllocatorDescriptor,
    ) -> Result<crate::metal4::CommandAllocator, Error> {
        self.inner
            .new_mtl4_command_allocator_from_descriptor(&descriptor.inner)
            .map(crate::metal4::CommandAllocator::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a reusable Metal 4 command queue from a descriptor.
    pub fn new_mtl4_command_queue_from_descriptor(
        &self,
        descriptor: &crate::metal4::CommandQueueDescriptor,
    ) -> Result<crate::metal4::CommandQueue, Error> {
        self.inner
            .new_mtl4_command_queue_from_descriptor(&descriptor.inner)
            .map(crate::metal4::CommandQueue::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a Metal 4 compiler from a descriptor.
    pub fn new_mtl4_compiler(
        &self,
        descriptor: &crate::metal4::CompilerDescriptor,
    ) -> Result<crate::metal4::Compiler, Error> {
        self.inner
            .new_mtl4_compiler(&descriptor.inner)
            .map(crate::metal4::Compiler::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a Metal 4 pipeline data-set serializer.
    pub fn new_pipeline_data_set_serializer(
        &self,
        descriptor: &crate::metal4::PipelineDataSetSerializerDescriptor,
    ) -> Result<crate::metal4::PipelineDataSetSerializer, Error> {
        self.inner
            .new_pipeline_data_set_serializer(&descriptor.inner)
            .map(crate::metal4::PipelineDataSetSerializer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the byte size of one declared Metal 4 counter-heap entry.
    pub fn size_of_counter_heap_entry(
        &self,
        counter_type: crate::metal4::CounterHeapType,
    ) -> Result<usize, Error> {
        self.inner
            .size_of_counter_heap_entry(counter_type)
            .map_err(Error::from_ffi)
    }
}
