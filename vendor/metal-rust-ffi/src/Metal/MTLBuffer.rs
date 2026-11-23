//! Audited buffer allocation and CPU write boundary.

use crate::ThreadBound;
use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::metal::Tensor;
use crate::metal::generated_value_types::BufferSparseTier;
use crate::metal::{CheckedTensorDescriptor, Device, StorageMode, Texture, TextureDescriptor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{MTLBuffer, MTLResource, MTLTensorDescriptor};

/// An owned Metal buffer.
#[derive(Clone)]
pub struct Buffer {
    pub(super) inner: Retained<ProtocolObject<dyn MTLBuffer>>,
    storage: StorageMode,
    _thread_bound: ThreadBound,
}

impl Buffer {
    pub(super) const fn new(
        inner: Retained<ProtocolObject<dyn MTLBuffer>>,
        storage: StorageMode,
    ) -> Self {
        Self {
            inner,
            storage,
            _thread_bound: ThreadBound::new(),
        }
    }

    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: generated callers use this only for selectors declared to
        // return an object conforming to MTLBuffer.
        let inner: Retained<ProtocolObject<dyn MTLBuffer>> =
            unsafe { Retained::cast_unchecked(inner) };
        let storage = StorageMode::try_from_system_raw(inner.storageMode().0)
            .ok_or_else(|| Error::unsupported("Metal returned an unknown buffer storage mode"))?;
        Ok(Self::new(inner, storage))
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: MTLBuffer is an Objective-C protocol object backed by this
        // same object pointer.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }

    /// Returns the byte length of the buffer.
    #[must_use]
    pub fn length(&self) -> usize {
        self.inner.length()
    }

    /// Returns the allocation storage mode selected at creation time.
    #[must_use]
    pub const fn storage_mode(&self) -> StorageMode {
        self.storage
    }

    /// Adds a debug marker for a checked byte range.
    pub fn add_debug_marker(
        &self,
        marker: &str,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        if range.start > range.end || range.end > self.length() {
            return Err(Error::invalid_argument(
                "buffer debug marker range is out of bounds",
            ));
        }
        let marker = NSString::from_str(marker);
        self.inner
            .addDebugMarker_range(&marker, NSRange::new(range.start, range.len()));
        Ok(())
    }

    /// Removes all debug markers.
    pub fn remove_all_debug_markers(&self) {
        self.inner.removeAllDebugMarkers();
    }

    /// Returns the GPU virtual address.
    #[must_use]
    pub fn gpu_address(&self) -> u64 {
        self.inner.gpuAddress()
    }

    /// Returns the sparse-buffer tier.
    #[must_use]
    pub fn sparse_buffer_tier(&self) -> BufferSparseTier {
        BufferSparseTier::from_system_raw(self.inner.sparseBufferTier().0)
    }

    /// Returns the remote-storage buffer, if present.
    #[must_use]
    pub fn remote_storage_buffer(&self) -> Option<Self> {
        self.inner.remoteStorageBuffer().map(|inner| {
            let storage = storage_mode(&inner);
            Self::new(inner, storage)
        })
    }

    /// Creates a remote view for another device when supported.
    pub fn new_remote_view(&self, device: &Device) -> Result<Self, Error> {
        self.inner
            .newRemoteBufferViewForDevice(&device.inner)
            .map(|inner| {
                let storage = storage_mode(&inner);
                Self::new(inner, storage)
            })
            .ok_or_else(|| Error::unsupported("Metal could not create a remote buffer view"))
    }

    /// Creates a texture view over a checked buffer range and row stride.
    pub fn new_texture(
        &self,
        descriptor: &TextureDescriptor,
        offset: usize,
        bytes_per_row: usize,
    ) -> Result<Texture, Error> {
        if offset >= self.length() || bytes_per_row == 0 {
            return Err(Error::invalid_argument(
                "buffer texture offset must be in range and row stride non-zero",
            ));
        }
        self.inner
            .newTextureWithDescriptor_offset_bytesPerRow(&descriptor.inner, offset, bytes_per_row)
            .map(Texture::new)
            .ok_or_else(|| Error::unsupported("Metal rejected the buffer texture view"))
    }

    /// Creates a tensor sharing this buffer after proving its complete byte range.
    pub fn new_tensor(
        &self,
        offset: usize,
        descriptor: &CheckedTensorDescriptor,
    ) -> Result<Tensor, Error> {
        let _checked_range = descriptor.checked_buffer_range(self.length(), offset)?;
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // the selector/bool ABI is fixed by NSObjectProtocol.
        let available: bool = unsafe {
            msg_send![
                &*self.inner,
                respondsToSelector: sel!(newTensorWithDescriptor:offset:error:)
            ]
        };
        if !available {
            return Err(Error::unsupported(
                "buffer-backed tensor creation is unavailable on this system",
            ));
        }
        // SAFETY: CheckedTensorDescriptor creates and retains exactly an
        // MTLTensorDescriptor. This restores its concrete SDK type without
        // changing object identity or lifetime.
        let descriptor: &MTLTensorDescriptor =
            unsafe { &*(std::ptr::from_ref(descriptor.as_inner()).cast::<MTLTensorDescriptor>()) };
        // SAFETY: checked_buffer_range proved the complete tensor span is
        // aligned, non-overflowing, and contained in this buffer. The runtime
        // selector was checked above and the descriptor has the exact class.
        let tensor = unsafe {
            self.inner
                .newTensorWithDescriptor_offset_error(descriptor, offset)
        }
        .map_err(|error| metal_error(&error))?;
        // SAFETY: Metal declares the returned object as conforming to MTLTensor;
        // this only erases that protocol type for the safe generated wrapper.
        let tensor = unsafe { Retained::cast_unchecked(tensor) };
        Ok(Tensor::from_inner(tensor))
    }

    /// Copies bytes into CPU-visible storage after checking the complete range.
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if matches!(self.storage, StorageMode::Private | StorageMode::Memoryless) {
            return Err(Error::unsupported(
                "private and memoryless buffers are not CPU writable",
            ));
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or_else(|| Error::invalid_argument("buffer write range overflow"))?;
        if end > self.length() {
            return Err(Error::invalid_argument(
                "buffer write range is out of bounds",
            ));
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let destination = self.inner.contents().as_ptr().cast::<u8>();
        // SAFETY: the storage mode is CPU-visible, the checked range is inside
        // the Metal allocation, and both pointers cover bytes.len() bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination.add(offset), bytes.len());
        }
        if self.storage == StorageMode::Managed {
            self.inner.didModifyRange(NSRange::new(offset, bytes.len()));
        }
        Ok(())
    }

    /// Copies CPU-visible bytes after the caller has synchronized GPU access.
    pub fn read(&self, offset: usize, length: usize) -> Result<Vec<u8>, Error> {
        if matches!(self.storage, StorageMode::Private | StorageMode::Memoryless) {
            return Err(Error::unsupported(
                "private and memoryless buffers are not CPU readable",
            ));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| Error::invalid_argument("buffer read range overflow"))?;
        if end > self.length() {
            return Err(Error::invalid_argument(
                "buffer read range is out of bounds",
            ));
        }
        if length == 0 {
            return Ok(Vec::new());
        }
        let source = self.inner.contents().as_ptr().cast::<u8>();
        let mut bytes = vec![0_u8; length];
        // SAFETY: the caller guarantees GPU synchronization; the checked
        // range is inside the CPU-visible Metal allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(source.add(offset), bytes.as_mut_ptr(), length);
        }
        Ok(bytes)
    }

    pub(crate) fn completed_bytes(&self, length: usize) -> Result<Vec<u8>, Error> {
        self.read(0, length)
    }
}

fn storage_mode(buffer: &ProtocolObject<dyn MTLBuffer>) -> StorageMode {
    match buffer.storageMode().0 {
        1 => StorageMode::Managed,
        2 => StorageMode::Private,
        3 => StorageMode::Memoryless,
        _ => StorageMode::Shared,
    }
}
