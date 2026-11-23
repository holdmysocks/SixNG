//! Safe Metal IO command queues, command buffers, and compression streams.

use super::{
    Buffer, Device, IOCommandQueueDescriptor, IOCompressionMethod, IOScratchBuffer,
    IOScratchBufferAllocator, IOStatus, Origin, SharedEvent, Size, Texture,
};
use crate::Error;
use std::path::Path;

impl IOScratchBufferAllocator {
    /// Requests an allocator-owned scratch buffer of at least `minimum_size` bytes.
    pub fn new_scratch_buffer(
        &self,
        minimum_size: usize,
    ) -> Result<Option<IOScratchBuffer>, Error> {
        self.inner
            .new_scratch_buffer(minimum_size)
            .map(|value| value.map(IOScratchBuffer::from_ffi))
            .map_err(Error::from_ffi)
    }
}

/// An owned Metal IO file handle.
#[derive(Clone)]
pub struct IoFileHandle {
    inner: metal_rust_ffi::IoFileHandle,
}

impl IoFileHandle {
    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets or clears the debug label.
    pub fn set_label(&self, label: Option<&str>) {
        self.inner.set_label(label);
    }
}

/// An owned Metal IO command queue.
#[derive(Clone)]
pub struct IoCommandQueue {
    inner: metal_rust_ffi::IoCommandQueue,
}

impl IoCommandQueue {
    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets or clears the debug label.
    pub fn set_label(&self, label: Option<&str>) {
        self.inner.set_label(label);
    }

    /// Inserts a queue ordering barrier.
    pub fn enqueue_barrier(&self) -> Result<(), Error> {
        self.inner.enqueue_barrier().map_err(Error::from_ffi)
    }

    /// Creates a command buffer that retains every referenced object.
    pub fn command_buffer(&self) -> Result<IoCommandBuffer, Error> {
        self.inner
            .command_buffer()
            .map(|inner| IoCommandBuffer { inner })
            .map_err(Error::from_ffi)
    }

    /// Synchronously reads an owned byte range from an IO file handle.
    pub fn read_bytes(
        &self,
        source: &IoFileHandle,
        source_offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, Error> {
        self.inner
            .read_bytes(&source.inner, source_offset, length)
            .map_err(Error::from_ffi)
    }
}

/// A recording Metal IO command buffer.
pub struct IoCommandBuffer {
    inner: metal_rust_ffi::IoCommandBuffer,
}

impl IoCommandBuffer {
    /// Returns the current command-buffer status.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        self.inner.status()
    }

    /// Returns owned error information when Metal has reported one.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(Error::from_ffi)
    }

    /// Installs a one-shot, panic-isolated completion callback.
    pub fn on_complete(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .on_complete(move |result| handler(result.map_err(Error::from_ffi)))
            .map_err(Error::from_ffi)
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets or clears the debug label.
    pub fn set_label(&mut self, label: Option<&str>) {
        self.inner.set_label(label);
    }

    /// Pushes a debug group.
    pub fn push_debug_group(&mut self, label: &str) {
        self.inner.push_debug_group(label);
    }

    /// Pops the latest debug group.
    pub fn pop_debug_group(&mut self) {
        self.inner.pop_debug_group();
    }

    /// Adds an ordering barrier inside this command buffer.
    pub fn add_barrier(&mut self) -> Result<(), Error> {
        self.inner.add_barrier().map_err(Error::from_ffi)
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
        self.inner
            .load_buffer(
                &destination.inner,
                destination_offset,
                length,
                &source.inner,
                source_offset,
            )
            .map_err(Error::from_ffi)
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
        self.inner
            .load_texture(
                &destination.inner,
                slice,
                level,
                size,
                source_bytes_per_row,
                source_bytes_per_image,
                destination_origin,
                &source.inner,
                source_offset,
            )
            .map_err(Error::from_ffi)
    }

    /// Encodes the terminal status into a checked buffer location.
    pub fn copy_status_to_buffer(
        &mut self,
        destination: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        self.inner
            .copy_status_to_buffer(&destination.inner, offset)
            .map_err(Error::from_ffi)
    }

    /// Encodes a shared-event wait.
    pub fn wait_for_event(&mut self, event: &SharedEvent, value: u64) -> Result<(), Error> {
        self.inner
            .wait_for_event(&event.inner, value)
            .map_err(Error::from_ffi)
    }

    /// Encodes a shared-event signal.
    pub fn signal_event(&mut self, event: &SharedEvent, value: u64) -> Result<(), Error> {
        self.inner
            .signal_event(&event.inner, value)
            .map_err(Error::from_ffi)
    }

    /// Enqueues this command buffer before committing it.
    pub fn enqueue(&mut self) -> Result<(), Error> {
        self.inner.enqueue().map_err(Error::from_ffi)
    }

    /// Commits this command buffer exactly once.
    #[must_use]
    pub fn commit(self) -> SubmittedIoCommandBuffer {
        SubmittedIoCommandBuffer {
            inner: self.inner.commit(),
        }
    }
}

/// A committed Metal IO command buffer.
pub struct SubmittedIoCommandBuffer {
    inner: metal_rust_ffi::SubmittedIoCommandBuffer,
}

impl SubmittedIoCommandBuffer {
    /// Returns the current status without claiming completion.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        self.inner.status()
    }

    /// Returns owned error information when Metal has reported one.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(Error::from_ffi)
    }

    /// Requests cancellation without claiming completion.
    pub fn try_cancel(&self) -> Result<(), Error> {
        self.inner.try_cancel().map_err(Error::from_ffi)
    }

    /// Waits for successful completion.
    pub fn wait(self) -> Result<CompletedIoCommandBuffer, Error> {
        self.inner
            .wait()
            .map(|inner| CompletedIoCommandBuffer { inner })
            .map_err(Error::from_ffi)
    }
}

/// Proof that an IO command buffer completed successfully.
pub struct CompletedIoCommandBuffer {
    inner: metal_rust_ffi::CompletedIoCommandBuffer,
}

impl CompletedIoCommandBuffer {
    /// Returns the terminal status reported by Metal.
    #[must_use]
    pub fn status(&self) -> IOStatus {
        self.inner.status()
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Returns owned error information. A successful completion normally has
    /// no error.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(Error::from_ffi)
    }
}

impl Device {
    /// Creates an IO command queue from its descriptor.
    pub fn new_io_command_queue(
        &self,
        descriptor: &IOCommandQueueDescriptor,
    ) -> Result<IoCommandQueue, Error> {
        self.inner
            .new_io_command_queue(&descriptor.inner)
            .map(|inner| IoCommandQueue { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a raw or compressed IO file handle.
    pub fn new_io_file_handle(
        &self,
        path: impl AsRef<Path>,
        compression: Option<IOCompressionMethod>,
    ) -> Result<IoFileHandle, Error> {
        self.inner
            .new_io_file_handle(path.as_ref(), compression)
            .map(|inner| IoFileHandle { inner })
            .map_err(Error::from_ffi)
    }
}

/// An owned streaming Metal IO compression context.
pub struct IoCompressionContext {
    inner: metal_rust_ffi::IoCompressionContext,
}

impl IoCompressionContext {
    /// Returns Metal's preferred default chunk size when available.
    pub fn default_chunk_size() -> Result<usize, Error> {
        metal_rust_ffi::IoCompressionContext::default_chunk_size().map_err(Error::from_ffi)
    }

    /// Creates a compression stream that writes to `path`.
    pub fn new(
        path: impl AsRef<Path>,
        method: IOCompressionMethod,
        chunk_size: usize,
    ) -> Result<Self, Error> {
        metal_rust_ffi::IoCompressionContext::new(path.as_ref(), method, chunk_size)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Appends bytes to the stream.
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.inner.append(bytes).map_err(Error::from_ffi)
    }

    /// Flushes, destroys, and reports Metal's terminal status.
    pub fn finish(self) -> Result<(), Error> {
        self.inner.finish().map_err(Error::from_ffi)
    }
}
