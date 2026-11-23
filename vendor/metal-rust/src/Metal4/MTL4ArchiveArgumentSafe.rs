//! Safe public Metal 4 archive queries and argument-table bindings.

use crate::Error;
use crate::metal::{
    AccelerationStructure, Buffer, ComputePipelineState, Device, RenderPipelineState, SamplerState,
    Tensor, Texture,
};
use crate::metal4::{
    BinaryFunction, BinaryFunctionDescriptor, ComputePipelineDescriptor, PipelineDescriptor,
    PipelineStageDynamicLinkingDescriptor, RenderPipelineDynamicLinkingDescriptor,
};

/// A checked, Rust-owned description of a Metal 4 argument table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArgumentTableDescriptor {
    pub(crate) inner: metal_rust_ffi::Mtl4ArgumentTableDescriptor,
}

impl ArgumentTableDescriptor {
    /// Creates a descriptor with no binding slots.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.inner.label()
    }

    /// Replaces or clears the debug label.
    pub fn set_label(&mut self, label: Option<&str>) {
        self.inner.set_label(label);
    }

    /// Returns the buffer binding count.
    #[must_use]
    pub const fn max_buffer_bind_count(&self) -> usize {
        self.inner.max_buffer_bind_count()
    }

    /// Sets the buffer binding count after validation.
    pub fn set_max_buffer_bind_count(&mut self, count: usize) -> Result<(), Error> {
        self.inner
            .set_max_buffer_bind_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns the texture binding count.
    #[must_use]
    pub const fn max_texture_bind_count(&self) -> usize {
        self.inner.max_texture_bind_count()
    }

    /// Sets the texture binding count after validation.
    pub fn set_max_texture_bind_count(&mut self, count: usize) -> Result<(), Error> {
        self.inner
            .set_max_texture_bind_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns the sampler binding count.
    #[must_use]
    pub const fn max_sampler_state_bind_count(&self) -> usize {
        self.inner.max_sampler_state_bind_count()
    }

    /// Sets the sampler binding count after validation.
    pub fn set_max_sampler_state_bind_count(&mut self, count: usize) -> Result<(), Error> {
        self.inner
            .set_max_sampler_state_bind_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns whether Metal initializes table bindings.
    #[must_use]
    pub const fn initialize_bindings(&self) -> bool {
        self.inner.initialize_bindings()
    }

    /// Configures whether Metal initializes table bindings.
    pub fn set_initialize_bindings(&mut self, initialize: bool) {
        self.inner.set_initialize_bindings(initialize);
    }

    /// Returns whether dynamic attribute strides are enabled.
    #[must_use]
    pub const fn support_attribute_strides(&self) -> bool {
        self.inner.support_attribute_strides()
    }

    /// Configures support for dynamic attribute strides.
    pub fn set_support_attribute_strides(&mut self, support: bool) {
        self.inner.set_support_attribute_strides(support);
    }
}

/// A Metal 4 argument table retaining the exact checked descriptor bounds.
#[derive(Clone)]
pub struct ArgumentTable {
    pub(crate) inner: metal_rust_ffi::Mtl4ArgumentTable,
}

/// A typed Metal resource accepted by an argument-table buffer slot.
#[derive(Clone, Copy)]
pub enum ArgumentTableResource<'a> {
    /// A conventional Metal buffer resource.
    Buffer(&'a Buffer),
    /// A Metal tensor resource.
    Tensor(&'a Tensor),
    /// A Metal acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl<'a> ArgumentTableResource<'a> {
    fn to_ffi(self) -> metal_rust_ffi::Mtl4ArgumentTableResource<'a> {
        match self {
            Self::Buffer(value) => metal_rust_ffi::Mtl4ArgumentTableResource::Buffer(&value.inner),
            Self::Tensor(value) => metal_rust_ffi::Mtl4ArgumentTableResource::Tensor(&value.inner),
            Self::AccelerationStructure(value) => {
                metal_rust_ffi::Mtl4ArgumentTableResource::AccelerationStructure(&value.inner)
            }
        }
    }
}

impl Device {
    /// Creates an argument table without exposing Objective-C error writeback.
    pub fn new_mtl4_argument_table(
        &self,
        descriptor: &ArgumentTableDescriptor,
    ) -> Result<ArgumentTable, Error> {
        self.inner
            .new_mtl4_argument_table(&descriptor.inner)
            .map(|inner| ArgumentTable { inner })
            .map_err(Error::from_ffi)
    }
}

impl ArgumentTable {
    /// Returns the device that created this table.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }

    /// Returns the label captured when this table was created.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.inner.label()
    }

    /// Binds a buffer address after validating offset and index.
    pub fn set_buffer(&self, buffer: &Buffer, offset: usize, index: usize) -> Result<(), Error> {
        self.inner
            .set_buffer(&buffer.inner, offset, index)
            .map_err(Error::from_ffi)
    }

    /// Binds a buffer address with a checked dynamic attribute stride.
    pub fn set_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer_with_stride(&buffer.inner, offset, stride, index)
            .map_err(Error::from_ffi)
    }

    /// Binds consecutive buffers from a checked Rust slice.
    pub fn set_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = bindings
            .iter()
            .map(|&(buffer, offset)| (&buffer.inner, offset))
            .collect::<Vec<_>>();
        self.inner
            .set_buffers(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds a texture at a checked index.
    pub fn set_texture(&self, texture: &Texture, index: usize) -> Result<(), Error> {
        self.inner
            .set_texture(&texture.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Binds consecutive textures from a checked Rust slice.
    pub fn set_textures(&self, textures: &[&Texture], start_index: usize) -> Result<(), Error> {
        let ffi = textures
            .iter()
            .map(|texture| &texture.inner)
            .collect::<Vec<_>>();
        self.inner
            .set_textures(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds a typed resource at a checked buffer index.
    pub fn set_resource(
        &self,
        resource: ArgumentTableResource<'_>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_resource(resource.to_ffi(), index)
            .map_err(Error::from_ffi)
    }

    /// Binds a sampler at a checked index.
    pub fn set_sampler_state(&self, sampler: &SamplerState, index: usize) -> Result<(), Error> {
        self.inner
            .set_sampler_state(&sampler.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Binds consecutive samplers from a Rust slice.
    pub fn set_sampler_states(
        &self,
        samplers: &[&SamplerState],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = samplers
            .iter()
            .map(|sampler| &sampler.inner)
            .collect::<Vec<_>>();
        self.inner
            .set_sampler_states(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds a tensor at a checked buffer index.
    pub fn set_tensor(&self, tensor: &Tensor, index: usize) -> Result<(), Error> {
        self.inner
            .set_tensor(&tensor.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Binds consecutive tensors from a Rust slice.
    pub fn set_tensors(&self, tensors: &[&Tensor], start_index: usize) -> Result<(), Error> {
        let ffi = tensors
            .iter()
            .map(|tensor| &tensor.inner)
            .collect::<Vec<_>>();
        self.inner
            .set_tensors(&ffi, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds an acceleration structure at a checked buffer index.
    pub fn set_acceleration_structure(
        &self,
        structure: &AccelerationStructure,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_acceleration_structure(&structure.inner, index)
            .map_err(Error::from_ffi)
    }

    /// Binds consecutive acceleration structures from a Rust slice.
    pub fn set_acceleration_structures(
        &self,
        structures: &[&AccelerationStructure],
        start_index: usize,
    ) -> Result<(), Error> {
        let ffi = structures
            .iter()
            .map(|structure| &structure.inner)
            .collect::<Vec<_>>();
        self.inner
            .set_acceleration_structures(&ffi, start_index)
            .map_err(Error::from_ffi)
    }
}

/// A read-only Metal 4 pipeline archive returned by a serializer.
#[derive(Clone)]
pub struct Archive {
    pub(crate) inner: metal_rust_ffi::Mtl4Archive,
}

impl Archive {
    #[allow(dead_code)]
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Mtl4Archive) -> Self {
        Self { inner }
    }

    /// Returns the optional archive label.
    pub fn label(&self) -> Result<Option<String>, Error> {
        self.inner.label().map_err(Error::from_ffi)
    }

    /// Replaces or clears the archive label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), Error> {
        self.inner.set_label(label).map_err(Error::from_ffi)
    }

    /// Looks up a binary function from this archive.
    pub fn new_binary_function(
        &self,
        descriptor: &BinaryFunctionDescriptor,
    ) -> Result<BinaryFunction, Error> {
        self.inner
            .new_binary_function(&descriptor.inner)
            .map(BinaryFunction::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Looks up a compute pipeline, optionally applying dynamic linking.
    pub fn new_compute_pipeline_state(
        &self,
        descriptor: &ComputePipelineDescriptor,
        linking: Option<&PipelineStageDynamicLinkingDescriptor>,
    ) -> Result<ComputePipelineState, Error> {
        self.inner
            .new_compute_pipeline_state(&descriptor.inner, linking.map(|value| &value.inner))
            .map(|inner| ComputePipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Looks up a render pipeline, optionally applying dynamic linking.
    pub fn new_render_pipeline_state(
        &self,
        descriptor: &PipelineDescriptor,
        linking: Option<&RenderPipelineDynamicLinkingDescriptor>,
    ) -> Result<RenderPipelineState, Error> {
        self.inner
            .new_render_pipeline_state(&descriptor.inner, linking.map(|value| &value.inner))
            .map(|inner| RenderPipelineState { inner })
            .map_err(Error::from_ffi)
    }
}
