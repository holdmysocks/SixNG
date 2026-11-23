//! Safe heap, residency-set, and rasterization-rate extensions.

use super::{
    AccelerationStructure, Allocation, Buffer, Heap, RasterizationRateLayerDescriptor,
    RasterizationRateMap, RasterizationRateMapDescriptor, ResidencySet, SamplePosition, Size,
};
use crate::Error;

impl RasterizationRateLayerDescriptor {
    /// Creates a layer descriptor with checked horizontal and vertical sample counts.
    pub fn with_sample_count(sample_count: Size) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::RasterizationRateLayerDescriptor::with_sample_count(
            sample_count,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Creates a layer descriptor by copying checked Rust sample slices.
    pub fn with_samples(horizontal: &[f32], vertical: &[f32]) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::RasterizationRateLayerDescriptor::with_samples(
            horizontal, vertical,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Returns the active sample count.
    pub fn sample_count(&self) -> Result<Size, Error> {
        self.inner.sample_count().map_err(Error::from_ffi)
    }

    /// Returns the maximum supported sample count for this descriptor.
    pub fn max_sample_count(&self) -> Result<Size, Error> {
        self.inner.max_sample_count().map_err(Error::from_ffi)
    }

    /// Changes the active sample count within the descriptor's allocation.
    pub fn set_sample_count(&self, sample_count: Size) -> Result<(), Error> {
        self.inner
            .set_sample_count(sample_count)
            .map_err(Error::from_ffi)
    }

    /// Copies horizontal samples into an owned Rust vector.
    pub fn horizontal_samples(&self) -> Result<Vec<f32>, Error> {
        self.inner.horizontal_samples().map_err(Error::from_ffi)
    }

    /// Copies vertical samples into an owned Rust vector.
    pub fn vertical_samples(&self) -> Result<Vec<f32>, Error> {
        self.inner.vertical_samples().map_err(Error::from_ffi)
    }

    /// Sets one checked horizontal quality sample.
    pub fn set_horizontal_sample(&self, index: usize, value: f32) -> Result<(), Error> {
        self.inner
            .set_horizontal_sample(index, value)
            .map_err(Error::from_ffi)
    }

    /// Sets one checked vertical quality sample.
    pub fn set_vertical_sample(&self, index: usize, value: f32) -> Result<(), Error> {
        self.inner
            .set_vertical_sample(index, value)
            .map_err(Error::from_ffi)
    }
}

impl RasterizationRateMapDescriptor {
    /// Creates a checked descriptor with no layers.
    pub fn with_screen_size(screen_size: Size) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor::with_screen_size(
            screen_size,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Creates a descriptor with one layer.
    pub fn with_layer(
        screen_size: Size,
        layer: &RasterizationRateLayerDescriptor,
    ) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor::with_layer(
            screen_size,
            &layer.inner,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Creates a descriptor from a Rust slice of layers.
    pub fn with_layers(
        screen_size: Size,
        layers: &[&RasterizationRateLayerDescriptor],
    ) -> Result<Self, Error> {
        let layers: Vec<_> = layers.iter().map(|layer| &layer.inner).collect();
        metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor::with_layers(
            screen_size,
            &layers,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Returns the screen-space dimensions.
    pub fn screen_size(&self) -> Result<Size, Error> {
        self.inner.screen_size().map_err(Error::from_ffi)
    }

    /// Sets non-zero screen-space dimensions.
    pub fn set_screen_size(&self, screen_size: Size) -> Result<(), Error> {
        self.inner
            .set_screen_size(screen_size)
            .map_err(Error::from_ffi)
    }

    /// Returns one checked layer.
    pub fn layer(&self, index: usize) -> Result<Option<RasterizationRateLayerDescriptor>, Error> {
        self.inner
            .layer(index)
            .map(|layer| layer.map(RasterizationRateLayerDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns all contiguous layers as owned safe wrappers.
    pub fn layer_vec(&self) -> Result<Vec<RasterizationRateLayerDescriptor>, Error> {
        self.inner
            .layer_vec()
            .map(|layers| {
                layers
                    .into_iter()
                    .map(RasterizationRateLayerDescriptor::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces, appends, or removes one checked layer.
    pub fn set_layer(
        &self,
        index: usize,
        layer: Option<&RasterizationRateLayerDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_layer(index, layer.map(|layer| &layer.inner))
            .map_err(Error::from_ffi)
    }
}

impl RasterizationRateMap {
    /// Returns the configured screen-space dimensions.
    pub fn screen_size(&self) -> Result<Size, Error> {
        self.inner.screen_size().map_err(Error::from_ffi)
    }

    /// Returns the physical rasterization granularity.
    pub fn physical_granularity(&self) -> Result<Size, Error> {
        self.inner.physical_granularity().map_err(Error::from_ffi)
    }

    /// Returns the physical dimensions for one checked layer.
    pub fn physical_size(&self, layer_index: usize) -> Result<Size, Error> {
        self.inner
            .physical_size(layer_index)
            .map_err(Error::from_ffi)
    }

    /// Returns parameter-buffer size and alignment requirements.
    pub fn parameter_buffer_size_and_align(&self) -> Result<super::SizeAndAlign, Error> {
        self.inner
            .parameter_buffer_size_and_align()
            .map_err(Error::from_ffi)
    }

    /// Copies parameter data into a checked shared buffer range.
    pub fn copy_parameter_data_to_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_parameter_data_to_buffer(&buffer.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Maps a finite screen coordinate to physical fragment space.
    pub fn map_screen_to_physical_coordinates(
        &self,
        coordinate: SamplePosition,
        layer_index: usize,
    ) -> Result<SamplePosition, Error> {
        self.inner
            .map_screen_to_physical_coordinates(coordinate, layer_index)
            .map_err(Error::from_ffi)
    }

    /// Maps a finite physical coordinate to screen space.
    pub fn map_physical_to_screen_coordinates(
        &self,
        coordinate: SamplePosition,
        layer_index: usize,
    ) -> Result<SamplePosition, Error> {
        self.inner
            .map_physical_to_screen_coordinates(coordinate, layer_index)
            .map_err(Error::from_ffi)
    }
}

impl Heap {
    /// Returns the maximum allocatable block for a checked alignment.
    pub fn max_available_size(&self, alignment: usize) -> Result<usize, Error> {
        self.inner
            .max_available_size(alignment)
            .map_err(Error::from_ffi)
    }

    /// Creates a non-empty buffer using compatible heap resource options.
    pub fn new_buffer(
        &self,
        length: usize,
        options: super::ResourceOptions,
    ) -> Result<Buffer, Error> {
        self.inner
            .new_buffer(length, options)
            .map(Buffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked placement buffer.
    pub fn new_buffer_at_offset(
        &self,
        length: usize,
        options: super::ResourceOptions,
        offset: usize,
    ) -> Result<Buffer, Error> {
        self.inner
            .new_buffer_at_offset(length, options, offset)
            .map(Buffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a texture from a validated descriptor.
    pub fn new_texture(
        &self,
        descriptor: &super::TextureDescriptor,
    ) -> Result<super::Texture, Error> {
        self.inner
            .new_texture(&descriptor.inner)
            .map(super::Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked placement texture.
    pub fn new_texture_at_offset(
        &self,
        descriptor: &super::TextureDescriptor,
        offset: usize,
    ) -> Result<super::Texture, Error> {
        self.inner
            .new_texture_at_offset(&descriptor.inner, offset)
            .map(super::Texture::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a non-empty acceleration-structure allocation.
    pub fn new_acceleration_structure(&self, size: usize) -> Result<AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure(size)
            .map(AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked placement acceleration structure.
    pub fn new_acceleration_structure_at_offset(
        &self,
        size: usize,
        offset: usize,
    ) -> Result<AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure_at_offset(size, offset)
            .map(AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an acceleration structure whose size is inferred from a descriptor.
    pub fn new_acceleration_structure_with_descriptor(
        &self,
        descriptor: &super::AccelerationStructureDescriptor,
    ) -> Result<AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure_with_descriptor(&descriptor.inner)
            .map(AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked descriptor-based placement acceleration structure.
    pub fn new_acceleration_structure_with_descriptor_at_offset(
        &self,
        descriptor: &super::AccelerationStructureDescriptor,
        offset: usize,
    ) -> Result<AccelerationStructure, Error> {
        self.inner
            .new_acceleration_structure_with_descriptor_at_offset(&descriptor.inner, offset)
            .map(AccelerationStructure::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Changes purgeability after validating the requested value.
    pub fn set_purgeable_state(
        &self,
        state: super::PurgeableState,
    ) -> Result<super::PurgeableState, Error> {
        self.inner
            .set_purgeable_state(state)
            .map_err(Error::from_ffi)
    }
}

/// A borrowed allocation accepted by a residency set.
#[derive(Clone, Copy)]
pub enum ResidencyAllocation<'a> {
    /// A Metal buffer allocation.
    Buffer(&'a Buffer),
    /// A Metal texture allocation.
    Texture(&'a super::Texture),
    /// A Metal heap allocation.
    Heap(&'a Heap),
    /// A Metal acceleration-structure allocation.
    AccelerationStructure(&'a AccelerationStructure),
    /// Another safe allocation wrapper.
    Other(&'a Allocation),
}

impl<'a> ResidencyAllocation<'a> {
    fn to_ffi(self) -> metal_rust_ffi::ResidencyAllocation<'a> {
        match self {
            Self::Buffer(value) => metal_rust_ffi::ResidencyAllocation::Buffer(&value.inner),
            Self::Texture(value) => metal_rust_ffi::ResidencyAllocation::Texture(&value.inner),
            Self::Heap(value) => metal_rust_ffi::ResidencyAllocation::Heap(&value.inner),
            Self::AccelerationStructure(value) => {
                metal_rust_ffi::ResidencyAllocation::AccelerationStructure(&value.inner)
            }
            Self::Other(value) => metal_rust_ffi::ResidencyAllocation::Other(&value.inner),
        }
    }
}

impl ResidencySet {
    /// Adds one allocation to pending changes.
    pub fn add_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<(), Error> {
        self.inner
            .add_allocation(allocation.to_ffi())
            .map_err(Error::from_ffi)
    }

    /// Adds allocations from a Rust slice.
    pub fn add_allocations(&self, allocations: &[ResidencyAllocation<'_>]) -> Result<(), Error> {
        let allocations: Vec<_> = allocations
            .iter()
            .copied()
            .map(ResidencyAllocation::to_ffi)
            .collect();
        self.inner
            .add_allocations(&allocations)
            .map_err(Error::from_ffi)
    }

    /// Marks one allocation for removal.
    pub fn remove_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<(), Error> {
        self.inner
            .remove_allocation(allocation.to_ffi())
            .map_err(Error::from_ffi)
    }

    /// Marks allocations from a Rust slice for removal.
    pub fn remove_allocations(&self, allocations: &[ResidencyAllocation<'_>]) -> Result<(), Error> {
        let allocations: Vec<_> = allocations
            .iter()
            .copied()
            .map(ResidencyAllocation::to_ffi)
            .collect();
        self.inner
            .remove_allocations(&allocations)
            .map_err(Error::from_ffi)
    }

    /// Returns whether an allocation is present, including pending changes.
    pub fn contains_allocation(&self, allocation: ResidencyAllocation<'_>) -> Result<bool, Error> {
        self.inner
            .contains_allocation(allocation.to_ffi())
            .map_err(Error::from_ffi)
    }

    /// Copies all allocations into owned wrappers.
    pub fn allocation_vec(&self) -> Result<Vec<Allocation>, Error> {
        self.inner
            .allocation_vec()
            .map(|values| values.into_iter().map(Allocation::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Commits pending allocation changes.
    pub fn commit(&self) -> Result<(), Error> {
        self.inner.commit().map_err(Error::from_ffi)
    }

    /// Requests residency for the committed set.
    pub fn request_residency(&self) -> Result<(), Error> {
        self.inner.request_residency().map_err(Error::from_ffi)
    }

    /// Ends residency for the committed set.
    pub fn end_residency(&self) -> Result<(), Error> {
        self.inner.end_residency().map_err(Error::from_ffi)
    }

    /// Marks all allocations for removal.
    pub fn remove_all_allocations(&self) -> Result<(), Error> {
        self.inner.remove_all_allocations().map_err(Error::from_ffi)
    }
}
