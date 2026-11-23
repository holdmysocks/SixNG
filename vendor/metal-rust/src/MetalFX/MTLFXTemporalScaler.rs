//! Safe MetalFX temporal scaler API.

use crate::{Device, Error, PixelFormat, Texture};

/// A temporal-scaler descriptor.
pub struct TemporalScalerDescriptor {
    pub(crate) inner: metal_rust_ffi::TemporalScalerDescriptor,
}

impl TemporalScalerDescriptor {
    /// Creates a descriptor when MetalFX is available on the current system.
    pub fn new() -> Result<Self, Error> {
        metal_rust_ffi::TemporalScalerDescriptor::new()
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Configures formats and dimensions after validating non-zero sizes.
    #[allow(clippy::too_many_arguments)]
    pub fn configure(
        &self,
        color_format: PixelFormat,
        depth_format: PixelFormat,
        motion_format: PixelFormat,
        output_format: PixelFormat,
        input_width: usize,
        input_height: usize,
        output_width: usize,
        output_height: usize,
    ) -> Result<(), Error> {
        self.inner
            .configure(
                color_format,
                depth_format,
                motion_format,
                output_format,
                input_width,
                input_height,
                output_width,
                output_height,
            )
            .map_err(Error::from_ffi)
    }

    /// Returns configured color, depth, motion, and output formats.
    #[must_use]
    pub fn texture_formats(&self) -> (PixelFormat, PixelFormat, PixelFormat, PixelFormat) {
        self.inner.texture_formats()
    }

    /// Sets color, depth, motion, and output formats.
    pub fn set_texture_formats(
        &self,
        color: PixelFormat,
        depth: PixelFormat,
        motion: PixelFormat,
        output: PixelFormat,
    ) {
        self.inner.set_texture_formats(color, depth, motion, output);
    }

    /// Returns configured input dimensions.
    #[must_use]
    pub fn input_size(&self) -> (usize, usize) {
        self.inner.input_size()
    }

    /// Sets non-zero input dimensions.
    pub fn set_input_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner
            .set_input_size(width, height)
            .map_err(Error::from_ffi)
    }

    /// Returns configured output dimensions.
    #[must_use]
    pub fn output_size(&self) -> (usize, usize) {
        self.inner.output_size()
    }

    /// Sets non-zero output dimensions.
    pub fn set_output_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner
            .set_output_size(width, height)
            .map_err(Error::from_ffi)
    }

    /// Returns whether automatic exposure is enabled.
    #[must_use]
    pub fn auto_exposure_enabled(&self) -> bool {
        self.inner.auto_exposure_enabled()
    }

    /// Sets automatic exposure behavior.
    pub fn set_auto_exposure_enabled(&self, value: bool) {
        self.inner.set_auto_exposure_enabled(value);
    }

    /// Returns whether input-content properties are enabled.
    #[must_use]
    pub fn input_content_properties_enabled(&self) -> bool {
        self.inner.input_content_properties_enabled()
    }

    /// Sets input-content properties behavior.
    pub fn set_input_content_properties_enabled(&self, value: bool) {
        self.inner.set_input_content_properties_enabled(value);
    }

    /// Returns whether initialization must be synchronous.
    #[must_use]
    pub fn requires_synchronous_initialization(&self) -> bool {
        self.inner.requires_synchronous_initialization()
    }

    /// Sets whether initialization must be synchronous.
    pub fn set_requires_synchronous_initialization(&self, value: bool) {
        self.inner.set_requires_synchronous_initialization(value);
    }

    /// Returns whether reactive masks are enabled.
    #[must_use]
    pub fn reactive_mask_texture_enabled(&self) -> bool {
        self.inner.reactive_mask_texture_enabled()
    }

    /// Enables or disables reactive masks.
    pub fn set_reactive_mask_texture_enabled(&self, value: bool) {
        self.inner.set_reactive_mask_texture_enabled(value);
    }

    /// Returns the reactive-mask pixel format.
    #[must_use]
    pub fn reactive_mask_texture_format(&self) -> PixelFormat {
        self.inner.reactive_mask_texture_format()
    }

    /// Sets the reactive-mask pixel format.
    pub fn set_reactive_mask_texture_format(&self, value: PixelFormat) {
        self.inner.set_reactive_mask_texture_format(value);
    }

    /// Returns the configured input-content scale range.
    #[must_use]
    pub fn input_content_scale_range(&self) -> (f32, f32) {
        self.inner.input_content_scale_range()
    }

    /// Sets a finite positive input-content scale range.
    pub fn set_input_content_scale_range(&self, min: f32, max: f32) -> Result<(), Error> {
        self.inner
            .set_input_content_scale_range(min, max)
            .map_err(Error::from_ffi)
    }

    /// Returns whether motion vectors use output resolution.
    #[must_use]
    pub fn output_resolution_motion_vectors_enabled(&self) -> bool {
        self.inner.output_resolution_motion_vectors_enabled()
    }

    /// Sets whether motion vectors use output resolution.
    pub fn set_output_resolution_motion_vectors_enabled(&self, value: bool) {
        self.inner
            .set_output_resolution_motion_vectors_enabled(value);
    }

    /// Returns whether the device supports Metal 4 FX temporal scaling.
    #[must_use]
    pub fn supports_metal4_fx(&self, device: &Device) -> bool {
        self.inner.supports_metal4_fx(&device.inner)
    }

    /// Returns whether the installed MetalFX runtime supports the device.
    #[must_use]
    pub fn supports_device(&self, device: &Device) -> bool {
        self.inner.supports_device(&device.inner)
    }

    /// Returns the minimum supported input content scale.
    pub fn supported_input_content_min_scale(&self, device: &Device) -> Result<f32, Error> {
        self.inner
            .supported_input_content_min_scale(&device.inner)
            .map_err(Error::from_ffi)
    }

    /// Returns the maximum supported input content scale.
    pub fn supported_input_content_max_scale(&self, device: &Device) -> Result<f32, Error> {
        self.inner
            .supported_input_content_max_scale(&device.inner)
            .map_err(Error::from_ffi)
    }

    /// Creates a temporal scaler.
    pub fn new_scaler(&self, device: &Device) -> Result<TemporalScaler, Error> {
        self.inner
            .new_scaler(&device.inner)
            .map(|inner| TemporalScaler { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates an MTL4FX temporal scaler.
    pub fn new_mtl4_scaler(
        &self,
        device: &Device,
        compiler: &crate::metal4::Compiler,
    ) -> Result<crate::metal4_fx::TemporalScaler, Error> {
        let compiler_object = compiler.as_ffi().as_inner();
        let scaler = self
            .inner
            .new_mtl4_scaler(&device.inner, compiler_object);
        let inner = scaler.map_err(Error::from_ffi)?;
        Ok(crate::metal4_fx::TemporalScaler::from_ffi(
            metal_rust_ffi::__private::objects::metal4_fx::TemporalScaler::from_retained(inner),
        ))
    }
}

/// An owned MetalFX temporal scaler.
pub struct TemporalScaler {
    pub(crate) inner: metal_rust_ffi::TemporalScaler,
}

impl TemporalScaler {
    /// Encodes the scaling operation into a command buffer.
    pub fn encode(&self, command_buffer: &mut crate::CommandBuffer) {
        self.inner.encode(&command_buffer.inner);
    }

    /// Returns the required input texture usage bitmask.
    #[must_use]
    pub fn color_texture_usage(&self) -> u64 {
        self.inner.color_texture_usage()
    }

    /// Returns the required output texture usage bitmask.
    #[must_use]
    pub fn output_texture_usage(&self) -> u64 {
        self.inner.output_texture_usage()
    }

    /// Returns the required input depth texture usage bitmask.
    #[must_use]
    pub fn depth_texture_usage(&self) -> u64 {
        self.inner.depth_texture_usage()
    }

    /// Returns the required input motion texture usage bitmask.
    #[must_use]
    pub fn motion_texture_usage(&self) -> u64 {
        self.inner.motion_texture_usage()
    }

    /// Returns assigned color, depth, motion, and output textures.
    #[must_use]
    pub fn textures(
        &self,
    ) -> (
        Option<Texture>,
        Option<Texture>,
        Option<Texture>,
        Option<Texture>,
    ) {
        let (color, depth, motion, output) = self.inner.textures();
        (
            color.map(Texture::from_ffi),
            depth.map(Texture::from_ffi),
            motion.map(Texture::from_ffi),
            output.map(Texture::from_ffi),
        )
    }

    /// Returns the exposure texture.
    #[must_use]
    pub fn exposure_texture(&self) -> Option<Texture> {
        self.inner.exposure_texture().map(Texture::from_ffi)
    }

    /// Sets the optional exposure texture.
    pub fn set_exposure_texture(&self, value: Option<&Texture>) {
        self.inner
            .set_exposure_texture(value.map(|texture| &texture.inner));
    }

    /// Returns the reactive-mask texture.
    #[must_use]
    pub fn reactive_mask_texture(&self) -> Option<Texture> {
        self.inner.reactive_mask_texture().map(Texture::from_ffi)
    }

    /// Sets the optional reactive-mask texture.
    pub fn set_reactive_mask_texture(&self, value: Option<&Texture>) {
        self.inner
            .set_reactive_mask_texture(value.map(|texture| &texture.inner));
    }

    /// Returns the deprecated reactive texture usage value.
    #[must_use]
    pub fn reactive_texture_usage(&self) -> u64 {
        self.inner.reactive_texture_usage()
    }

    /// Sets whether depth values are reversed.
    pub fn set_depth_reversed(&self, value: bool) {
        self.inner.set_depth_reversed(value);
    }

    /// Sets the input content dimensions after checking they are non-zero.
    pub fn set_input_content_size(&self, width: usize, height: usize) -> Result<(), Error> {
        self.inner
            .set_input_content_size(width, height)
            .map_err(Error::from_ffi)
    }

    /// Assigns the color, depth, motion, and output textures. The output must
    /// use private storage.
    pub fn set_textures(
        &self,
        color: Option<&Texture>,
        depth: Option<&Texture>,
        motion: Option<&Texture>,
        output: Option<&Texture>,
    ) -> Result<(), Error> {
        self.inner
            .set_textures(
                color.map(|texture| &texture.inner),
                depth.map(|texture| &texture.inner),
                motion.map(|texture| &texture.inner),
                output.map(|texture| &texture.inner),
            )
            .map_err(Error::from_ffi)
    }
}
