//! Audited texture descriptor and texture resource boundary.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    Resource, SharedTextureHandle, TextureViewDescriptor,
};
use crate::metal::generated_struct_types::{ResourceID, TextureSwizzleChannels};
use crate::metal::generated_value_types::{
    CPUCacheMode, HazardTrackingMode, SparsePageSize, TextureCompressionType, TextureSparseTier,
    TextureSwizzle,
};
use crate::metal::{
    Buffer, Device, PixelFormat, Region, ResourceOptions, StorageMode, TextureType, TextureUsage,
};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::NSRange;
use objc2_metal::{
    MTLCPUCacheMode, MTLHazardTrackingMode, MTLResource, MTLSparsePageSize, MTLStorageMode,
    MTLTexture, MTLTextureCompressionType, MTLTextureDescriptor, MTLTextureSwizzle,
    MTLTextureSwizzleChannels,
};
use std::ffi::c_void;
use std::ptr::NonNull;

/// A validated descriptor for a two-dimensional Metal texture.
pub struct TextureDescriptor {
    pub(super) inner: Retained<MTLTextureDescriptor>,
    _thread_bound: ThreadBound,
}

impl TextureDescriptor {
    pub(crate) fn as_any_object(&self) -> &objc2::runtime::AnyObject {
        &self.inner
    }
    /// Creates an empty texture descriptor with Metal defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: MTLTextureDescriptor::new(),
            _thread_bound: ThreadBound::new(),
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
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "texture dimensions must be greater than zero",
            ));
        }
        let inner = MTLTextureDescriptor::new();
        inner.setTextureType(TextureType::D2.as_objc());
        inner.setPixelFormat(pixel_format.as_objc());
        // SAFETY: width and height are non-zero usize values and are passed as
        // NSUInteger on the supported Apple targets.
        unsafe {
            inner.setWidth(width);
            inner.setHeight(height);
        }
        inner.setResourceOptions(storage.as_objc());
        inner.setUsage(usage.as_objc());
        Ok(Self {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Creates a validated 2D texture descriptor using Metal's convenience factory.
    pub fn texture_2d(
        pixel_format: PixelFormat,
        width: usize,
        height: usize,
        mipmapped: bool,
    ) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "texture dimensions must be greater than zero",
            ));
        }
        // SAFETY: dimensions are non-zero and the pixel format is represented
        // by Metal's declared MTLPixelFormat value wrapper.
        let inner = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                pixel_format.as_objc(),
                width,
                height,
                mipmapped,
            )
        };
        Ok(Self {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Creates a validated cube texture descriptor using Metal's convenience factory.
    pub fn texture_cube(
        pixel_format: PixelFormat,
        size: usize,
        mipmapped: bool,
    ) -> Result<Self, Error> {
        if size == 0 {
            return Err(Error::invalid_argument(
                "cube texture size must be greater than zero",
            ));
        }
        // SAFETY: size is non-zero and the pixel format is represented by
        // Metal's declared MTLPixelFormat value wrapper.
        let inner = unsafe {
            MTLTextureDescriptor::textureCubeDescriptorWithPixelFormat_size_mipmapped(
                pixel_format.as_objc(),
                size,
                mipmapped,
            )
        };
        Ok(Self {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Creates a validated linear texture-buffer descriptor.
    pub fn texture_buffer(
        pixel_format: PixelFormat,
        width: usize,
        resource_options: ResourceOptions,
        usage: TextureUsage,
    ) -> Result<Self, Error> {
        if width == 0 {
            return Err(Error::invalid_argument(
                "texture-buffer width must be greater than zero",
            ));
        }
        // SAFETY: width is non-zero and all remaining values are validated
        // Metal value wrappers with the SDK-declared ABI.
        let inner = unsafe {
            MTLTextureDescriptor::textureBufferDescriptorWithPixelFormat_width_resourceOptions_usage(
                pixel_format.as_objc(),
                width,
                resource_options.as_objc(),
                usage.as_objc(),
            )
        };
        Ok(Self {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Sets the number of mip levels after validating the value.
    pub fn set_mipmap_level_count(&self, count: usize) -> Result<(), Error> {
        if count == 0 {
            return Err(Error::invalid_argument(
                "mipmap level count must be greater than zero",
            ));
        }
        // SAFETY: the count is non-zero and fits NSUInteger on supported
        // Apple targets.
        unsafe { self.inner.setMipmapLevelCount(count) };
        Ok(())
    }

    /// Returns descriptor dimensions.
    #[must_use]
    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.inner.width(), self.inner.height(), self.inner.depth())
    }

    /// Sets non-zero descriptor dimensions.
    pub fn set_dimensions(&self, width: usize, height: usize, depth: usize) -> Result<(), Error> {
        if width == 0 || height == 0 || depth == 0 {
            return Err(Error::invalid_argument(
                "texture dimensions must be greater than zero",
            ));
        }
        // SAFETY: all dimensions were checked non-zero and fit NSUInteger.
        unsafe {
            self.inner.setWidth(width);
            self.inner.setHeight(height);
            self.inner.setDepth(depth);
        }
        Ok(())
    }

    /// Returns the array length.
    #[must_use]
    pub fn array_length(&self) -> usize {
        self.inner.arrayLength()
    }

    /// Sets a non-zero array length.
    pub fn set_array_length(&self, value: usize) -> Result<(), Error> {
        if value == 0 {
            return Err(Error::invalid_argument("array length must be non-zero"));
        }
        // SAFETY: value is non-zero and fits NSUInteger.
        unsafe { self.inner.setArrayLength(value) };
        Ok(())
    }

    /// Returns the sample count.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.inner.sampleCount()
    }

    /// Sets a non-zero sample count.
    pub fn set_sample_count(&self, value: usize) -> Result<(), Error> {
        if value == 0 {
            return Err(Error::invalid_argument("sample count must be non-zero"));
        }
        // SAFETY: value is non-zero and fits NSUInteger.
        unsafe { self.inner.setSampleCount(value) };
        Ok(())
    }

    /// Returns the mipmap level count.
    #[must_use]
    pub fn mipmap_level_count(&self) -> usize {
        self.inner.mipmapLevelCount()
    }

    /// Returns and sets the pixel format.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.pixelFormat().0)
    }

    /// Sets the pixel format.
    pub fn set_pixel_format(&self, value: PixelFormat) {
        self.inner.setPixelFormat(value.as_objc());
    }

    /// Returns and sets the texture type.
    pub fn texture_type(&self) -> Result<TextureType, Error> {
        TextureType::try_from_system_raw(self.inner.textureType().0)
            .ok_or_else(|| Error::unsupported("Metal returned an unknown texture type"))
    }

    /// Sets the texture type.
    pub fn set_texture_type(&self, value: TextureType) {
        self.inner.setTextureType(value.as_objc());
    }

    /// Returns and sets resource options.
    #[must_use]
    pub fn resource_options(&self) -> ResourceOptions {
        ResourceOptions::from_system_raw(self.inner.resourceOptions().0)
    }

    /// Sets resource options.
    pub fn set_resource_options(&self, value: ResourceOptions) {
        self.inner.setResourceOptions(value.as_objc());
    }

    /// Returns and sets texture usage flags.
    #[must_use]
    pub fn usage(&self) -> TextureUsage {
        TextureUsage::from_system_raw(self.inner.usage().0)
    }

    /// Sets texture usage flags.
    pub fn set_usage(&self, value: TextureUsage) {
        self.inner.setUsage(value.as_objc());
    }

    /// Returns and sets the storage mode.
    pub fn storage_mode(&self) -> Result<StorageMode, Error> {
        StorageMode::try_from_system_raw(self.inner.storageMode().0)
            .ok_or_else(|| Error::unsupported("Metal returned an unknown storage mode"))
    }

    /// Sets the storage mode.
    pub fn set_storage_mode(&self, value: StorageMode) {
        self.inner.setStorageMode(MTLStorageMode(value.as_raw()));
    }

    /// Returns and sets the CPU cache mode.
    #[must_use]
    pub fn cpu_cache_mode(&self) -> CPUCacheMode {
        CPUCacheMode::from_system_raw(self.inner.cpuCacheMode().0)
    }

    /// Sets the CPU cache mode.
    pub fn set_cpu_cache_mode(&self, value: CPUCacheMode) {
        self.inner.setCpuCacheMode(MTLCPUCacheMode(value.as_raw()));
    }

    /// Returns and sets the hazard tracking mode.
    #[must_use]
    pub fn hazard_tracking_mode(&self) -> HazardTrackingMode {
        HazardTrackingMode::from_system_raw(self.inner.hazardTrackingMode().0)
    }

    /// Sets the hazard-tracking mode.
    pub fn set_hazard_tracking_mode(&self, value: HazardTrackingMode) {
        self.inner
            .setHazardTrackingMode(MTLHazardTrackingMode(value.as_raw()));
    }

    /// Returns and sets the compression type.
    #[must_use]
    pub fn compression_type(&self) -> TextureCompressionType {
        TextureCompressionType::from_system_raw(self.inner.compressionType().0)
    }

    /// Sets the compression type.
    pub fn set_compression_type(&self, value: TextureCompressionType) {
        self.inner
            .setCompressionType(MTLTextureCompressionType(value.as_raw()));
    }

    /// Returns and sets the sparse placement page size.
    #[must_use]
    pub fn placement_sparse_page_size(&self) -> SparsePageSize {
        SparsePageSize::from_system_raw(self.inner.placementSparsePageSize().0)
    }

    /// Sets the sparse placement page size.
    pub fn set_placement_sparse_page_size(&self, value: SparsePageSize) {
        self.inner
            .setPlacementSparsePageSize(MTLSparsePageSize(value.as_raw()));
    }

    /// Returns whether GPU-optimized contents are allowed.
    #[must_use]
    pub fn allows_gpu_optimized_contents(&self) -> bool {
        self.inner.allowGPUOptimizedContents()
    }

    /// Sets whether GPU-optimized contents are allowed.
    pub fn set_allows_gpu_optimized_contents(&self, value: bool) {
        self.inner.setAllowGPUOptimizedContents(value);
    }

    /// Returns the descriptor's channel swizzle.
    #[must_use]
    pub fn swizzle(&self) -> TextureSwizzleChannels {
        swizzle_from_objc(self.inner.swizzle())
    }

    /// Sets the descriptor's channel swizzle after validating every channel.
    pub fn set_swizzle(&self, value: &TextureSwizzleChannels) -> Result<(), Error> {
        self.inner.setSwizzle(swizzle_to_objc(value)?);
        Ok(())
    }
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// An owned Metal texture resource.
#[derive(Clone)]
pub struct Texture {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLTexture>>,
    _thread_bound: ThreadBound,
}

/// An owned, opaque IOSurface identity backing a Metal texture.
///
/// The Objective-C/CoreFoundation object remains private; callers can clone
/// this handle without managing retain counts or raw pointers.
#[derive(Clone)]
pub struct IoSurface {
    #[allow(dead_code)]
    inner: Retained<AnyObject>,
    _thread_bound: ThreadBound,
}

impl IoSurface {
    pub(crate) fn as_inner(&self) -> &AnyObject {
        &self.inner
    }
}

impl Texture {
    pub(crate) const fn new(inner: Retained<ProtocolObject<dyn MTLTexture>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: callers use this only for Objective-C selectors whose SDK
        // declaration returns an object conforming to MTLTexture.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(Self::new(inner))
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: ProtocolObject is a transparent type erasure around the same
        // Objective-C object and MTLTexture inherits NSObjectProtocol.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
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
        (
            self.inner.depth(),
            self.inner.arrayLength(),
            self.inner.mipmapLevelCount(),
            self.inner.sampleCount(),
        )
    }

    /// Returns the texture type, rejecting unknown future values.
    pub fn texture_type(&self) -> Result<TextureType, Error> {
        TextureType::try_from_system_raw(self.inner.textureType().0)
            .ok_or_else(|| Error::unsupported("Metal returned an unknown texture type"))
    }

    /// Returns usage flags.
    #[must_use]
    pub fn usage(&self) -> TextureUsage {
        TextureUsage::from_system_raw(self.inner.usage().0)
    }

    /// Returns whether this texture is framebuffer-only.
    #[must_use]
    pub fn is_framebuffer_only(&self) -> bool {
        self.inner.isFramebufferOnly()
    }

    /// Returns whether this texture is shareable.
    #[must_use]
    pub fn is_shareable(&self) -> bool {
        self.inner.isShareable()
    }

    /// Returns whether this texture is sparse.
    #[must_use]
    pub fn is_sparse(&self) -> bool {
        self.inner.isSparse()
    }

    /// Returns whether GPU-optimized contents are allowed.
    #[must_use]
    pub fn allows_gpu_optimized_contents(&self) -> bool {
        self.inner.allowGPUOptimizedContents()
    }

    /// Returns the compression type.
    #[must_use]
    pub fn compression_type(&self) -> TextureCompressionType {
        TextureCompressionType::from_system_raw(self.inner.compressionType().0)
    }

    /// Returns the sparse-texture tier.
    #[must_use]
    pub fn sparse_texture_tier(&self) -> TextureSparseTier {
        TextureSparseTier::from_system_raw(self.inner.sparseTextureTier().0)
    }

    /// Returns the parent texture, if this is a view.
    #[must_use]
    pub fn parent_texture(&self) -> Option<Self> {
        self.inner.parentTexture().map(Self::new)
    }

    /// Returns parent-relative mip level and slice.
    #[must_use]
    pub fn parent_relative_location(&self) -> (usize, usize) {
        (
            self.inner.parentRelativeLevel(),
            self.inner.parentRelativeSlice(),
        )
    }

    /// Returns the backing buffer for buffer textures.
    #[must_use]
    pub fn buffer(&self) -> Option<Buffer> {
        self.inner.buffer().map(|inner| {
            let mode = match inner.storageMode().0 {
                1 => StorageMode::Managed,
                2 => StorageMode::Private,
                3 => StorageMode::Memoryless,
                _ => StorageMode::Shared,
            };
            Buffer::new(inner, mode)
        })
    }

    /// Returns backing-buffer offset and row stride.
    #[must_use]
    pub fn buffer_layout(&self) -> (usize, usize) {
        (self.inner.bufferOffset(), self.inner.bufferBytesPerRow())
    }

    /// Returns the first mip in the sparse tail and its byte size.
    #[must_use]
    pub fn sparse_tail(&self) -> (usize, usize) {
        (self.inner.firstMipmapInTail(), self.inner.tailSizeInBytes())
    }

    /// Returns the texture pixel format, preserving future system values.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_system_raw(self.inner.pixelFormat().0)
    }

    /// Returns the texture storage mode.
    #[must_use]
    pub fn storage_mode(&self) -> StorageMode {
        match self.inner.storageMode().0 {
            1 => StorageMode::Managed,
            2 => StorageMode::Private,
            3 => StorageMode::Memoryless,
            _ => StorageMode::Shared,
        }
    }

    /// Returns the opaque GPU resource identifier used by argument tables.
    #[must_use]
    pub fn gpu_resource_id(&self) -> ResourceID {
        let raw = self.inner.gpuResourceID();
        // SAFETY: MTLResourceID is an SDK-declared repr(C) structure whose
        // sole field is a u64; the generated safe value preserves that field.
        let value = unsafe { std::mem::transmute::<objc2_metal::MTLResourceID, u64>(raw) };
        ResourceID { _impl: value }
    }

    /// Returns the IOSurface plane index. A non-IOSurface texture reports zero.
    #[must_use]
    pub fn iosurface_plane(&self) -> usize {
        self.inner.iosurfacePlane()
    }

    /// Returns the owned IOSurface identity backing this texture, if any.
    #[must_use]
    pub fn iosurface(&self) -> Option<IoSurface> {
        // SAFETY: `iosurface` is declared to return a nullable retained-capable
        // Objective-C IOSurface object. Erasing the concrete class keeps the
        // object identity and ownership while avoiding an FFI type leak.
        let inner: Option<Retained<AnyObject>> = unsafe { msg_send![&*self.inner, iosurface] };
        inner.map(|inner| IoSurface {
            inner,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Returns the deprecated root resource as an opaque safe resource wrapper.
    #[allow(deprecated)]
    pub fn root_resource(&self) -> Option<Resource> {
        self.inner.rootResource().map(|resource| {
            // SAFETY: MTLResource inherits NSObjectProtocol and this cast only
            // erases the protocol type while retaining the same object.
            let object = unsafe { Retained::cast_unchecked(resource) };
            Resource::from_inner(object)
        })
    }

    /// Returns the source texture for a remote-storage view, if present.
    #[must_use]
    pub fn remote_storage_texture(&self) -> Option<Self> {
        self.inner.remoteStorageTexture().map(Self::new)
    }

    /// Creates a remote view for another device after validating the storage contract.
    pub fn new_remote_view(&self, device: &Device) -> Result<Self, Error> {
        if self.storage_mode() != StorageMode::Private && !self.is_shareable() {
            return Err(Error::invalid_argument(
                "remote texture views require private or shareable storage",
            ));
        }
        self.inner
            .newRemoteTextureViewForDevice(&device.inner)
            .map(Self::new)
            .ok_or_else(|| Error::unsupported("Metal could not create a remote texture view"))
    }

    /// Creates an interprocess shared texture handle when the resource is shareable.
    pub fn new_shared_texture_handle(&self) -> Result<SharedTextureHandle, Error> {
        if !self.is_shareable() {
            return Err(Error::invalid_argument(
                "a shared texture handle requires a shareable texture",
            ));
        }
        self.inner
            .newSharedTextureHandle()
            .map(|handle| {
                // SAFETY: MTLSharedTextureHandle is an Objective-C object and
                // this cast only erases its concrete class while retaining it.
                let object = unsafe { Retained::cast_unchecked(handle) };
                SharedTextureHandle::from_inner(object)
            })
            .ok_or_else(|| Error::unsupported("Metal could not create a shared texture handle"))
    }

    /// Returns a view using a compatible pixel format.
    pub fn view_with_pixel_format(&self, pixel_format: PixelFormat) -> Result<Self, Error> {
        self.inner
            .newTextureViewWithPixelFormat(pixel_format.as_objc())
            .map(Self::new)
            .ok_or_else(|| Error::unsupported("Metal rejected the texture view format"))
    }

    /// Creates a checked texture view over mip-level and slice ranges.
    pub fn view(
        &self,
        pixel_format: PixelFormat,
        texture_type: TextureType,
        levels: std::ops::Range<usize>,
        slices: std::ops::Range<usize>,
    ) -> Result<Self, Error> {
        self.validate_view_ranges(&levels, &slices)?;
        // SAFETY: both NSRange values were checked for ordering, overflow, and
        // bounds against the source texture before calling Metal.
        unsafe {
            self.inner
                .newTextureViewWithPixelFormat_textureType_levels_slices(
                    pixel_format.as_objc(),
                    texture_type.as_objc(),
                    NSRange::new(levels.start, levels.len()),
                    NSRange::new(slices.start, slices.len()),
                )
        }
        .map(Self::new)
        .ok_or_else(|| Error::unsupported("Metal rejected the texture view ranges"))
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
        self.validate_view_ranges(&levels, &slices)?;
        let swizzle = swizzle_to_objc(swizzle)?;
        // SAFETY: ranges were bounds-checked and every swizzle channel was
        // validated against the declared Metal enum values.
        unsafe {
            self.inner
                .newTextureViewWithPixelFormat_textureType_levels_slices_swizzle(
                    pixel_format.as_objc(),
                    texture_type.as_objc(),
                    NSRange::new(levels.start, levels.len()),
                    NSRange::new(slices.start, slices.len()),
                    swizzle,
                )
        }
        .map(Self::new)
        .ok_or_else(|| Error::unsupported("Metal rejected the swizzled texture view"))
    }

    /// Creates a texture view from the safe generated descriptor.
    pub fn view_with_descriptor(&self, descriptor: &TextureViewDescriptor) -> Result<Self, Error> {
        // SAFETY: the generated descriptor owns an MTLTextureViewDescriptor;
        // this cast restores the concrete SDK type without changing identity.
        let descriptor: &objc2_metal::MTLTextureViewDescriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner())
                .cast::<objc2_metal::MTLTextureViewDescriptor>())
        };
        self.inner
            .newTextureViewWithDescriptor(descriptor)
            .map(Self::new)
            .ok_or_else(|| Error::unsupported("Metal rejected the texture view descriptor"))
    }

    /// Copies owned bytes into a checked 2D texture region.
    pub fn replace_region_2d(
        &self,
        region: Region,
        mipmap_level: usize,
        bytes: &[u8],
        bytes_per_row: usize,
    ) -> Result<(), Error> {
        self.validate_cpu_write(region, mipmap_level, 0)?;
        let bytes_per_pixel = audited_bytes_per_pixel(self.pixel_format())?;
        let active_row = region
            .size
            .width
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| Error::invalid_argument("texture row byte count overflow"))?;
        if bytes_per_row < active_row {
            return Err(Error::invalid_argument(
                "texture row stride is smaller than the active row",
            ));
        }
        let required = texture_source_span(region, bytes_per_row, 0, bytes_per_pixel)?;
        if bytes_per_row == 0 || bytes.len() < required || required == 0 {
            return Err(Error::invalid_argument(
                "texture source does not cover every requested row",
            ));
        }
        let source = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>())
            .ok_or_else(|| Error::invalid_argument("texture source is empty"))?;
        // SAFETY: region and mip level are bounds-checked, storage is CPU
        // writable, and `bytes` covers bytes_per_row for every requested row.
        unsafe {
            self.inner.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                region.into(),
                mipmap_level,
                source,
                bytes_per_row,
            );
        }
        Ok(())
    }

    /// Copies owned bytes into a checked texture region and array slice.
    pub fn replace_region(
        &self,
        region: Region,
        mipmap_level: usize,
        slice: usize,
        bytes: &[u8],
        bytes_per_row: usize,
        bytes_per_image: usize,
    ) -> Result<(), Error> {
        self.validate_cpu_write(region, mipmap_level, slice)?;
        let bytes_per_pixel = audited_bytes_per_pixel(self.pixel_format())?;
        let active_row = region
            .size
            .width
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| Error::invalid_argument("texture row byte count overflow"))?;
        let minimum_image = bytes_per_row
            .checked_mul(region.size.height)
            .ok_or_else(|| Error::invalid_argument("texture image stride overflow"))?;
        if bytes_per_row < active_row || bytes_per_image < minimum_image {
            return Err(Error::invalid_argument(
                "texture row or image stride is too small",
            ));
        }
        let required =
            texture_source_span(region, bytes_per_row, bytes_per_image, bytes_per_pixel)?;
        if bytes_per_row == 0 || bytes_per_image == 0 || bytes.len() < required || required == 0 {
            return Err(Error::invalid_argument(
                "texture source does not cover every requested image",
            ));
        }
        let source = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>())
            .ok_or_else(|| Error::invalid_argument("texture source is empty"))?;
        // SAFETY: the texture destination and slice are bounds-checked, and
        // the borrowed source covers every row and image Metal may read.
        unsafe {
            self.inner
                .replaceRegion_mipmapLevel_slice_withBytes_bytesPerRow_bytesPerImage(
                    region.into(),
                    mipmap_level,
                    slice,
                    source,
                    bytes_per_row,
                    bytes_per_image,
                );
        }
        Ok(())
    }

    /// Returns this texture's channel swizzle.
    #[must_use]
    pub fn swizzle(&self) -> TextureSwizzleChannels {
        swizzle_from_objc(self.inner.swizzle())
    }

    fn validate_view_ranges(
        &self,
        levels: &std::ops::Range<usize>,
        slices: &std::ops::Range<usize>,
    ) -> Result<(), Error> {
        if levels.start >= levels.end
            || levels.end > self.inner.mipmapLevelCount()
            || slices.start >= slices.end
            || slices.end > self.slice_count()?
        {
            return Err(Error::invalid_argument(
                "texture view level or slice range is out of bounds",
            ));
        }
        Ok(())
    }

    fn validate_cpu_write(
        &self,
        region: Region,
        mipmap_level: usize,
        slice: usize,
    ) -> Result<(), Error> {
        if matches!(
            self.storage_mode(),
            StorageMode::Private | StorageMode::Memoryless
        ) {
            return Err(Error::unsupported(
                "private and memoryless textures are not CPU writable",
            ));
        }
        if mipmap_level >= self.inner.mipmapLevelCount() || slice >= self.slice_count()? {
            return Err(Error::invalid_argument(
                "texture mip level or slice is out of bounds",
            ));
        }
        if region.size.width == 0 || region.size.height == 0 || region.size.depth == 0 {
            return Err(Error::invalid_argument("texture region must be non-empty"));
        }
        let width = (self.width() >> mipmap_level.min(usize::BITS as usize - 1)).max(1);
        let height = (self.height() >> mipmap_level.min(usize::BITS as usize - 1)).max(1);
        let depth = (self.inner.depth() >> mipmap_level.min(usize::BITS as usize - 1)).max(1);
        let end_x = region.origin.x.checked_add(region.size.width);
        let end_y = region.origin.y.checked_add(region.size.height);
        let end_z = region.origin.z.checked_add(region.size.depth);
        if end_x.is_none_or(|end| end > width)
            || end_y.is_none_or(|end| end > height)
            || end_z.is_none_or(|end| end > depth)
        {
            return Err(Error::invalid_argument("texture region is out of bounds"));
        }
        Ok(())
    }

    fn slice_count(&self) -> Result<usize, Error> {
        let count = match self.texture_type()? {
            TextureType::Cube => 6,
            TextureType::CubeArray => self
                .inner
                .arrayLength()
                .checked_mul(6)
                .ok_or_else(|| Error::invalid_argument("cube-array slice count overflow"))?,
            TextureType::D1Array | TextureType::D2Array | TextureType::D2MultisampleArray => {
                self.inner.arrayLength()
            }
            _ => 1,
        };
        Ok(count.max(1))
    }
}

fn swizzle_to_objc(value: &TextureSwizzleChannels) -> Result<MTLTextureSwizzleChannels, Error> {
    for channel in [&value.red, &value.green, &value.blue, &value.alpha] {
        if !channel.is_valid() {
            return Err(Error::invalid_argument(
                "texture swizzle channel is not a declared value",
            ));
        }
    }
    Ok(MTLTextureSwizzleChannels {
        red: MTLTextureSwizzle(value.red.as_raw()),
        green: MTLTextureSwizzle(value.green.as_raw()),
        blue: MTLTextureSwizzle(value.blue.as_raw()),
        alpha: MTLTextureSwizzle(value.alpha.as_raw()),
    })
}

fn swizzle_from_objc(value: MTLTextureSwizzleChannels) -> TextureSwizzleChannels {
    TextureSwizzleChannels {
        red: TextureSwizzle::from_system_raw(value.red.0),
        green: TextureSwizzle::from_system_raw(value.green.0),
        blue: TextureSwizzle::from_system_raw(value.blue.0),
        alpha: TextureSwizzle::from_system_raw(value.alpha.0),
    }
}

pub(super) fn audited_bytes_per_pixel(pixel_format: PixelFormat) -> Result<usize, Error> {
    match pixel_format {
        PixelFormat::RGBA8_UNORM
        | PixelFormat::BGRA8_UNORM
        | PixelFormat::RGBA8_UNORM_SRGB
        | PixelFormat::BGRA8_UNORM_SRGB
        | PixelFormat::RG16_FLOAT
        | PixelFormat::DEPTH32_FLOAT => Ok(4),
        PixelFormat::R16_FLOAT => Ok(2),
        PixelFormat::RGBA16_FLOAT => Ok(8),
        _ => Err(Error::unsupported(
            "CPU texture writes do not support this pixel format",
        )),
    }
}

fn texture_source_span(
    region: Region,
    bytes_per_row: usize,
    bytes_per_image: usize,
    bytes_per_pixel: usize,
) -> Result<usize, Error> {
    let final_texel_row = region
        .size
        .width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| Error::invalid_argument("texture row byte count overflow"))?;
    let preceding_rows = region
        .size
        .height
        .saturating_sub(1)
        .checked_mul(bytes_per_row)
        .ok_or_else(|| Error::invalid_argument("texture row layout overflow"))?;
    let preceding_images = region
        .size
        .depth
        .saturating_sub(1)
        .checked_mul(bytes_per_image)
        .ok_or_else(|| Error::invalid_argument("texture image layout overflow"))?;
    preceding_images
        .checked_add(preceding_rows)
        .and_then(|value| value.checked_add(final_texel_row))
        .ok_or_else(|| Error::invalid_argument("texture source layout overflow"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metal::{Origin, Size};

    #[test]
    fn source_span_covers_final_texel_row_and_image() {
        let region = Region::new(Origin::new(0, 0, 0), Size::new(4, 3, 2));
        assert_eq!(texture_source_span(region, 16, 48, 4).unwrap(), 96);
    }

    #[test]
    fn source_span_rejects_overflow() {
        let region = Region::new(Origin::new(0, 0, 0), Size::new(usize::MAX, 1, 1));
        assert!(texture_source_span(region, 1, 1, 4).is_err());
    }
}
