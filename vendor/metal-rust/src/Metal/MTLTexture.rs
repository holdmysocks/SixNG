//! Safe Metal texture resources and descriptors.

use super::{
    CPUCacheMode, Device, HazardTrackingMode, PixelFormat, Region, Resource, ResourceID,
    ResourceOptions, SharedTextureHandle, SparsePageSize, StorageMode, TextureCompressionType,
    TextureSparseTier, TextureSwizzleChannels, TextureType, TextureUsage, TextureViewDescriptor,
};
use crate::Error;

/// A validated descriptor for a two-dimensional Metal texture.
pub struct TextureDescriptor {
    pub(crate) inner: metal_rust_ffi::TextureDescriptor,
}

impl TextureDescriptor {
    /// Creates a descriptor using Metal defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: metal_rust_ffi::TextureDescriptor::new(),
        }
    }

    /// Creates a 2D descriptor with validated dimensions and allocation hints.
    pub fn new_2d(
        pixel_format: PixelFormat,
        width: usize,
        height: usize,
        storage: ResourceOptions,
        usage: TextureUsage,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TextureDescriptor::new_2d(pixel_format, width, height, storage, usage)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a validated 2D descriptor using Metal's convenience factory.
    pub fn texture_2d(
        pixel_format: PixelFormat,
        width: usize,
        height: usize,
        mipmapped: bool,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TextureDescriptor::texture_2d(pixel_format, width, height, mipmapped)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a validated cube descriptor using Metal's convenience factory.
    pub fn texture_cube(
        pixel_format: PixelFormat,
        size: usize,
        mipmapped: bool,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TextureDescriptor::texture_cube(pixel_format, size, mipmapped)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a validated linear texture-buffer descriptor.
    pub fn texture_buffer(
        pixel_format: PixelFormat,
        width: usize,
        resource_options: ResourceOptions,
        usage: TextureUsage,
    ) -> Result<Self, Error> {
        metal_rust_ffi::TextureDescriptor::texture_buffer(
            pixel_format,
            width,
            resource_options,
            usage,
        )
        .map(|inner| Self { inner })
        .map_err(Error::from_ffi)
    }

    /// Sets the number of mip levels.
    pub fn set_mipmap_level_count(&self, count: usize) -> Result<(), Error> {
        self.inner
            .set_mipmap_level_count(count)
            .map_err(Error::from_ffi)
    }

    /// Returns width, height, and depth.
    #[must_use]
    pub fn dimensions(&self) -> (usize, usize, usize) {
        self.inner.dimensions()
    }

    /// Sets non-zero width, height, and depth.
    pub fn set_dimensions(&self, width: usize, height: usize, depth: usize) -> Result<(), Error> {
        self.inner
            .set_dimensions(width, height, depth)
            .map_err(Error::from_ffi)
    }

    /// Returns and sets the array length.
    #[must_use]
    pub fn array_length(&self) -> usize {
        self.inner.array_length()
    }

    /// Sets a non-zero array length.
    pub fn set_array_length(&self, value: usize) -> Result<(), Error> {
        self.inner.set_array_length(value).map_err(Error::from_ffi)
    }

    /// Returns the sample count.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.inner.sample_count()
    }

    /// Sets a non-zero sample count.
    pub fn set_sample_count(&self, value: usize) -> Result<(), Error> {
        self.inner.set_sample_count(value).map_err(Error::from_ffi)
    }

    /// Returns the mipmap level count.
    #[must_use]
    pub fn mipmap_level_count(&self) -> usize {
        self.inner.mipmap_level_count()
    }

    /// Returns the pixel format.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        self.inner.pixel_format()
    }

    /// Sets the pixel format.
    pub fn set_pixel_format(&self, value: PixelFormat) {
        self.inner.set_pixel_format(value);
    }

    /// Returns the texture type.
    pub fn texture_type(&self) -> Result<TextureType, Error> {
        self.inner.texture_type().map_err(Error::from_ffi)
    }

    /// Sets the texture type.
    pub fn set_texture_type(&self, value: TextureType) {
        self.inner.set_texture_type(value);
    }

    /// Returns resource options.
    #[must_use]
    pub fn resource_options(&self) -> ResourceOptions {
        self.inner.resource_options()
    }

    /// Sets resource options.
    pub fn set_resource_options(&self, value: ResourceOptions) {
        self.inner.set_resource_options(value);
    }

    /// Returns texture usage flags.
    #[must_use]
    pub fn usage(&self) -> TextureUsage {
        self.inner.usage()
    }

    /// Sets texture usage flags.
    pub fn set_usage(&self, value: TextureUsage) {
        self.inner.set_usage(value);
    }

    /// Returns the storage mode.
    pub fn storage_mode(&self) -> Result<StorageMode, Error> {
        self.inner.storage_mode().map_err(Error::from_ffi)
    }

    /// Sets the storage mode.
    pub fn set_storage_mode(&self, value: StorageMode) {
        self.inner.set_storage_mode(value);
    }

    /// Returns the CPU cache mode.
    #[must_use]
    pub fn cpu_cache_mode(&self) -> CPUCacheMode {
        self.inner.cpu_cache_mode()
    }

    /// Sets the CPU cache mode.
    pub fn set_cpu_cache_mode(&self, value: CPUCacheMode) {
        self.inner.set_cpu_cache_mode(value);
    }

    /// Returns the hazard-tracking mode.
    #[must_use]
    pub fn hazard_tracking_mode(&self) -> HazardTrackingMode {
        self.inner.hazard_tracking_mode()
    }

    /// Sets the hazard-tracking mode.
    pub fn set_hazard_tracking_mode(&self, value: HazardTrackingMode) {
        self.inner.set_hazard_tracking_mode(value);
    }

    /// Returns the compression type.
    #[must_use]
    pub fn compression_type(&self) -> TextureCompressionType {
        self.inner.compression_type()
    }

    /// Sets the compression type.
    pub fn set_compression_type(&self, value: TextureCompressionType) {
        self.inner.set_compression_type(value);
    }

    /// Returns the sparse placement page size.
    #[must_use]
    pub fn placement_sparse_page_size(&self) -> SparsePageSize {
        self.inner.placement_sparse_page_size()
    }

    /// Sets the sparse placement page size.
    pub fn set_placement_sparse_page_size(&self, value: SparsePageSize) {
        self.inner.set_placement_sparse_page_size(value);
    }

    /// Returns whether GPU-optimized contents are allowed.
    #[must_use]
    pub fn allows_gpu_optimized_contents(&self) -> bool {
        self.inner.allows_gpu_optimized_contents()
    }

    /// Sets whether GPU-optimized contents are allowed.
    pub fn set_allows_gpu_optimized_contents(&self, value: bool) {
        self.inner.set_allows_gpu_optimized_contents(value);
    }

    /// Returns the descriptor's channel swizzle.
    #[must_use]
    pub fn swizzle(&self) -> TextureSwizzleChannels {
        self.inner.swizzle()
    }

    /// Sets the descriptor's channel swizzle after validation.
    pub fn set_swizzle(&self, value: &TextureSwizzleChannels) -> Result<(), Error> {
        self.inner.set_swizzle(value).map_err(Error::from_ffi)
    }
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// An owned Metal texture.
#[derive(Clone)]
pub struct Texture {
    pub(crate) inner: metal_rust_ffi::Texture,
}

/// An owned, opaque IOSurface identity backing a Metal texture.
#[derive(Clone)]
pub struct IoSurface {
    #[allow(dead_code)]
    inner: metal_rust_ffi::IoSurface,
}

impl IoSurface {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::IoSurface) -> Self {
        Self { inner }
    }

    pub(crate) const fn as_ffi(&self) -> &metal_rust_ffi::IoSurface {
        &self.inner
    }
}

impl Texture {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Texture) -> Self {
        Self { inner }
    }

    /// Returns the texture width.
    #[must_use]
    pub fn width(&self) -> usize {
        self.inner.width()
    }

    /// Returns the texture height.
    #[must_use]
    pub fn height(&self) -> usize {
        self.inner.height()
    }

    /// Returns depth, array length, mip levels, and sample count.
    #[must_use]
    pub fn layout(&self) -> (usize, usize, usize, usize) {
        self.inner.layout()
    }

    /// Returns the checked texture type.
    pub fn texture_type(&self) -> Result<TextureType, Error> {
        self.inner.texture_type().map_err(Error::from_ffi)
    }

    /// Returns texture usage flags.
    #[must_use]
    pub fn usage(&self) -> TextureUsage {
        self.inner.usage()
    }

    /// Returns whether this texture is framebuffer-only.
    #[must_use]
    pub fn is_framebuffer_only(&self) -> bool {
        self.inner.is_framebuffer_only()
    }

    /// Returns whether this texture is shareable.
    #[must_use]
    pub fn is_shareable(&self) -> bool {
        self.inner.is_shareable()
    }

    /// Returns whether this texture is sparse.
    #[must_use]
    pub fn is_sparse(&self) -> bool {
        self.inner.is_sparse()
    }

    /// Returns whether GPU-optimized contents are allowed.
    #[must_use]
    pub fn allows_gpu_optimized_contents(&self) -> bool {
        self.inner.allows_gpu_optimized_contents()
    }

    /// Returns the compression type.
    #[must_use]
    pub fn compression_type(&self) -> TextureCompressionType {
        self.inner.compression_type()
    }

    /// Returns the sparse-texture tier.
    #[must_use]
    pub fn sparse_texture_tier(&self) -> TextureSparseTier {
        self.inner.sparse_texture_tier()
    }

    /// Returns the parent texture, if this texture is a view.
    #[must_use]
    pub fn parent_texture(&self) -> Option<Self> {
        self.inner.parent_texture().map(Self::from_ffi)
    }

    /// Returns parent-relative mip level and slice.
    #[must_use]
    pub fn parent_relative_location(&self) -> (usize, usize) {
        self.inner.parent_relative_location()
    }

    /// Returns the backing buffer for a buffer texture.
    #[must_use]
    pub fn buffer(&self) -> Option<super::Buffer> {
        self.inner.buffer().map(super::Buffer::from_ffi)
    }

    /// Returns backing-buffer offset and row stride.
    #[must_use]
    pub fn buffer_layout(&self) -> (usize, usize) {
        self.inner.buffer_layout()
    }

    /// Returns the first sparse-tail mip and tail byte size.
    #[must_use]
    pub fn sparse_tail(&self) -> (usize, usize) {
        self.inner.sparse_tail()
    }

    /// Returns the texture pixel format, preserving future system values.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        self.inner.pixel_format()
    }

    /// Returns the texture storage mode.
    #[must_use]
    pub fn storage_mode(&self) -> StorageMode {
        self.inner.storage_mode()
    }

    /// Returns the opaque GPU resource identifier used by argument tables.
    #[must_use]
    pub fn gpu_resource_id(&self) -> ResourceID {
        self.inner.gpu_resource_id()
    }

    /// Returns the IOSurface plane index. Non-IOSurface textures report zero.
    #[must_use]
    pub fn iosurface_plane(&self) -> usize {
        self.inner.iosurface_plane()
    }

    /// Returns the owned IOSurface identity backing this texture, if any.
    #[must_use]
    pub fn iosurface(&self) -> Option<IoSurface> {
        self.inner.iosurface().map(IoSurface::from_ffi)
    }

    /// Returns the deprecated root resource as an opaque safe wrapper.
    #[allow(deprecated)]
    pub fn root_resource(&self) -> Option<Resource> {
        self.inner.root_resource().map(Resource::from_ffi)
    }

    /// Returns the source texture for a remote-storage view, if present.
    #[must_use]
    pub fn remote_storage_texture(&self) -> Option<Self> {
        self.inner.remote_storage_texture().map(Self::from_ffi)
    }

    /// Creates a remote view for another device.
    pub fn new_remote_view(&self, device: &Device) -> Result<Self, Error> {
        self.inner
            .new_remote_view(&device.inner)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an interprocess shared texture handle.
    pub fn new_shared_texture_handle(&self) -> Result<SharedTextureHandle, Error> {
        self.inner
            .new_shared_texture_handle()
            .map(SharedTextureHandle::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a compatible-format texture view.
    pub fn view_with_pixel_format(&self, pixel_format: PixelFormat) -> Result<Self, Error> {
        self.inner
            .view_with_pixel_format(pixel_format)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked texture view over mip-level and slice ranges.
    pub fn view(
        &self,
        pixel_format: PixelFormat,
        texture_type: TextureType,
        levels: std::ops::Range<usize>,
        slices: std::ops::Range<usize>,
    ) -> Result<Self, Error> {
        self.inner
            .view(pixel_format, texture_type, levels, slices)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a checked texture view with an explicit channel swizzle.
    pub fn view_with_swizzle(
        &self,
        pixel_format: PixelFormat,
        texture_type: TextureType,
        levels: std::ops::Range<usize>,
        slices: std::ops::Range<usize>,
        swizzle: &TextureSwizzleChannels,
    ) -> Result<Self, Error> {
        self.inner
            .view_with_swizzle(pixel_format, texture_type, levels, slices, swizzle)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a texture view from a safe descriptor.
    pub fn view_with_descriptor(&self, descriptor: &TextureViewDescriptor) -> Result<Self, Error> {
        self.inner
            .view_with_descriptor(&descriptor.inner)
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Copies bytes into a checked 2D texture region.
    pub fn replace_region_2d(
        &self,
        region: Region,
        mipmap_level: usize,
        bytes: &[u8],
        bytes_per_row: usize,
    ) -> Result<(), Error> {
        self.inner
            .replace_region_2d(region, mipmap_level, bytes, bytes_per_row)
            .map_err(Error::from_ffi)
    }

    /// Copies bytes into a checked texture region and array slice.
    pub fn replace_region(
        &self,
        region: Region,
        mipmap_level: usize,
        slice: usize,
        bytes: &[u8],
        bytes_per_row: usize,
        bytes_per_image: usize,
    ) -> Result<(), Error> {
        self.inner
            .replace_region(
                region,
                mipmap_level,
                slice,
                bytes,
                bytes_per_row,
                bytes_per_image,
            )
            .map_err(Error::from_ffi)
    }

    /// Returns this texture's channel swizzle.
    #[must_use]
    pub fn swizzle(&self) -> TextureSwizzleChannels {
        self.inner.swizzle()
    }
}
