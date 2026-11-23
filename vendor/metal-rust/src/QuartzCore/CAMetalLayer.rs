//! Safe Core Animation Metal layer and drawable wrappers.

use crate::{Device, Error, PixelFormat, Texture};
use std::ffi::c_void;
use std::ptr::NonNull;

/// An owned Core Graphics color space accepted by `CAMetalLayer`.
pub struct ColorSpace {
    inner: metal_rust_ffi::ColorSpace,
}

impl ColorSpace {
    /// Creates the platform device RGB color space.
    pub fn device_rgb() -> Result<Self, Error> {
        metal_rust_ffi::ColorSpace::device_rgb()
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
}

/// An owned `CAMetalLayer`.
pub struct Layer {
    pub(crate) inner: metal_rust_ffi::Layer,
}

impl Layer {
    /// Creates a new Metal layer.
    pub fn new() -> Result<Self, Error> {
        metal_rust_ffi::Layer::new()
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a layer associated with a device.
    pub fn with_device(device: &Device) -> Result<Self, Error> {
        metal_rust_ffi::Layer::with_device(&device.inner)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns this layer's borrowed Objective-C object pointer.
    ///
    /// The pointer remains valid only while this layer is alive.
    #[must_use]
    pub fn as_ptr(&self) -> NonNull<c_void> {
        self.inner.as_ptr()
    }

    /// Sets the layer's Metal device.
    pub fn set_device(&self, device: Option<&Device>) {
        self.inner.set_device(device.map(|device| &device.inner));
    }

    /// Returns the current device, if configured.
    #[must_use]
    pub fn device(&self) -> Option<Device> {
        self.inner.device().map(Device::from_ffi)
    }

    /// Sets the pixel format used for future drawables.
    pub fn set_pixel_format(&self, pixel_format: PixelFormat) {
        self.inner.set_pixel_format(pixel_format);
    }

    /// Returns the drawable pixel format, preserving unknown future values.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        self.inner.pixel_format()
    }

    /// Returns whether drawable textures are framebuffer-only.
    #[must_use]
    pub fn framebuffer_only(&self) -> bool {
        self.inner.framebuffer_only()
    }

    /// Sets whether drawable textures are framebuffer-only.
    pub fn set_framebuffer_only(&self, value: bool) {
        self.inner.set_framebuffer_only(value);
    }

    /// Returns drawable width and height.
    #[must_use]
    pub fn drawable_size(&self) -> (f64, f64) {
        self.inner.drawable_size()
    }

    /// Sets finite, non-negative drawable width and height.
    pub fn set_drawable_size(&self, width: f64, height: f64) -> Result<(), Error> {
        self.inner
            .set_drawable_size(width, height)
            .map_err(Error::from_ffi)
    }

    /// Returns the maximum drawable count.
    #[must_use]
    pub fn maximum_drawable_count(&self) -> usize {
        self.inner.maximum_drawable_count()
    }

    /// Sets the maximum drawable count to 2 or 3.
    pub fn set_maximum_drawable_count(&self, count: usize) -> Result<(), Error> {
        self.inner
            .set_maximum_drawable_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns whether display synchronization is enabled.
    #[must_use]
    pub fn display_sync_enabled(&self) -> bool {
        self.inner.display_sync_enabled()
    }

    /// Sets whether display synchronization is enabled.
    pub fn set_display_sync_enabled(&self, value: bool) {
        self.inner.set_display_sync_enabled(value);
    }

    /// Returns the drawable color space.
    #[must_use]
    pub fn color_space(&self) -> Option<ColorSpace> {
        self.inner.color_space().map(|inner| ColorSpace { inner })
    }

    /// Sets or clears the drawable color space.
    pub fn set_color_space(&self, value: Option<&ColorSpace>) {
        self.inner.set_color_space(value.map(|value| &value.inner));
    }

    /// Returns whether acquiring a drawable may time out.
    #[must_use]
    pub fn allows_next_drawable_timeout(&self) -> bool {
        self.inner.allows_next_drawable_timeout()
    }

    /// Sets whether acquiring a drawable may time out.
    pub fn set_allows_next_drawable_timeout(&self, value: bool) {
        self.inner.set_allows_next_drawable_timeout(value);
    }

    /// Returns whether drawable content uses extended dynamic range.
    #[must_use]
    pub fn wants_extended_dynamic_range_content(&self) -> bool {
        self.inner.wants_extended_dynamic_range_content()
    }

    /// Sets whether drawable content uses extended dynamic range.
    pub fn set_wants_extended_dynamic_range_content(&self, value: bool) {
        self.inner.set_wants_extended_dynamic_range_content(value);
    }

    /// Returns the layer's residency set when supported by the runtime.
    pub fn residency_set(&self) -> Result<crate::metal::ResidencySet, Error> {
        self.inner
            .residency_set()
            .map(crate::metal::ResidencySet::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Acquires the next drawable.
    pub fn next_drawable(&self) -> Result<Drawable, Error> {
        self.inner
            .next_drawable()
            .map(|inner| Drawable { inner })
            .map_err(Error::from_ffi)
    }
}

/// An owned drawable obtained from a Metal layer.
pub struct Drawable {
    pub(crate) inner: metal_rust_ffi::Drawable,
}

impl Drawable {
    /// Returns the drawable's texture.
    #[must_use]
    pub fn texture(&self) -> Texture {
        Texture::from_ffi(self.inner.texture())
    }
}
