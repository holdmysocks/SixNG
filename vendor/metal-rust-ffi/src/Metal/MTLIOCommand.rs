//! Audited Metal IO command and compression boundary.

use crate::ThreadBound;
use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::metal::{
    IOCommandQueueDescriptor, IOScratchBuffer, IOScratchBufferAllocator, SharedEvent,
};
use crate::metal::generated_value_types::{IOCompressionMethod, IOStatus};
use crate::metal::{Buffer, Device, Origin, Size, Texture};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, sel};
use objc2_foundation::{NSObjectProtocol, NSString, NSURL};
use objc2_metal::{
    MTLDevice, MTLIOCommandBuffer, MTLIOCommandQueue, MTLIOCommandQueueDescriptor,
    MTLIOCompressionMethod, MTLIOFileHandle,
};
use std::ffi::{CString, c_char, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use std::{mem, panic};

#[link(name = "System")]
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

impl IOScratchBufferAllocator {
    /// Requests an owned scratch buffer without exposing the allocator protocol.
    pub fn new_scratch_buffer(
        &self,
        minimum_size: usize,
    ) -> Result<Option<IOScratchBuffer>, Error> {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // Sel/bool use the runtime's stable declared ABIs.
        let available: bool = unsafe {
            msg_send![self.as_inner(), respondsToSelector: sel!(newScratchBufferWithMinimumSize:)]
        };
        if !available {
            return Err(Error::unsupported(
                "MTLIOScratchBufferAllocator::newScratchBuffer is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; usize is NSUInteger on
        // supported targets and objc2 captures the nullable +1 result in a
        // retained owner before it is moved into the safe wrapper.
        let value: Option<Retained<objc2::runtime::AnyObject>> =
            unsafe { msg_send![self.as_inner(), newScratchBufferWithMinimumSize: minimum_size] };
        Ok(value.map(IOScratchBuffer::from_inner))
    }
}

/// A retained Metal IO file handle.
#[derive(Clone)]
pub struct IoFileHandle {
    inner: Retained<ProtocolObject<dyn MTLIOFileHandle>>,
    _thread_bound: ThreadBound,
}

impl IoFileHandle {
    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|label| label.to_string())
    }

    /// Sets or clears the debug label.
    pub fn set_label(&self, label: Option<&str>) {
        let label = label.map(NSString::from_str);
        self.inner.setLabel(label.as_deref());
    }
}

/// A Metal IO command queue that always creates retaining command buffers.
#[derive(Clone)]
pub struct IoCommandQueue {
    inner: Retained<ProtocolObject<dyn MTLIOCommandQueue>>,
    _thread_bound: ThreadBound,
}

impl IoCommandQueue {
    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|label| label.to_string())
    }

    /// Sets or clears the debug label.
    pub fn set_label(&self, label: Option<&str>) {
        let label = label.map(NSString::from_str);
        self.inner.setLabel(label.as_deref());
    }

    /// Inserts a queue ordering barrier after checking availability.
    pub fn enqueue_barrier(&self) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(enqueueBarrier)) {
            return Err(Error::unsupported(
                "MTLIOCommandQueue::enqueueBarrier is unavailable",
            ));
        }
        self.inner.enqueueBarrier();
        Ok(())
    }

    /// Creates a command buffer that retains every referenced Metal object.
    pub fn command_buffer(&self) -> Result<IoCommandBuffer, Error> {
        if !self.inner.respondsToSelector(sel!(commandBuffer)) {
            return Err(Error::unsupported(
                "MTLIOCommandQueue::commandBuffer is unavailable",
            ));
        }
        Ok(IoCommandBuffer {
            inner: self.inner.commandBuffer(),
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Reads file bytes through an IO command and returns owned memory only
    /// after the command has completed.
    pub fn read_bytes(
        &self,
        source: &IoFileHandle,
        source_offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, Error> {
        source_offset
            .checked_add(length)
            .ok_or_else(|| Error::invalid_argument("IO source range overflow"))?;
        if length == 0 {
            return Ok(Vec::new());
        }
        let mut bytes = vec![0_u8; length];
        let command_buffer = self.command_buffer()?;
        command_buffer.encode_load_bytes(&mut bytes, source, source_offset)?;
        command_buffer.commit().wait()?;
        Ok(bytes)
    }
}

/// A recording Metal IO command buffer.
pub struct IoCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLIOCommandBuffer>>,
    _thread_bound: ThreadBound,
}

impl IoCommandBuffer {
    /// Returns the current command-buffer status.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        IOStatus::from_system_raw(self.inner.status().0)
    }

    /// Returns owned error information when Metal has reported one.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(|error| metal_error(&error))
    }

    /// Installs a one-shot completion callback. Callback panics are contained
    /// at the Objective-C block boundary.
    pub fn on_complete(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(addCompletedHandler:)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::addCompletedHandler is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = block2::RcBlock::new(
            move |command_buffer: NonNull<ProtocolObject<dyn MTLIOCommandBuffer>>| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                // SAFETY: Metal passes a non-null command buffer that remains
                // alive for the duration of the completion callback.
                let command_buffer = unsafe { command_buffer.as_ref() };
                let result = completion_result(command_buffer);
                let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector availability was checked and RcBlock has the exact
        // MTLIOCommandBufferHandler ABI. Metal copies the block for delivery.
        unsafe {
            let _: () = msg_send![&*self.inner, addCompletedHandler: &*block];
        }
        Ok(())
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|label| label.to_string())
    }

    /// Sets or clears the debug label.
    pub fn set_label(&mut self, label: Option<&str>) {
        let label = label.map(NSString::from_str);
        self.inner.setLabel(label.as_deref());
    }

    /// Pushes a debug group.
    pub fn push_debug_group(&mut self, label: &str) {
        self.inner.pushDebugGroup(&NSString::from_str(label));
    }

    /// Pops the last debug group.
    pub fn pop_debug_group(&mut self) {
        self.inner.popDebugGroup();
    }

    /// Adds an ordering barrier inside this command buffer.
    pub fn add_barrier(&mut self) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(addBarrier)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::addBarrier is unavailable",
            ));
        }
        self.inner.addBarrier();
        Ok(())
    }

    /// Encodes a checked file-to-buffer load.
    pub fn load_buffer(
        &mut self,
        destination: &Buffer,
        destination_offset: usize,
        length: usize,
        source: &IoFileHandle,
        source_offset: usize,
    ) -> Result<(), Error> {
        checked_range(
            destination_offset,
            length,
            destination.length(),
            "IO buffer load",
        )?;
        source_offset
            .checked_add(length)
            .ok_or_else(|| Error::invalid_argument("IO source range overflow"))?;
        if length == 0 {
            return Ok(());
        }
        if !self
            .inner
            .respondsToSelector(sel!(loadBuffer:offset:size:sourceHandle:sourceHandleOffset:))
        {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::loadBuffer is unavailable",
            ));
        }
        // SAFETY: both arithmetic ranges were checked; the retaining IO
        // command buffer keeps destination and source alive through execution.
        unsafe {
            self.inner
                .loadBuffer_offset_size_sourceHandle_sourceHandleOffset(
                    &destination.inner,
                    destination_offset,
                    length,
                    &source.inner,
                    source_offset,
                );
        }
        Ok(())
    }

    /// Encodes a checked file-to-texture load.
    #[allow(clippy::too_many_arguments)]
    pub fn load_texture(
        &mut self,
        destination: &Texture,
        slice: usize,
        level: usize,
        size: Size,
        source_bytes_per_row: usize,
        source_bytes_per_image: usize,
        destination_origin: Origin,
        source: &IoFileHandle,
        source_offset: usize,
    ) -> Result<(), Error> {
        validate_texture_load(
            destination,
            slice,
            level,
            size,
            source_bytes_per_row,
            source_bytes_per_image,
            destination_origin,
            source_offset,
        )?;
        if !self.inner.respondsToSelector(sel!(loadTexture:slice:level:size:sourceBytesPerRow:sourceBytesPerImage:destinationOrigin:sourceHandle:sourceHandleOffset:)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::loadTexture is unavailable",
            ));
        }
        // SAFETY: texture bounds, slice, level, strides, and source arithmetic
        // were validated; this command buffer retains both Objective-C objects.
        unsafe {
            self.inner.loadTexture_slice_level_size_sourceBytesPerRow_sourceBytesPerImage_destinationOrigin_sourceHandle_sourceHandleOffset(
                &destination.inner,
                slice,
                level,
                size.into(),
                source_bytes_per_row,
                source_bytes_per_image,
                destination_origin.into(),
                &source.inner,
                source_offset,
            );
        }
        Ok(())
    }

    /// Encodes the final IO status into a checked buffer location.
    pub fn copy_status_to_buffer(
        &mut self,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let status_size = mem::size_of::<isize>();
        checked_range(offset, status_size, destination.length(), "IO status copy")?;
        if !offset.is_multiple_of(status_size) {
            return Err(Error::invalid_argument(
                "IO status destination offset is not naturally aligned",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(copyStatusToBuffer:offset:))
        {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::copyStatusToBuffer is unavailable",
            ));
        }
        // SAFETY: destination range and natural alignment were checked, and
        // the retaining command buffer keeps the buffer alive until completion.
        unsafe {
            self.inner
                .copyStatusToBuffer_offset(&destination.inner, offset);
        }
        Ok(())
    }

    /// Encodes a wait on a shared event.
    pub fn wait_for_event(&mut self, event: &SharedEvent, value: u64) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(waitForEvent:value:)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::waitForEvent is unavailable",
            ));
        }
        // SAFETY: the generated wrapper owns an object declared to conform to
        // MTLSharedEvent, and the selector was checked before sending.
        unsafe {
            let _: () = msg_send![&*self.inner, waitForEvent: event.as_inner(), value: value];
        }
        Ok(())
    }

    /// Encodes a signal on a shared event.
    pub fn signal_event(&mut self, event: &SharedEvent, value: u64) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(signalEvent:value:)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::signalEvent is unavailable",
            ));
        }
        // SAFETY: the generated wrapper owns an object declared to conform to
        // MTLSharedEvent, and the selector was checked before sending.
        unsafe {
            let _: () = msg_send![&*self.inner, signalEvent: event.as_inner(), value: value];
        }
        Ok(())
    }

    /// Enqueues the command buffer while leaving it available for commit.
    pub fn enqueue(&mut self) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(enqueue)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::enqueue is unavailable",
            ));
        }
        self.inner.enqueue();
        Ok(())
    }

    /// Commits this command buffer exactly once.
    #[must_use]
    pub fn commit(self) -> SubmittedIoCommandBuffer {
        self.inner.commit();
        SubmittedIoCommandBuffer {
            inner: self.inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    fn encode_load_bytes(
        &self,
        destination: &mut [u8],
        source: &IoFileHandle,
        source_offset: usize,
    ) -> Result<(), Error> {
        if destination.is_empty() {
            return Ok(());
        }
        if !self
            .inner
            .respondsToSelector(sel!(loadBytes:size:sourceHandle:sourceHandleOffset:))
        {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::loadBytes is unavailable",
            ));
        }
        let pointer = NonNull::new(destination.as_mut_ptr().cast::<c_void>())
            .ok_or_else(|| Error::invalid_argument("IO byte destination is null"))?;
        // SAFETY: the mutable slice remains alive and inaccessible to the
        // caller until read_bytes synchronously waits for GPU IO completion.
        unsafe {
            self.inner.loadBytes_size_sourceHandle_sourceHandleOffset(
                pointer,
                destination.len(),
                &source.inner,
                source_offset,
            );
        }
        Ok(())
    }
}

/// A committed Metal IO command buffer.
pub struct SubmittedIoCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLIOCommandBuffer>>,
    _thread_bound: ThreadBound,
}

impl SubmittedIoCommandBuffer {
    /// Returns the current command-buffer status without claiming completion.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        IOStatus::from_system_raw(self.inner.status().0)
    }

    /// Returns owned error information when Metal has reported one.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(|error| metal_error(&error))
    }

    /// Requests cancellation. Completion must still be observed with `wait`.
    pub fn try_cancel(&self) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(tryCancel)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::tryCancel is unavailable",
            ));
        }
        self.inner.tryCancel();
        Ok(())
    }

    /// Waits for completion and returns a completed-state token.
    pub fn wait(self) -> Result<CompletedIoCommandBuffer, Error> {
        if !self.inner.respondsToSelector(sel!(waitUntilCompleted)) {
            return Err(Error::unsupported(
                "MTLIOCommandBuffer::waitUntilCompleted is unavailable",
            ));
        }
        self.inner.waitUntilCompleted();
        completion_result(&self.inner)?;
        Ok(CompletedIoCommandBuffer {
            inner: self.inner,
            _thread_bound: ThreadBound::new(),
        })
    }
}

/// Proof that a Metal IO command buffer completed successfully.
pub struct CompletedIoCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLIOCommandBuffer>>,
    _thread_bound: ThreadBound,
}

impl CompletedIoCommandBuffer {
    /// Returns the terminal status reported by Metal.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        IOStatus::from_system_raw(self.inner.status().0)
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|label| label.to_string())
    }

    /// Returns owned error information. A successfully completed token should
    /// normally report `None`.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(|error| metal_error(&error))
    }
}

impl Device {
    /// Creates an IO queue from a runtime-checked descriptor.
    pub fn new_io_command_queue(
        &self,
        descriptor: &IOCommandQueueDescriptor,
    ) -> Result<IoCommandQueue, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newIOCommandQueueWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "MTLDevice::newIOCommandQueue is unavailable",
            ));
        }
        // SAFETY: the generated descriptor wrapper owns an instance of the
        // SDK-declared MTLIOCommandQueueDescriptor class.
        let descriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner()).cast::<MTLIOCommandQueueDescriptor>())
        };
        self.inner
            .newIOCommandQueueWithDescriptor_error(descriptor)
            .map(|inner| IoCommandQueue {
                inner,
                _thread_bound: ThreadBound::new(),
            })
            .map_err(|error| metal_error(&error))
    }

    /// Creates an IO file handle for a raw or compressed file path.
    pub fn new_io_file_handle(
        &self,
        path: &Path,
        compression: Option<IOCompressionMethod>,
    ) -> Result<IoFileHandle, Error> {
        let path = path
            .to_str()
            .ok_or_else(|| Error::invalid_argument("IO file path is not valid UTF-8"))?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let inner = match compression {
            None => {
                if !self
                    .inner
                    .respondsToSelector(sel!(newIOFileHandleWithURL:error:))
                {
                    return Err(Error::unsupported(
                        "MTLDevice::newIOFileHandle is unavailable",
                    ));
                }
                self.inner.newIOFileHandleWithURL_error(&url)
            }
            Some(method) => {
                if !method.is_valid() {
                    return Err(Error::invalid_argument(
                        "IO compression method is not a declared value",
                    ));
                }
                if !self.inner.respondsToSelector(sel!(
                    newIOFileHandleWithURL:compressionMethod:error:
                )) {
                    return Err(Error::unsupported(
                        "compressed MTLDevice::newIOFileHandle is unavailable",
                    ));
                }
                self.inner.newIOFileHandleWithURL_compressionMethod_error(
                    &url,
                    MTLIOCompressionMethod(method.as_raw()),
                )
            }
        };
        inner
            .map(|inner| IoFileHandle {
                inner,
                _thread_bound: ThreadBound::new(),
            })
            .map_err(|error| metal_error(&error))
    }
}

/// RAII owner for Metal's streaming IO compression context.
pub struct IoCompressionContext {
    inner: Option<NonNull<c_void>>,
    append: CompressionAppend,
    flush_and_destroy: CompressionFlush,
}

type CompressionAppend = unsafe extern "C-unwind" fn(NonNull<c_void>, NonNull<c_void>, usize);
type CompressionFlush =
    unsafe extern "C-unwind" fn(NonNull<c_void>) -> objc2_metal::MTLIOCompressionStatus;

impl IoCompressionContext {
    /// Returns Metal's preferred default chunk size when the function exists.
    pub fn default_chunk_size() -> Result<usize, Error> {
        let function: unsafe extern "C-unwind" fn() -> usize =
            resolve_function(c"MTLIOCompressionContextDefaultChunkSize")?;
        // SAFETY: dlsym resolved the exact SDK function named above.
        Ok(unsafe { function() })
    }

    /// Creates a compression stream writing to `path`.
    pub fn new(path: &Path, method: IOCompressionMethod, chunk_size: usize) -> Result<Self, Error> {
        if !method.is_valid() {
            return Err(Error::invalid_argument(
                "IO compression method is not a declared value",
            ));
        }
        if chunk_size == 0 {
            return Err(Error::invalid_argument(
                "IO compression chunk size must be non-zero",
            ));
        }
        let path = path
            .to_str()
            .ok_or_else(|| Error::invalid_argument("compression path is not valid UTF-8"))?;
        let path = CString::new(path)
            .map_err(|_| Error::invalid_argument("compression path contains a NUL byte"))?;
        type Create = unsafe extern "C-unwind" fn(
            NonNull<c_char>,
            MTLIOCompressionMethod,
            usize,
        ) -> *mut c_void;
        let function: Create = resolve_function(c"MTLIOCreateCompressionContext")?;
        let append = resolve_function(c"MTLIOCompressionContextAppendData")?;
        let flush_and_destroy = resolve_function(c"MTLIOFlushAndDestroyCompressionContext")?;
        let path = NonNull::new(path.as_ptr().cast_mut())
            .ok_or_else(|| Error::invalid_argument("compression path is null"))?;
        // SAFETY: path is a live NUL-terminated CString, the enum is validated,
        // the chunk size is non-zero, and dlsym resolved the exact SDK symbol.
        let inner = unsafe { function(path, MTLIOCompressionMethod(method.as_raw()), chunk_size) };
        let inner = NonNull::new(inner).ok_or_else(|| {
            Error::unsupported("Metal could not create an IO compression context")
        })?;
        Ok(Self {
            inner: Some(inner),
            append,
            flush_and_destroy,
        })
    }

    /// Appends bytes to the compression stream.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.is_empty() {
            return Ok(());
        }
        let context = self
            .inner
            .ok_or_else(|| Error::invalid_argument("compression context is already finished"))?;
        let data = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>())
            .ok_or_else(|| Error::invalid_argument("compression input is null"))?;
        // SAFETY: context is exclusively owned and live, data covers bytes.len
        // readable bytes for this synchronous C call, and the symbol is exact.
        unsafe { (self.append)(context, data, bytes.len()) };
        Ok(())
    }

    /// Flushes, destroys, and reports the compression result.
    pub fn finish(mut self) -> Result<(), Error> {
        let context = self
            .inner
            .take()
            .ok_or_else(|| Error::invalid_argument("compression context is already finished"))?;
        // SAFETY: taking the Option transfers the one live context to the
        // destroy function resolved before the context was created.
        let status = unsafe { (self.flush_and_destroy)(context) }.0;
        if status == 0 {
            Ok(())
        } else {
            Err(execution_error("Metal IO compression failed"))
        }
    }
}

impl Drop for IoCompressionContext {
    fn drop(&mut self) {
        let Some(context) = self.inner.take() else {
            return;
        };
        // Drop must release Metal's opaque context. It deliberately discards
        // the status and therefore never reports an unobserved success.
        // SAFETY: Drop takes the one live context exactly once and the stored
        // function pointer was resolved before creation succeeded.
        let _ = unsafe { (self.flush_and_destroy)(context) };
    }
}

fn completion_result(command_buffer: &ProtocolObject<dyn MTLIOCommandBuffer>) -> Result<(), Error> {
    if let Some(error) = command_buffer.error() {
        return Err(metal_error(&error));
    }
    match command_buffer.status().0 {
        3 => Ok(()),
        1 => Err(execution_error("Metal IO command buffer was cancelled")),
        2 => Err(execution_error(
            "Metal IO command buffer failed without NSError",
        )),
        status => Err(execution_error(format!(
            "Metal IO command buffer returned non-terminal status {status}"
        ))),
    }
}

fn checked_range(offset: usize, length: usize, total: usize, operation: &str) -> Result<(), Error> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::invalid_argument(format!("{operation} range overflow")))?;
    if end > total {
        return Err(Error::invalid_argument(format!(
            "{operation} range is out of bounds"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_texture_load(
    texture: &Texture,
    slice: usize,
    level: usize,
    size: Size,
    bytes_per_row: usize,
    bytes_per_image: usize,
    origin: Origin,
    source_offset: usize,
) -> Result<(), Error> {
    if size.width == 0 || size.height == 0 || size.depth == 0 {
        return Err(Error::invalid_argument(
            "IO texture load dimensions must be non-zero",
        ));
    }
    let (depth, array_length, mip_levels, _) = texture.layout();
    if level >= mip_levels || slice >= array_length.max(1) {
        return Err(Error::invalid_argument(
            "IO texture load slice or mip level is out of bounds",
        ));
    }
    let mip_width = (texture.width() >> level).max(1);
    let mip_height = (texture.height() >> level).max(1);
    let mip_depth = (depth >> level).max(1);
    let end_x = origin
        .x
        .checked_add(size.width)
        .ok_or_else(|| Error::invalid_argument("IO texture x range overflow"))?;
    let end_y = origin
        .y
        .checked_add(size.height)
        .ok_or_else(|| Error::invalid_argument("IO texture y range overflow"))?;
    let end_z = origin
        .z
        .checked_add(size.depth)
        .ok_or_else(|| Error::invalid_argument("IO texture z range overflow"))?;
    if end_x > mip_width || end_y > mip_height || end_z > mip_depth {
        return Err(Error::invalid_argument(
            "IO texture destination region is out of bounds",
        ));
    }
    if bytes_per_row == 0 || bytes_per_image == 0 {
        return Err(Error::invalid_argument(
            "IO texture source strides must be non-zero",
        ));
    }
    let minimum_image = bytes_per_row
        .checked_mul(size.height)
        .ok_or_else(|| Error::invalid_argument("IO texture row stride overflow"))?;
    if bytes_per_image < minimum_image {
        return Err(Error::invalid_argument(
            "IO texture image stride is smaller than its rows",
        ));
    }
    let source_length = bytes_per_image
        .checked_mul(size.depth)
        .ok_or_else(|| Error::invalid_argument("IO texture image stride overflow"))?;
    source_offset
        .checked_add(source_length)
        .ok_or_else(|| Error::invalid_argument("IO texture source range overflow"))?;
    Ok(())
}

fn resolve_function<T: Copy>(symbol: &std::ffi::CStr) -> Result<T, Error> {
    let handle = (-2_isize) as *mut c_void;
    // SAFETY: handle is Darwin RTLD_DEFAULT and symbol is NUL-terminated.
    let address = unsafe { dlsym(handle, symbol.as_ptr()) };
    if address.is_null() {
        return Err(Error::unsupported(format!(
            "Metal IO function {symbol:?} is unavailable"
        )));
    }
    if mem::size_of::<T>() != mem::size_of::<*mut c_void>() {
        return Err(Error::unsupported(
            "Metal IO function pointer has an unsupported representation",
        ));
    }
    // SAFETY: callers choose T to exactly match the SDK declaration for the
    // closed symbol passed alongside it; function and data pointers have equal
    // representation on supported Darwin targets, checked immediately above.
    Ok(unsafe { mem::transmute_copy::<*mut c_void, T>(&address) })
}

fn execution_error(message: impl Into<String>) -> Error {
    Error {
        domain: Some("MTLIOErrorDomain".into()),
        // Preserve classification as an execution failure in the facade even
        // when Metal returned no NSError with a more specific native code.
        code: Some(-1),
        message: message.into(),
        invalid_argument: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_ranges_reject_overflow_and_out_of_bounds() {
        assert!(checked_range(4, 4, 8, "test").is_ok());
        assert!(checked_range(5, 4, 8, "test").is_err());
        assert!(checked_range(usize::MAX, 1, usize::MAX, "test").is_err());
    }

    #[test]
    fn compression_rejects_zero_chunk_before_calling_metal() {
        let method = IOCompressionMethod::try_from(0).expect("zlib is declared");
        let error = IoCompressionContext::new(Path::new("output.gpuio"), method, 0)
            .err()
            .expect("zero chunks must be rejected");
        assert!(error.invalid_argument);
    }
}
