//! Audited Core Animation and Metal drawable boundary.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::{Device, PixelFormat, Texture};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_core_foundation::CGSize;
use objc2_core_graphics::CGColorSpace;
use objc2_foundation::{NSObjectProtocol, NSThread};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use std::ffi::c_void;
use std::ptr::NonNull;

/// An owned `CAMetalLayer`.
pub struct Layer {
    pub(super) inner: Retained<CAMetalLayer>,
    _thread_bound: ThreadBound,
}

/// An owned Core Graphics color space accepted by `CAMetalLayer`.
pub struct ColorSpace {
    inner: Retained<CGColorSpace>,
    _thread_bound: ThreadBound,
}

impl ColorSpace {
    /// Creates the platform device RGB color space.
    pub fn device_rgb() -> Result<Self, Error> {
        CGColorSpace::new_device_rgb()
            .map(|inner| Self {
                inner: inner.into(),
                _thread_bound: ThreadBound::new(),
            })
            .ok_or_else(|| Error::unsupported("Core Graphics could not create device RGB"))
    }
}

impl Layer {
    /// Creates a new Core Animation Metal layer.
    pub fn new() -> Result<Self, Error> {
        if !NSThread::isMainThread_class() {
            return Err(Error::invalid_argument(
                "CAMetalLayer must be created on the main thread",
            ));
        }
        Ok(Self {
            inner: CAMetalLayer::layer(),
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Creates a layer associated with a specific Metal device.
    pub fn with_device(device: &Device) -> Result<Self, Error> {
        let layer = Self::new()?;
        layer.inner.setDevice(Some(&device.inner));
        Ok(layer)
    }

    /// Returns this layer's borrowed Objective-C object pointer.
    ///
    /// The pointer remains valid only while this layer is alive.
    #[must_use]
    pub fn as_ptr(&self) -> NonNull<c_void> {
        NonNull::from(&*self.inner).cast()
    }

    /// Sets the Metal device used for drawable allocation.
    pub fn set_device(&self, device: Option<&Device>) {
        self.inner.setDevice(device.map(|device| &*device.inner));
    }

    /// Returns the layer's current device, if one has been selected.
    #[must_use]
    pub fn device(&self) -> Option<Device> {
        self.inner.device().map(Device::from_inner)
    }

    /// Sets the drawable pixel format.
    pub fn set_pixel_format(&self, pixel_format: PixelFormat) {
        self.inner.setPixelFormat(pixel_format.as_objc());
    }

    /// Returns the drawable pixel format, preserving unknown future values.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.pixelFormat().0)
    }

    /// Returns whether drawable textures are framebuffer-only.
    #[must_use]
    pub fn framebuffer_only(&self) -> bool {
        self.inner.framebufferOnly()
    }

    /// Sets whether drawable textures are framebuffer-only.
    pub fn set_framebuffer_only(&self, value: bool) {
        self.inner.setFramebufferOnly(value);
    }

    /// Returns the drawable dimensions.
    #[must_use]
    pub fn drawable_size(&self) -> (f64, f64) {
        let value = self.inner.drawableSize();
        (value.width, value.height)
    }

    /// Sets finite, non-negative drawable dimensions.
    pub fn set_drawable_size(&self, width: f64, height: f64) -> Result<(), Error> {
        if !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0 {
            return Err(Error::invalid_argument(
                "drawable width and height must be finite and non-negative",
            ));
        }
        self.inner.setDrawableSize(CGSize::new(width, height));
        Ok(())
    }

    /// Returns the maximum number of drawable objects.
    #[must_use]
    pub fn maximum_drawable_count(&self) -> usize {
        self.inner.maximumDrawableCount()
    }

    /// Sets the maximum drawable count accepted by Core Animation.
    pub fn set_maximum_drawable_count(&self, count: usize) -> Result<(), Error> {
        if !(2..=3).contains(&count) {
            return Err(Error::invalid_argument(
                "maximum drawable count must be either 2 or 3",
            ));
        }
        self.inner.setMaximumDrawableCount(count);
        Ok(())
    }

    /// Returns whether presentation synchronizes with the display.
    #[must_use]
    pub fn display_sync_enabled(&self) -> bool {
        self.inner.displaySyncEnabled()
    }

    /// Sets whether presentation synchronizes with the display.
    pub fn set_display_sync_enabled(&self, value: bool) {
        self.inner.setDisplaySyncEnabled(value);
    }

    /// Returns the current color space.
    #[must_use]
    pub fn color_space(&self) -> Option<ColorSpace> {
        self.inner.colorspace().map(|inner| ColorSpace {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Sets or clears the drawable color space.
    pub fn set_color_space(&self, value: Option<&ColorSpace>) {
        self.inner.setColorspace(value.map(|value| &*value.inner));
    }

    /// Returns whether `nextDrawable` is allowed to time out.
    #[must_use]
    pub fn allows_next_drawable_timeout(&self) -> bool {
        self.inner.allowsNextDrawableTimeout()
    }

    /// Sets whether `nextDrawable` is allowed to time out.
    pub fn set_allows_next_drawable_timeout(&self, value: bool) {
        self.inner.setAllowsNextDrawableTimeout(value);
    }

    /// Returns whether drawable content uses extended dynamic range.
    #[must_use]
    pub fn wants_extended_dynamic_range_content(&self) -> bool {
        self.inner.wantsExtendedDynamicRangeContent()
    }

    /// Sets whether drawable content uses extended dynamic range.
    pub fn set_wants_extended_dynamic_range_content(&self, value: bool) {
        self.inner.setWantsExtendedDynamicRangeContent(value);
    }

    /// Returns the layer's residency set when the runtime exposes it.
    pub fn residency_set(
        &self,
    ) -> Result<crate::metal::generated_object_types::metal::ResidencySet, Error> {
        if !self.inner.respondsToSelector(sel!(residencySet)) {
            return Err(Error::unsupported(
                "CAMetalLayer::residencySet is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the SDK declares an
        // Objective-C object result, which is retained by the owned wrapper.
        let value: Option<Retained<AnyObject>> = unsafe { msg_send![&*self.inner, residencySet] };
        value
            .map(crate::metal::generated_object_types::metal::ResidencySet::from_inner)
            .ok_or_else(|| Error::unsupported("CAMetalLayer returned no residency set"))
    }

    /// Returns the next drawable, if the layer can currently provide one.
    pub fn next_drawable(&self) -> Result<Drawable, Error> {
        self.inner
            .nextDrawable()
            .map(Drawable::new)
            .ok_or_else(|| Error::unsupported("Core Animation could not provide a drawable"))
    }
}

/// An owned drawable obtained from a `CAMetalLayer`.
pub struct Drawable {
    pub(crate) inner: Retained<ProtocolObject<dyn CAMetalDrawable>>,
    _thread_bound: ThreadBound,
}

impl Drawable {
    pub(crate) const fn new(inner: Retained<ProtocolObject<dyn CAMetalDrawable>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the texture backing this drawable.
    #[must_use]
    pub fn texture(&self) -> Texture {
        Texture::new(self.inner.texture())
    }
}
