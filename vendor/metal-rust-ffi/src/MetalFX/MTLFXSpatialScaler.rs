//! Audited MetalFX spatial scaler binding.
#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::{CommandBuffer, Device, PixelFormat, StorageMode, Texture};
use crate::metal_fx::SpatialScalerColorProcessingMode;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, ProtocolObject};
use objc2::{extern_class, extern_conformance, extern_methods, extern_protocol};
use objc2_foundation::{NSCopying, NSObject, NSObjectProtocol};
use objc2_metal::{MTLCommandBuffer, MTLDevice, MTLTexture, MTLTextureUsage};

extern_protocol!(
    /// MetalFX spatial scaler protocol declared by the macOS SDK.
    ///
    /// # Safety
    ///
    /// This trait is implemented only by the Objective-C runtime object whose
    /// selectors and return layouts match the SDK protocol declaration.
    unsafe trait MTLFXSpatialScaler: NSObjectProtocol {
        #[unsafe(method(encodeToCommandBuffer:))]
        #[unsafe(method_family = none)]
        fn encodeToCommandBuffer(&self, command_buffer: &ProtocolObject<dyn MTLCommandBuffer>);

        #[unsafe(method(colorTextureUsage))]
        #[unsafe(method_family = none)]
        fn colorTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(outputTextureUsage))]
        #[unsafe(method_family = none)]
        fn outputTextureUsage(&self) -> MTLTextureUsage;

        #[unsafe(method(colorTexture))]
        #[unsafe(method_family = none)]
        fn colorTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;

        #[unsafe(method(outputTexture))]
        #[unsafe(method_family = none)]
        fn outputTexture(&self) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;

        #[unsafe(method(colorProcessingMode))]
        #[unsafe(method_family = none)]
        fn colorProcessingMode(&self) -> isize;

        #[unsafe(method(setInputContentWidth:))]
        #[unsafe(method_family = none)]
        fn setInputContentWidth(&self, width: usize);

        #[unsafe(method(setInputContentHeight:))]
        #[unsafe(method_family = none)]
        fn setInputContentHeight(&self, height: usize);

        #[unsafe(method(setColorTexture:))]
        #[unsafe(method_family = none)]
        fn setColorTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);

        #[unsafe(method(setOutputTexture:))]
        #[unsafe(method_family = none)]
        fn setOutputTexture(&self, texture: Option<&ProtocolObject<dyn MTLTexture>>);
    }
);

extern_class!(
    /// MetalFX spatial scaler descriptor declared by the macOS SDK.
    #[unsafe(super(NSObject))]
    pub struct MTLFXSpatialScalerDescriptor;
);

extern_conformance!(
    unsafe impl NSObjectProtocol for MTLFXSpatialScalerDescriptor {}
);
extern_conformance!(
    unsafe impl NSCopying for MTLFXSpatialScalerDescriptor {}
);

impl MTLFXSpatialScalerDescriptor {
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

        #[unsafe(method(colorProcessingMode))]
        #[unsafe(method_family = none)]
        pub fn colorProcessingMode(&self) -> isize;

        #[unsafe(method(setColorProcessingMode:))]
        #[unsafe(method_family = none)]
        pub fn setColorProcessingMode(&self, mode: isize);

        #[unsafe(method(newSpatialScalerWithDevice:))]
        #[unsafe(method_family = new)]
        pub fn newSpatialScalerWithDevice(
            &self,
            device: &ProtocolObject<dyn MTLDevice>,
        ) -> Option<Retained<ProtocolObject<dyn MTLFXSpatialScaler>>>;

        #[unsafe(method(supportsDevice:))]
        #[unsafe(method_family = none)]
        pub fn supportsDevice(device: &ProtocolObject<dyn MTLDevice>) -> bool;

        #[unsafe(method(supportsMetal4FX:))]
        #[unsafe(method_family = none)]
        pub fn supportsMetal4FX(device: &ProtocolObject<dyn MTLDevice>) -> bool;
    );
}

/// An owned spatial-scaler descriptor.
pub struct SpatialScalerDescriptor {
    inner: Retained<MTLFXSpatialScalerDescriptor>,
    _thread_bound: ThreadBound,
}

impl SpatialScalerDescriptor {
    /// Creates a descriptor when the MetalFX class is present at runtime.
    pub fn new() -> Result<Self, Error> {
        if AnyClass::get(c"MTLFXSpatialScalerDescriptor").is_none() {
            return Err(Error::unsupported(
                "MetalFX spatial scaling is unavailable on this system",
            ));
        }
        Ok(Self {
            inner: MTLFXSpatialScalerDescriptor::new(),
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Sets the input and output formats and dimensions.
    pub fn configure(
        &self,
        color_format: PixelFormat,
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
        self.inner.setOutputTextureFormat(output_format.as_objc());
        self.inner.setInputWidth(input_width);
        self.inner.setInputHeight(input_height);
        self.inner.setOutputWidth(output_width);
        self.inner.setOutputHeight(output_height);
        Ok(())
    }

    /// Returns the configured input pixel format.
    #[must_use]
    pub fn color_texture_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.colorTextureFormat().0)
    }

    /// Sets the input pixel format.
    pub fn set_color_texture_format(&self, value: PixelFormat) {
        self.inner.setColorTextureFormat(value.as_objc());
    }

    /// Returns the configured output pixel format.
    #[must_use]
    pub fn output_texture_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.outputTextureFormat().0)
    }

    /// Sets the output pixel format.
    pub fn set_output_texture_format(&self, value: PixelFormat) {
        self.inner.setOutputTextureFormat(value.as_objc());
    }

    /// Returns the configured input dimensions.
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

    /// Returns the configured output dimensions.
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

    /// Returns the configured color-processing mode.
    pub fn color_processing_mode(&self) -> Result<SpatialScalerColorProcessingMode, Error> {
        SpatialScalerColorProcessingMode::try_from(self.inner.colorProcessingMode())
            .map_err(|()| Error::unsupported("MetalFX returned an unknown color-processing mode"))
    }

    /// Sets a validated color-processing mode.
    pub fn set_color_processing_mode(&self, value: SpatialScalerColorProcessingMode) {
        self.inner.setColorProcessingMode(value.as_raw());
    }

    /// Returns whether the device supports the Metal 4 FX implementation.
    #[must_use]
    pub fn supports_metal4_fx(&self, device: &Device) -> bool {
        MTLFXSpatialScalerDescriptor::supportsMetal4FX(&device.inner)
    }

    /// Returns whether the installed MetalFX runtime supports the device.
    #[must_use]
    pub fn supports_device(&self, device: &Device) -> bool {
        MTLFXSpatialScalerDescriptor::supportsDevice(&device.inner)
    }

    /// Creates the scaler, or returns a platform capability error.
    pub fn new_scaler(&self, device: &Device) -> Result<SpatialScaler, Error> {
        self.inner
            .newSpatialScalerWithDevice(&device.inner)
            .map(|inner| SpatialScaler {
                inner,
                _thread_bound: ThreadBound::new(),
            })
            .ok_or_else(|| Error::unsupported("MetalFX spatial scaling is unavailable"))
    }
}

/// An owned MetalFX spatial scaler.
pub struct SpatialScaler {
    inner: Retained<ProtocolObject<dyn MTLFXSpatialScaler>>,
    _thread_bound: ThreadBound,
}

impl SpatialScaler {
    /// Encodes the scaling operation into a command buffer.
    pub fn encode(&self, command_buffer: &CommandBuffer) {
        self.inner.encodeToCommandBuffer(&command_buffer.inner);
    }

    /// Returns the texture usage required for the input texture.
    #[must_use]
    pub fn color_texture_usage(&self) -> u64 {
        self.inner.colorTextureUsage().0 as u64
    }

    /// Returns the texture usage required for the output texture.
    #[must_use]
    pub fn output_texture_usage(&self) -> u64 {
        self.inner.outputTextureUsage().0 as u64
    }

    /// Returns the assigned input texture.
    #[must_use]
    pub fn color_texture(&self) -> Option<Texture> {
        self.inner.colorTexture().map(Texture::new)
    }

    /// Returns the assigned output texture.
    #[must_use]
    pub fn output_texture(&self) -> Option<Texture> {
        self.inner.outputTexture().map(Texture::new)
    }

    /// Returns the scaler color-processing mode.
    pub fn color_processing_mode(&self) -> Result<SpatialScalerColorProcessingMode, Error> {
        SpatialScalerColorProcessingMode::try_from(self.inner.colorProcessingMode())
            .map_err(|()| Error::unsupported("MetalFX returned an unknown color-processing mode"))
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

    /// Assigns input and output textures. The output must use private storage.
    pub fn set_textures(
        &self,
        color: Option<&Texture>,
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
            .setOutputTexture(output.map(|texture| &*texture.inner));
        Ok(())
    }
}
