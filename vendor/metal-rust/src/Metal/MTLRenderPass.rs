//! Safe render and compute pass descriptors.

use super::{ClearColor, Texture};
use crate::Error;

/// Copyable, validated scalar configuration for a render pass.
pub use metal_rust_ffi::RenderPassOptions;

/// A render-pass descriptor with checked color-attachment indices.
pub struct RenderPassDescriptor {
    pub(crate) inner: metal_rust_ffi::RenderPassDescriptor,
}

impl RenderPassDescriptor {
    /// Creates an empty descriptor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: metal_rust_ffi::RenderPassDescriptor::new(),
        }
    }

    /// Sets a color attachment's texture and clear color.
    pub fn set_color_attachment(
        &self,
        index: usize,
        texture: &Texture,
        clear_color: ClearColor,
    ) -> Result<(), Error> {
        self.inner
            .set_color_attachment(index, &texture.inner, clear_color)
            .map_err(Error::from_ffi)
    }

    /// Returns a color attachment descriptor by checked attachment index.
    pub fn color_attachment(
        &self,
        index: usize,
    ) -> Result<Option<super::RenderPassColorAttachmentDescriptor>, Error> {
        self.inner
            .color_attachment(index)
            .map(|value| value.map(super::RenderPassColorAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears a color attachment descriptor at a checked index.
    pub fn set_color_attachment_descriptor(
        &self,
        index: usize,
        value: Option<&super::RenderPassColorAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_color_attachment_descriptor(index, value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns a color attachment's clear color by checked index.
    pub fn color_attachment_clear_color(&self, index: usize) -> Result<ClearColor, Error> {
        self.inner
            .color_attachment_clear_color(index)
            .map_err(Error::from_ffi)
    }

    /// Updates a color attachment's clear color by checked index.
    pub fn set_color_attachment_clear_color(
        &self,
        index: usize,
        value: ClearColor,
    ) -> Result<(), Error> {
        self.inner
            .set_color_attachment_clear_color(index, value)
            .map_err(Error::from_ffi)
    }

    /// Returns the optional depth attachment descriptor.
    pub fn depth_attachment(
        &self,
    ) -> Result<Option<super::RenderPassDepthAttachmentDescriptor>, Error> {
        self.inner
            .depth_attachment()
            .map(|value| value.map(super::RenderPassDepthAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the depth attachment descriptor.
    pub fn set_depth_attachment(
        &self,
        value: Option<&super::RenderPassDepthAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_depth_attachment(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns the optional stencil attachment descriptor.
    pub fn stencil_attachment(
        &self,
    ) -> Result<Option<super::RenderPassStencilAttachmentDescriptor>, Error> {
        self.inner
            .stencil_attachment()
            .map(|value| value.map(super::RenderPassStencilAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the stencil attachment descriptor.
    pub fn set_stencil_attachment(
        &self,
        value: Option<&super::RenderPassStencilAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_stencil_attachment(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns the optional rasterization-rate map.
    pub fn rasterization_rate_map(&self) -> Result<Option<super::RasterizationRateMap>, Error> {
        self.inner
            .rasterization_rate_map()
            .map(|value| value.map(super::RasterizationRateMap::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the rasterization-rate map.
    pub fn set_rasterization_rate_map(
        &self,
        value: Option<&super::RasterizationRateMap>,
    ) -> Result<(), Error> {
        self.inner
            .set_rasterization_rate_map(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns a counter sample-buffer attachment by checked index.
    pub fn sample_buffer_attachment(
        &self,
        index: usize,
    ) -> Result<Option<super::RenderPassSampleBufferAttachmentDescriptor>, Error> {
        self.inner
            .sample_buffer_attachment(index)
            .map(|value| value.map(super::RenderPassSampleBufferAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears a counter sample-buffer attachment at a checked index.
    pub fn set_sample_buffer_attachment(
        &self,
        index: usize,
        value: Option<&super::RenderPassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_sample_buffer_attachment(index, value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns scalar render-pass configuration after availability checks.
    pub fn options(&self) -> Result<RenderPassOptions, Error> {
        self.inner.options().map_err(Error::from_ffi)
    }

    /// Applies scalar render-pass configuration without partial mutation.
    pub fn set_options(&self, value: &RenderPassOptions) -> Result<(), Error> {
        self.inner.set_options(value).map_err(Error::from_ffi)
    }

    /// Returns up to `count` programmable sample positions as owned values.
    pub fn sample_positions(&self, count: usize) -> Result<Vec<super::SamplePosition>, Error> {
        self.inner.sample_positions(count).map_err(Error::from_ffi)
    }

    /// Sets programmable sample positions from a Rust slice.
    pub fn set_sample_positions(&self, positions: &[super::SamplePosition]) -> Result<(), Error> {
        self.inner
            .set_sample_positions(positions)
            .map_err(Error::from_ffi)
    }

    /// Returns the optional visibility-result buffer.
    pub fn visibility_result_buffer(&self) -> Result<Option<super::Buffer>, Error> {
        self.inner
            .visibility_result_buffer()
            .map(|value| value.map(super::Buffer::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the visibility-result buffer.
    pub fn set_visibility_result_buffer(&self, value: Option<&super::Buffer>) {
        self.inner
            .set_visibility_result_buffer(value.map(|buffer| &buffer.inner));
    }
}

impl Default for RenderPassDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// A compute-pass descriptor.
pub struct ComputePassDescriptor {
    pub(crate) inner: metal_rust_ffi::ComputePassDescriptor,
}

impl ComputePassDescriptor {
    /// Creates a default compute-pass descriptor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: metal_rust_ffi::ComputePassDescriptor::new(),
        }
    }

    /// Returns the dispatch scheduling type after availability checks.
    pub fn dispatch_type(&self) -> Result<super::DispatchType, Error> {
        self.inner.dispatch_type().map_err(Error::from_ffi)
    }

    /// Sets the dispatch scheduling type after value and availability checks.
    pub fn set_dispatch_type(&self, value: super::DispatchType) -> Result<(), Error> {
        self.inner.set_dispatch_type(value).map_err(Error::from_ffi)
    }

    /// Returns a counter sample-buffer attachment by checked index.
    pub fn sample_buffer_attachment(
        &self,
        index: usize,
    ) -> Result<Option<super::ComputePassSampleBufferAttachmentDescriptor>, Error> {
        self.inner
            .sample_buffer_attachment(index)
            .map(|value| value.map(super::ComputePassSampleBufferAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears a counter sample-buffer attachment at a checked index.
    pub fn set_sample_buffer_attachment(
        &self,
        index: usize,
        value: Option<&super::ComputePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_sample_buffer_attachment(index, value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl Default for ComputePassDescriptor {
    fn default() -> Self {
        Self::new()
    }
}
