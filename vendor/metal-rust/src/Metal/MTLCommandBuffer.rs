//! Safe Metal command buffer lifecycle.

use super::{
    BlitCommandEncoder, BlitPassDescriptor, BufferReadback, CommandBufferErrorOption,
    CommandBufferStatus, ComputeCommandEncoder, ComputePassDescriptor, DispatchType, Event,
    LogContainer, RenderCommandEncoder, RenderPassDescriptor, ResidencySet,
};
use crate::{Drawable, Error};

impl super::CommandBufferEncoderInfo {
    /// Returns the encoder's optional debug signposts as owned Rust strings.
    pub fn debug_signposts(&self) -> Result<Option<Vec<String>>, Error> {
        self.inner.debug_signpost_strings().map_err(Error::from_ffi)
    }
}

/// A staged texture readback tied to the command submission that recorded it.
pub struct TextureReadback {
    pub(crate) inner: metal_rust_ffi::TextureReadback,
}

/// Owned texture bytes and their row layout.
pub struct TextureReadbackData {
    /// Bytes including any Metal-required row padding.
    pub bytes: Vec<u8>,
    /// Number of bytes between rows.
    pub bytes_per_row: usize,
    /// Width in texels.
    pub width: usize,
    /// Height in texels.
    pub height: usize,
}
/// An owned Metal command buffer in the recording state.
pub struct CommandBuffer {
    pub(crate) inner: metal_rust_ffi::CommandBuffer,
}

impl CommandBuffer {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::CommandBuffer) -> Self {
        Self { inner }
    }

    /// Registers a one-shot completion callback. Callback panics are isolated
    /// inside Metal-Rust and never unwind across the Objective-C block ABI.
    pub fn on_complete(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .on_complete(move |result| handler(result.map_err(Error::from_ffi)))
            .map_err(Error::from_ffi)
    }

    /// Registers a one-shot callback for the scheduled transition.
    pub fn on_scheduled(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .on_scheduled(move |result| handler(result.map_err(Error::from_ffi)))
            .map_err(Error::from_ffi)
    }

    /// Returns the device that owns this command buffer.
    #[must_use]
    pub fn device(&self) -> crate::Device {
        crate::Device::from_ffi(self.inner.device())
    }

    /// Returns the queue that created this command buffer.
    #[must_use]
    pub fn command_queue(&self) -> crate::CommandQueue {
        crate::CommandQueue::from_ffi(self.inner.command_queue())
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets the optional debug label.
    pub fn set_label(&self, value: Option<&str>) {
        self.inner.set_label(value);
    }

    /// Returns whether referenced resources are retained.
    #[must_use]
    pub fn retained_references(&self) -> bool {
        self.inner.retained_references()
    }

    /// Pushes a debug group.
    pub fn push_debug_group(&mut self, value: &str) {
        self.inner.push_debug_group(value);
    }

    /// Pops the most recent debug group.
    pub fn pop_debug_group(&mut self) {
        self.inner.pop_debug_group();
    }

    /// Schedules presentation at a host time in seconds.
    pub fn present_drawable_at_time(
        &mut self,
        drawable: &Drawable,
        presentation_time: f64,
    ) -> Result<(), Error> {
        self.inner
            .present_drawable_at_time(&drawable.inner, presentation_time)
            .map_err(Error::from_ffi)
    }

    /// Schedules presentation after a minimum previous-frame duration.
    pub fn present_drawable_after_minimum_duration(
        &mut self,
        drawable: &Drawable,
        duration: f64,
    ) -> Result<(), Error> {
        self.inner
            .present_drawable_after_minimum_duration(&drawable.inner, duration)
            .map_err(Error::from_ffi)
    }

    /// Returns the GPU execution start timestamp.
    #[must_use]
    pub fn gpu_start_time(&self) -> f64 {
        self.inner.gpu_start_time()
    }

    /// Returns the GPU execution end timestamp.
    #[must_use]
    pub fn gpu_end_time(&self) -> f64 {
        self.inner.gpu_end_time()
    }

    /// Returns the kernel execution start timestamp.
    #[must_use]
    pub fn kernel_start_time(&self) -> f64 {
        self.inner.kernel_start_time()
    }

    /// Returns the kernel execution end timestamp.
    #[must_use]
    pub fn kernel_end_time(&self) -> f64 {
        self.inner.kernel_end_time()
    }

    /// Begins a render encoder. The exclusive borrow prevents command
    /// submission while the encoder is active.
    pub fn render_encoder<'a>(
        &'a mut self,
        descriptor: &RenderPassDescriptor,
    ) -> Result<RenderCommandEncoder<'a>, Error> {
        self.inner
            .render_encoder(&descriptor.inner)
            .map(|inner| RenderCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a compute encoder. The exclusive borrow prevents command
    /// submission while the encoder is active.
    pub fn compute_encoder<'a>(
        &'a mut self,
        descriptor: &ComputePassDescriptor,
    ) -> Result<ComputeCommandEncoder<'a>, Error> {
        self.inner
            .compute_encoder(&descriptor.inner)
            .map(|inner| ComputeCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a compute encoder using Metal's default dispatch mode.
    pub fn compute_encoder_default<'a>(&'a mut self) -> Result<ComputeCommandEncoder<'a>, Error> {
        self.inner
            .compute_encoder_default()
            .map(|inner| ComputeCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a compute encoder in serial or concurrent dispatch mode.
    pub fn compute_encoder_with_dispatch_type<'a>(
        &'a mut self,
        dispatch_type: DispatchType,
    ) -> Result<ComputeCommandEncoder<'a>, Error> {
        self.inner
            .compute_encoder_with_dispatch_type(dispatch_type)
            .map(|inner| ComputeCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a blit encoder. The exclusive borrow prevents concurrent
    /// command-buffer mutation while the encoder is active.
    pub fn blit_encoder<'a>(&'a mut self) -> Result<BlitCommandEncoder<'a>, Error> {
        self.inner
            .blit_encoder()
            .map(|inner| BlitCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Begins a descriptor-configured blit encoder. The exclusive borrow
    /// prevents submission while the encoder is active.
    pub fn blit_encoder_with_descriptor<'a>(
        &'a mut self,
        descriptor: &BlitPassDescriptor,
    ) -> Result<BlitCommandEncoder<'a>, Error> {
        self.inner
            .blit_encoder_with_descriptor(&descriptor.inner)
            .map(|inner| BlitCommandEncoder { inner })
            .map_err(Error::from_ffi)
    }

    /// Encodes a wait until an event reaches `value`.
    pub fn encode_wait(&mut self, event: &Event, value: u64) -> Result<(), Error> {
        self.inner
            .encode_wait(&event.inner, value)
            .map_err(Error::from_ffi)
    }

    /// Encodes a signal that advances an event to `value`.
    pub fn encode_signal_event(&mut self, event: &Event, value: u64) -> Result<(), Error> {
        self.inner
            .encode_signal_event(&event.inner, value)
            .map_err(Error::from_ffi)
    }

    /// Marks residency sets for this command-buffer submission.
    pub fn use_residency_sets(&mut self, sets: &[&ResidencySet]) -> Result<(), Error> {
        let sets = sets.iter().map(|set| &set.inner).collect::<Vec<_>>();
        self.inner
            .use_residency_sets(&sets)
            .map_err(Error::from_ffi)
    }

    /// Explicitly enqueues this buffer once while keeping it in recording state.
    pub fn enqueue(&mut self) -> Result<(), Error> {
        self.inner.enqueue().map_err(Error::from_ffi)
    }

    /// Commits the command buffer exactly once and transitions it to the
    /// submitted state.
    #[must_use]
    pub fn commit(self) -> SubmittedCommandBuffer {
        SubmittedCommandBuffer {
            inner: self.inner.commit(),
        }
    }

    /// Schedules a drawable for presentation when this command buffer runs.
    pub fn present_drawable(&mut self, drawable: &Drawable) {
        self.inner.present_drawable(&drawable.inner);
    }

    /// Returns the current command buffer state.
    #[must_use]
    pub fn status(&self) -> CommandBufferStatus {
        self.inner.status()
    }

    /// Returns the native execution error, if the buffer failed.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(Error::from_ffi)
    }

    /// Returns the configured command-buffer error reporting options.
    pub fn error_options(&self) -> Result<CommandBufferErrorOption, Error> {
        self.inner.error_options().map_err(Error::from_ffi)
    }
}

/// A command buffer that has been committed to Metal.
pub struct SubmittedCommandBuffer {
    inner: metal_rust_ffi::SubmittedCommandBuffer,
}

impl SubmittedCommandBuffer {
    /// Waits until Metal schedules this already-submitted buffer.
    pub fn wait_until_scheduled(&self) -> Result<(), Error> {
        self.inner.wait_until_scheduled().map_err(Error::from_ffi)
    }

    /// Waits for GPU completion and transitions to the completed state.
    pub fn wait(self) -> Result<CompletedCommandBuffer, Error> {
        self.inner
            .wait()
            .map(|inner| CompletedCommandBuffer { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns the current submitted command-buffer state without waiting.
    #[must_use]
    pub fn status(&self) -> CommandBufferStatus {
        self.inner.status()
    }
}

/// A command buffer whose GPU work has completed successfully.
pub struct CompletedCommandBuffer {
    inner: metal_rust_ffi::CompletedCommandBuffer,
}

impl CompletedCommandBuffer {
    /// Returns function logs after successful GPU completion.
    pub fn logs(&self) -> Result<LogContainer, Error> {
        self.inner
            .logs()
            .map(LogContainer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Resolves a buffer readback recorded into this exact submission.
    pub fn resolve_buffer(&self, readback: BufferReadback) -> Result<Vec<u8>, Error> {
        self.inner
            .resolve_buffer(readback.inner)
            .map_err(Error::from_ffi)
    }

    /// Resolves a texture readback recorded into this exact submission.
    pub fn resolve_texture(&self, readback: TextureReadback) -> Result<TextureReadbackData, Error> {
        self.inner
            .resolve_texture(readback.inner)
            .map(|value| TextureReadbackData {
                bytes: value.bytes,
                bytes_per_row: value.bytes_per_row,
                width: value.width,
                height: value.height,
            })
            .map_err(Error::from_ffi)
    }

    /// Returns the terminal command-buffer state.
    #[must_use]
    pub fn status(&self) -> CommandBufferStatus {
        self.inner.status()
    }
}
