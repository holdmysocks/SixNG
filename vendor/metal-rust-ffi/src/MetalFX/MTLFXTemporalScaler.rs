//! Audited MetalFX temporal scaler binding.
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::{CommandBuffer, Device, PixelFormat, StorageMode, Texture};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, ProtocolObject};
use objc2::{extern_class, extern_conformance, extern_methods, extern_protocol, sel};
use objc2_foundation::{NSCopying, NSObject, NSObjectProtocol};
use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLTexture, MTLTextureUsage};

extern_protocol!(
    /// MetalFX temporal scaler protocol declared by the macOS SDK.
    ///
    /// # Safety
    ///
    /// This trait is implemented only by the Objective-C runtime object whose
    /// selectors and return layouts match the SDK protocol declaration.
    unsafe trait MTLFXTemporalScaler: NSObjectProtocol {
        #[unsafe(method(encodeToCommandBuffer:))]
        #[unsafe(method_family = none)]
        fn encodeToCommandBuffer(&self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>);

        #[unsafe(method(colorTextureUsage))]
        #[unsafe(method_family = none)]
        fn colorTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(depthTextureUsage))]
        #[unsafe(method_family = none)]
        fn depthTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(motionTextureUsage))]
        #[unsafe(method_family = none)]
        fn motionTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(outputTextureUsage))]
        #[unsafe(method_family = none)]
        fn outputTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(colorTexture))]
        #[unsafe(method_family = none)]
        fn colorTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(depthTexture))]
        #[unsafe(method_family = none)]
        fn depthTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(motionTexture))]
        #[unsafe(method_family = none)]
        fn motionTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(outputTexture))]
        #[unsafe(method_family = none)]
        fn outputTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(exposureTexture))]
        #[unsafe(method_family = none)]
        fn exposureTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(setExposureTexture:))]
        #[unsafe(method_family = none)]
        fn setExposureTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(reactiveMaskTexture))]
        #[unsafe(method_family = none)]
        fn reactiveMaskTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
        #[unsafe(method(setReactiveMaskTexture:))]
        #[unsafe(method_family = none)]
        fn setReactiveMaskTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
        #[unsafe(method(reactiveTextureUsage))]
        #[unsafe(method_family = none)]
        fn reactiveTextureUsage(&self) -> MTLTextureUsage;
        #[unsafe(method(setDepthReversed:))]
        #[unsafe(method_family = none)]
        fn setDepthReversed(&self, value: bool);

        #[unsafe(method(setInputContentWidth:))]
        #[unsafe(method_family = none)]
        fn setInputContentWidth(&self, width: usize);

        #[unsafe(method(setInputContentHeight:))]
        #[unsafe(method_family = none)]
        fn setInputContentHeight(&self, height: usize);

        #[unsafe(method(setColorTexture:))]
        #[unsafe(method_family = none)]
        fn setColorTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);

        #[unsafe(method(setDepthTexture:))]
        #[unsafe(method_family = none)]
        fn setDepthTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);

        #[unsafe(method(setMotionTexture:))]
        #[unsafe(method_family = none)]
        fn setMotionTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);

        #[unsafe(method(setOutputTexture:))]
        #[unsafe(method_family = none)]
        fn setOutputTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    }
);

extern_class!(
    /// MetalFX temporal scaler descriptor declared by the macOS SDK.
    #[unsafe(super(NSObject))]
    pub struct MTLFXTemporalScalerDescriptor;
);

extern_conformance!(
    unsafe impl NSObjectProtocol for MTLFXTemporalScalerDescriptor {}
);
extern_conformance!(
    unsafe impl NSCopying for MTLFXTemporalScalerDescriptor {}
);

impl MTLFXTemporalScalerDescriptor {
    extern_methods!(
        #[unsafe(method(new))]
        #[unsafe(method_family = new)]
        pub fn new() -> Retained<Self>;

        #[unsafe(method(setColorTextureFormat:))]
        #[unsafe(method_family = none)]
        pub fn setColorTextureFormat(&self, format: objc2_metal::MTLPixelFormat);
        #[unsafe(method(colorTextureFormat))]
        #[unsafe(method_family = none)]
        pub fn colorTextureFormat(&self) -> objc2_metal::MTLPixelFormat;

        #[unsafe(method(setDepthTextureFormat:))]
        #[unsafe(method_family = none)]
        pub fn setDepthTextureFormat(&self, format: objc2_metal::MTLPixelFormat);
        #[unsafe(method(depthTextureFormat))]
        #[unsafe(method_family = none)]
        pub fn depthTextureFormat(&self) -> objc2_metal::MTLPixelFormat;

        #[unsafe(method(setMotionTextureFormat:))]
        #[unsafe(method_family = none)]
        pub fn setMotionTextureFormat(&self, format: objc2_metal::MTLPixelFormat);
        #[unsafe(method(motionTextureFormat))]
        #[unsafe(method_family = none)]
        pub fn motionTextureFormat(&self) -> objc2_metal::MTLPixelFormat;

        #[unsafe(method(setOutputTextureFormat:))]
        #[unsafe(method_family = none)]
        pub fn setOutputTextureFormat(&self, format: objc2_metal::MTLPixelFormat);
        #[unsafe(method(outputTextureFormat))]
        #[unsafe(method_family = none)]
        pub fn outputTextureFormat(&self) -> objc2_metal::MTLPixelFormat;

        #[unsafe(method(setInputWidth:))]
        #[unsafe(method_family = none)]
        pub fn setInputWidth(&self, width: usize);
        #[unsafe(method(inputWidth))]
        #[unsafe(method_family = none)]
        pub fn inputWidth(&self) -> usize;

        #[unsafe(method(setInputHeight:))]
        #[unsafe(method_family = none)]
        pub fn setInputHeight(&self, height: usize);
        #[unsafe(method(inputHeight))]
        #[unsafe(method_family = none)]
        pub fn inputHeight(&self) -> usize;

        #[unsafe(method(setOutputWidth:))]
        #[unsafe(method_family = none)]
        pub fn setOutputWidth(&self, width: usize);
        #[unsafe(method(outputWidth))]
        #[unsafe(method_family = none)]
        pub fn outputWidth(&self) -> usize;

        #[unsafe(method(setOutputHeight:))]
        #[unsafe(method_family = none)]
        pub fn setOutputHeight(&self, height: usize);
        #[unsafe(method(outputHeight))]
        #[unsafe(method_family = none)]
        pub fn outputHeight(&self) -> usize;

        #[unsafe(method(isAutoExposureEnabled))]
        #[unsafe(method_family = none)]
        pub fn isAutoExposureEnabled(&self) -> bool;
        #[unsafe(method(setAutoExposureEnabled:))]
        #[unsafe(method_family = none)]
        pub fn setAutoExposureEnabled(&self, value: bool);
        #[unsafe(method(isInputContentPropertiesEnabled))]
        #[unsafe(method_family = none)]
        pub fn isInputContentPropertiesEnabled(&self) -> bool;
        #[unsafe(method(setInputContentPropertiesEnabled:))]
        #[unsafe(method_family = none)]
        pub fn setInputContentPropertiesEnabled(&self, value: bool);
        #[unsafe(method(requiresSynchronousInitialization))]
        #[unsafe(method_family = none)]
        pub fn requiresSynchronousInitialization(&self) -> bool;
        #[unsafe(method(setRequiresSynchronousInitialization:))]
        #[unsafe(method_family = none)]
        pub fn setRequiresSynchronousInitialization(&self, value: bool);
        #[unsafe(method(isReactiveMaskTextureEnabled))]
        #[unsafe(method_family = none)]
        pub fn isReactiveMaskTextureEnabled(&self) -> bool;
        #[unsafe(method(setReactiveMaskTextureEnabled:))]
        #[unsafe(method_family = none)]
        pub fn setReactiveMaskTextureEnabled(&self, value: bool);
        #[unsafe(method(reactiveMaskTextureFormat))]
        #[unsafe(method_family = none)]
        pub fn reactiveMaskTextureFormat(&self) -> objc2_metal::MTLPixelFormat;
        #[unsafe(method(setReactiveMaskTextureFormat:))]
        #[unsafe(method_family = none)]
        pub fn setReactiveMaskTextureFormat(&self, value: objc2_metal::MTLPixelFormat);
        #[unsafe(method(inputContentMinScale))]
        #[unsafe(method_family = none)]
        pub fn inputContentMinScale(&self) -> f32;
        #[unsafe(method(setInputContentMinScale:))]
        #[unsafe(method_family = none)]
        pub fn setInputContentMinScale(&self, value: f32);
        #[unsafe(method(inputContentMaxScale))]
        #[unsafe(method_family = none)]
        pub fn inputContentMaxScale(&self) -> f32;
        #[unsafe(method(setInputContentMaxScale:))]
        #[unsafe(method_family = none)]
        pub fn setInputContentMaxScale(&self, value: f32);
        #[unsafe(method(isOutputResolutionMotionVectorsEnabled))]
        #[unsafe(method_family = none)]
        pub fn isOutputResolutionMotionVectorsEnabled(&self) -> bool;
        #[unsafe(method(setOutputResolutionMotionVectorsEnabled:))]
        #[unsafe(method_family = none)]
        pub fn setOutputResolutionMotionVectorsEnabled(&self, value: bool);

        #[unsafe(method(newTemporalScalerWithDevice:))]
        #[unsafe(method_family = new)]
        pub fn newTemporalScalerWithDevice(
            &self,
            device: &ProtocolObject<dyn MTLDevice>,
        ) -> Option<Retained<ProtocolObject<dyn MTLFXTemporalScaler>>>;

        #[unsafe(method(newTemporalScalerWithDevice:compiler:))]
        #[unsafe(method_family = new)]
        pub fn newTemporalScalerWithDeviceCompiler(
            &self,
            device: &ProtocolObject<dyn MTLDevice>,
            compiler: &ProtocolObject<dyn objc2_metal::MTL4Compiler>,
        ) -> Option<Retained<objc2::runtime::AnyObject>>;

        #[unsafe(method(supportsDevice:))]
        #[unsafe(method_family = none)]
        pub fn supportsDevice(device: &ProtocolObject<dyn MTLDevice>) -> bool;

        #[unsafe(method(supportsMetal4FX:))]
        #[unsafe(method_family = none)]
        pub fn supportsMetal4FX(device: &ProtocolObject<dyn MTLDevice>) -> bool;

        #[unsafe(method(supportedInputContentMinScaleForDevice:))]
        #[unsafe(method_family = none)]
        pub fn supportedInputContentMinScaleForDevice(
            device: &ProtocolObject<dyn MTLDevice>,
        ) -> f32;

        #[unsafe(method(supportedInputContentMaxScaleForDevice:))]
        #[unsafe(method_family = none)]
        pub fn supportedInputContentMaxScaleForDevice(
            device: &ProtocolObject<dyn MTLDevice>,
        ) -> f32;
    );
}

/// An owned temporal-scaler descriptor.
pub struct TemporalScalerDescriptor {
    inner: Retained<MTLFXTemporalScalerDescriptor>,
    _thread_bound: ThreadBound,
}

impl TemporalScalerDescriptor {
    /// Creates a descriptor when the MetalFX class is present at runtime.
    pub fn new() -> Result<Self, Error> {
        if AnyClass::get(c"MTLFXTemporalScalerDescriptor").is_none() {
            return Err(Error::unsupported(
                "MetalFX temporal scaling is unavailable on this system",
            ));
        }
        Ok(Self {
            inner: MTLFXTemporalScalerDescriptor::new(),
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Sets formats and dimensions after validating non-zero dimensions.
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
        if input_width == 0 || input_height == 0 || output_width == 0 || output_height == 0 {
            return Err(Error::invalid_argument(
                "MetalFX dimensions must be greater than zero",
            ));
        }
        self.inner.setColorTextureFormat(color_format.as_objc());
        self.inner.setDepthTextureFormat(depth_format.as_objc());
        self.inner.setMotionTextureFormat(motion_format.as_objc());
        self.inner.setOutputTextureFormat(output_format.as_objc());
        self.inner.setInputWidth(input_width);
        self.inner.setInputHeight(input_height);
        self.inner.setOutputWidth(output_width);
        self.inner.setOutputHeight(output_height);
        Ok(())
    }

    /// Returns the four configured texture formats.
    #[must_use]
    pub fn texture_formats(&self) -> (PixelFormat, PixelFormat, PixelFormat, PixelFormat) {
        (
            PixelFormat::from_system_raw(self.inner.colorTextureFormat().0),
            PixelFormat::from_system_raw(self.inner.depthTextureFormat().0),
            PixelFormat::from_system_raw(self.inner.motionTextureFormat().0),
            PixelFormat::from_system_raw(self.inner.outputTextureFormat().0),
        )
    }

    /// Sets the four texture formats.
    pub fn set_texture_formats(
        &self,
        color: PixelFormat,
        depth: PixelFormat,
        motion: PixelFormat,
        output: PixelFormat,
    ) {
        self.inner.setColorTextureFormat(color.as_objc());
        self.inner.setDepthTextureFormat(depth.as_objc());
        self.inner.setMotionTextureFormat(motion.as_objc());
        self.inner.setOutputTextureFormat(output.as_objc());
    }

    /// Returns configured input dimensions.
    #[must_use]
    pub fn input_size(&self) -> (usize, usize) {
        (self.inner.inputWidth(), self.inner.inputHeight())
    }

    /// Sets non-zero input dimensions.
    pub fn set_input_size(&self, width: usize, height: usize) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "MetalFX input dimensions must be greater than zero",
            ));
        }
        self.inner.setInputWidth(width);
        self.inner.setInputHeight(height);
        Ok(())
    }

    /// Returns configured output dimensions.
    #[must_use]
    pub fn output_size(&self) -> (usize, usize) {
        (self.inner.outputWidth(), self.inner.outputHeight())
    }

    /// Sets non-zero output dimensions.
    pub fn set_output_size(&self, width: usize, height: usize) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "MetalFX output dimensions must be greater than zero",
            ));
        }
        self.inner.setOutputWidth(width);
        self.inner.setOutputHeight(height);
        Ok(())
    }

    /// Returns whether automatic exposure is enabled.
    #[must_use]
    pub fn auto_exposure_enabled(&self) -> bool {
        self.inner.isAutoExposureEnabled()
    }

    /// Sets automatic exposure behavior.
    pub fn set_auto_exposure_enabled(&self, value: bool) {
        self.inner.setAutoExposureEnabled(value);
    }

    /// Returns whether input-content properties are enabled.
    #[must_use]
    pub fn input_content_properties_enabled(&self) -> bool {
        self.inner.isInputContentPropertiesEnabled()
    }

    /// Sets input-content properties behavior.
    pub fn set_input_content_properties_enabled(&self, value: bool) {
        self.inner.setInputContentPropertiesEnabled(value);
    }

    /// Returns whether initialization must be synchronous.
    #[must_use]
    pub fn requires_synchronous_initialization(&self) -> bool {
        self.inner.requiresSynchronousInitialization()
    }

    /// Sets whether initialization must be synchronous.
    pub fn set_requires_synchronous_initialization(&self, value: bool) {
        self.inner.setRequiresSynchronousInitialization(value);
    }

    /// Returns whether reactive-mask textures are enabled.
    #[must_use]
    pub fn reactive_mask_texture_enabled(&self) -> bool {
        self.inner.isReactiveMaskTextureEnabled()
    }

    /// Enables or disables reactive-mask textures.
    pub fn set_reactive_mask_texture_enabled(&self, value: bool) {
        self.inner.setReactiveMaskTextureEnabled(value);
    }

    /// Returns the reactive-mask texture format.
    #[must_use]
    pub fn reactive_mask_texture_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.reactiveMaskTextureFormat().0)
    }

    /// Sets the reactive-mask texture format.
    pub fn set_reactive_mask_texture_format(&self, value: PixelFormat) {
        self.inner.setReactiveMaskTextureFormat(value.as_objc());
    }

    /// Returns the configured input scale range.
    #[must_use]
    pub fn input_content_scale_range(&self) -> (f32, f32) {
        (
            self.inner.inputContentMinScale(),
            self.inner.inputContentMaxScale(),
        )
    }

    /// Sets a finite positive input scale range.
    pub fn set_input_content_scale_range(&self, min: f32, max: f32) -> Result<(), Error> {
        if !min.is_finite() || !max.is_finite() || min <= 0.0 || max < min {
            return Err(Error::invalid_argument(
                "MetalFX input scale range must be finite, positive, and ordered",
            ));
        }
        self.inner.setInputContentMinScale(min);
        self.inner.setInputContentMaxScale(max);
        Ok(())
    }

    /// Returns whether motion vectors use output resolution.
    #[must_use]
    pub fn output_resolution_motion_vectors_enabled(&self) -> bool {
        self.inner.isOutputResolutionMotionVectorsEnabled()
    }

    /// Sets whether motion vectors use output resolution.
    pub fn set_output_resolution_motion_vectors_enabled(&self, value: bool) {
        self.inner.setOutputResolutionMotionVectorsEnabled(value);
    }

    /// Returns whether the device supports Metal 4 FX temporal scaling.
    #[must_use]
    pub fn supports_metal4_fx(&self, device: &Device) -> bool {
        MTLFXTemporalScalerDescriptor::supportsMetal4FX(&device.inner)
    }

    /// Returns whether the installed MetalFX runtime supports the device.
    #[must_use]
    pub fn supports_device(&self, device: &Device) -> bool {
        MTLFXTemporalScalerDescriptor::supportsDevice(&device.inner)
    }

    /// Returns the minimum input content scale supported by MetalFX.
    pub fn supported_input_content_min_scale(&self, device: &Device) -> Result<f32, Error> {
        let class = AnyClass::get(c"MTLFXTemporalScalerDescriptor").ok_or_else(|| {
            Error::unsupported("MetalFX temporal scaling is unavailable on this system")
        })?;
        if !class
            .metaclass()
            .responds_to(sel!(supportedInputContentMinScaleForDevice:))
        {
            return Err(Error::unsupported(
                "MetalFX temporal scale queries require macOS 14 or newer",
            ));
        }
        Ok(MTLFXTemporalScalerDescriptor::supportedInputContentMinScaleForDevice(&device.inner))
    }

    /// Returns the maximum input content scale supported by MetalFX.
    pub fn supported_input_content_max_scale(&self, device: &Device) -> Result<f32, Error> {
        let class = AnyClass::get(c"MTLFXTemporalScalerDescriptor").ok_or_else(|| {
            Error::unsupported("MetalFX temporal scaling is unavailable on this system")
        })?;
        if !class
            .metaclass()
            .responds_to(sel!(supportedInputContentMaxScaleForDevice:))
        {
            return Err(Error::unsupported(
                "MetalFX temporal scale queries require macOS 14 or newer",
            ));
        }
        Ok(MTLFXTemporalScalerDescriptor::supportedInputContentMaxScaleForDevice(&device.inner))
    }

    /// Creates the scaler, or returns a platform capability error.
    pub fn new_scaler(&self, device: &Device) -> Result<TemporalScaler, Error> {
        self.inner
            .newTemporalScalerWithDevice(&device.inner)
            .map(|inner| TemporalScaler {
                inner,
                _thread_bound: ThreadBound::new(),
            })
            .ok_or_else(|| Error::unsupported("MetalFX temporal scaling is unavailable"))
    }

    /// Creates an MTL4FX temporal scaler using the supplied compiler.
    pub fn new_mtl4_scaler(
        &self,
        device: &Device,
        compiler: &objc2::runtime::AnyObject,
    ) -> Result<Retained<objc2::runtime::AnyObject>, Error> {
        let value: Option<Retained<objc2::runtime::AnyObject>> = unsafe {
            objc2::msg_send![&*self.inner, newTemporalScalerWithDevice: &*device.inner, compiler: compiler]
        };
        value.ok_or_else(|| Error::unsupported("Metal 4 Temporal scaling is unavailable"))
    }
}

/// An owned MetalFX temporal scaler.
pub struct TemporalScaler {
    inner: Retained<ProtocolObject<dyn MTLFXTemporalScaler>>,
    _thread_bound: ThreadBound,
}

impl TemporalScaler {
    /// Encodes the temporal scaling operation into a command buffer.
    pub fn encode(&self, command_buffer: &CommandBuffer) {
        self.inner.encodeToCommandBuffer(&command_buffer.inner);
    }

    /// Returns the input texture usage required by the scaler.
    #[must_use]
    pub fn color_texture_usage(&self) -> u64 {
        self.inner.colorTextureUsage().0 as u64
    }

    /// Returns the output texture usage required by the scaler.
    #[must_use]
    pub fn output_texture_usage(&self) -> u64 {
        self.inner.outputTextureUsage().0 as u64
    }

    /// Returns the required input depth texture usage bitmask.
    #[must_use]
    pub fn depth_texture_usage(&self) -> u64 {
        self.inner.depthTextureUsage().0 as u64
    }

    /// Returns the required input motion texture usage bitmask.
    #[must_use]
    pub fn motion_texture_usage(&self) -> u64 {
        self.inner.motionTextureUsage().0 as u64
    }

    /// Returns currently assigned color, depth, motion, and output textures.
    #[must_use]
    pub fn textures(
        &self,
    ) -> (
        Option<Texture>,
        Option<Texture>,
        Option<Texture>,
        Option<Texture>,
    ) {
        (
            self.inner.colorTexture().map(Texture::new),
            self.inner.depthTexture().map(Texture::new),
            self.inner.motionTexture().map(Texture::new),
            self.inner.outputTexture().map(Texture::new),
        )
    }

    /// Returns the assigned exposure texture.
    #[must_use]
    pub fn exposure_texture(&self) -> Option<Texture> {
        self.inner.exposureTexture().map(Texture::new)
    }

    /// Sets the optional exposure texture.
    pub fn set_exposure_texture(&self, value: Option<&Texture>) {
        self.inner
            .setExposureTexture(value.map(|texture| &*texture.inner));
    }

    /// Returns the assigned reactive-mask texture.
    #[must_use]
    pub fn reactive_mask_texture(&self) -> Option<Texture> {
        self.inner.reactiveMaskTexture().map(Texture::new)
    }

    /// Sets the optional reactive-mask texture.
    pub fn set_reactive_mask_texture(&self, value: Option<&Texture>) {
        self.inner
            .setReactiveMaskTexture(value.map(|texture| &*texture.inner));
    }

    /// Returns the deprecated reactive texture usage for compatibility queries.
    #[must_use]
    pub fn reactive_texture_usage(&self) -> u64 {
        self.inner.reactiveTextureUsage().0 as u64
    }

    /// Sets whether depth values are reversed.
    pub fn set_depth_reversed(&self, value: bool) {
        self.inner.setDepthReversed(value);
    }

    /// Sets the input content dimensions after checking they are non-zero.
    pub fn set_input_content_size(&self, width: usize, height: usize) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "MetalFX input content dimensions must be greater than zero",
            ));
        }
        self.inner.setInputContentWidth(width);
        self.inner.setInputContentHeight(height);
        Ok(())
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
        if let Some(texture) = output
            && texture.storage_mode() != StorageMode::Private
        {
            return Err(Error::invalid_argument(
                "MetalFX output texture must use private storage",
            ));
        }
        self.inner
            .setColorTexture(color.map(|texture| &*texture.inner));
        self.inner
            .setDepthTexture(depth.map(|texture| &*texture.inner));
        self.inner
            .setMotionTexture(motion.map(|texture| &*texture.inner));
        self.inner
            .setOutputTexture(output.map(|texture| &*texture.inner));
        Ok(())
    }
}
