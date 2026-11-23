//! Audited command buffer lifecycle and encoder borrowing boundary.

use crate::ThreadBound;
use crate::foundation::{Error, metal_error};
use crate::metal::{
    BlitCommandEncoder, Buffer, CommandQueue, ComputeCommandEncoder, ComputePassDescriptor, Device,
    RenderCommandEncoder, RenderPassDescriptor,
};
use crate::quartz_core::Drawable;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSObjectProtocol, NSString};
use objc2_metal::{MTLCommandBuffer, MTLDrawable};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_SUBMISSION_ID: AtomicU64 = AtomicU64::new(1);

impl crate::metal::generated_object_types::metal::CommandBufferEncoderInfo {
    /// Returns optional owned debug signpost strings after validating availability.
    pub fn debug_signpost_strings(&self) -> Result<Option<Vec<String>>, Error> {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // Sel/bool use the runtime's stable declared ABIs.
        let available: bool =
            unsafe { msg_send![self.as_inner(), respondsToSelector: sel!(debugSignposts)] };
        if !available {
            return Err(Error::unsupported(
                "MTLCommandBufferEncoderInfo::debugSignposts is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and Metal declares the
        // result as a retained NSArray whose elements are NSString objects.
        let values: Option<Retained<NSArray<NSString>>> =
            unsafe { msg_send![self.as_inner(), debugSignposts] };
        Ok(values.map(|values| values.iter().map(|value| value.to_string()).collect()))
    }
}

/// An owned Metal command buffer.
pub struct CommandBuffer {
    pub(crate) inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    submission_id: u64,
    enqueued: bool,
    _thread_bound: ThreadBound,
}

impl CommandBuffer {
    pub(super) fn new(inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>) -> Self {
        Self {
            inner,
            submission_id: NEXT_SUBMISSION_ID.fetch_add(1, Ordering::Relaxed),
            enqueued: false,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Installs a one-shot callback invoked when GPU execution completes.
    pub fn on_complete(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(addCompletedHandler:)) {
            return Err(Error::unsupported(
                "MTLCommandBuffer::addCompletedHandler is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = block2::RcBlock::new(
            move |command_buffer: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                // SAFETY: Metal invokes the block with a non-null command
                // buffer that remains alive for the duration of this call.
                let command_buffer = unsafe { command_buffer.as_ref() };
                let result = command_buffer
                    .error()
                    .map_or(Ok(()), |error| Err(metal_error(&error)));
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector availability was checked; `block` has the exact
        // MTLCommandBufferHandler ABI and Metal copies it for later delivery.
        unsafe {
            let _: () = msg_send![&*self.inner, addCompletedHandler: &*block];
        }
        Ok(())
    }

    /// Installs a one-shot callback invoked once scheduling finishes.
    pub fn on_scheduled(
        &mut self,
        handler: impl FnOnce(Result<(), Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(addScheduledHandler:)) {
            return Err(Error::unsupported(
                "MTLCommandBuffer::addScheduledHandler is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = block2::RcBlock::new(
            move |command_buffer: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                let callback = callback_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let Some(callback) = callback else {
                    return;
                };
                // SAFETY: Metal invokes the block with a non-null command
                // buffer that remains alive for the duration of this call.
                let command_buffer = unsafe { command_buffer.as_ref() };
                let result = command_buffer
                    .error()
                    .map_or(Ok(()), |error| Err(metal_error(&error)));
                let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
            },
        );
        // SAFETY: selector availability was checked; `block` has the exact
        // MTLCommandBufferHandler ABI and Metal copies it for later delivery.
        unsafe {
            let _: () = msg_send![&*self.inner, addScheduledHandler: &*block];
        }
        Ok(())
    }

    /// Returns the device that owns this command buffer.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Returns the queue that created this command buffer.
    #[must_use]
    pub fn command_queue(&self) -> CommandQueue {
        CommandQueue::new(self.inner.commandQueue())
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }

    /// Sets the optional debug label.
    pub fn set_label(&self, value: Option<&str>) {
        let value = value.map(objc2_foundation::NSString::from_str);
        self.inner.setLabel(value.as_deref());
    }

    /// Returns whether the command buffer retains referenced resources.
    #[must_use]
    pub fn retained_references(&self) -> bool {
        self.inner.retainedReferences()
    }

    /// Pushes an owned Rust debug-group string.
    pub fn push_debug_group(&mut self, value: &str) {
        let value = objc2_foundation::NSString::from_str(value);
        self.inner.pushDebugGroup(&value);
    }

    /// Pops the most recently pushed debug group.
    pub fn pop_debug_group(&mut self) {
        self.inner.popDebugGroup();
    }

    /// Presents a drawable at a host time in seconds.
    pub fn present_drawable_at_time(
        &mut self,
        drawable: &Drawable,
        presentation_time: f64,
    ) -> Result<(), Error> {
        if !presentation_time.is_finite() || presentation_time < 0.0 {
            return Err(Error::invalid_argument(
                "presentation time must be finite and non-negative",
            ));
        }
        let drawable = ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable.inner);
        self.inner
            .presentDrawable_atTime(drawable, presentation_time);
        Ok(())
    }

    /// Presents after the previous frame has remained visible for a duration.
    pub fn present_drawable_after_minimum_duration(
        &mut self,
        drawable: &Drawable,
        duration: f64,
    ) -> Result<(), Error> {
        if !duration.is_finite() || duration < 0.0 {
            return Err(Error::invalid_argument(
                "minimum presentation duration must be finite and non-negative",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(presentDrawable:afterMinimumDuration:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::presentDrawableAfterMinimumDuration is unavailable",
            ));
        }
        let drawable = ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable.inner);
        self.inner
            .presentDrawable_afterMinimumDuration(drawable, duration);
        Ok(())
    }

    /// Returns the GPU execution start timestamp.
    #[must_use]
    pub fn gpu_start_time(&self) -> f64 {
        self.inner.GPUStartTime()
    }

    /// Returns the GPU execution end timestamp.
    #[must_use]
    pub fn gpu_end_time(&self) -> f64 {
        self.inner.GPUEndTime()
    }

    /// Returns the kernel execution start timestamp.
    #[must_use]
    pub fn kernel_start_time(&self) -> f64 {
        self.inner.kernelStartTime()
    }

    /// Returns the kernel execution end timestamp.
    #[must_use]
    pub fn kernel_end_time(&self) -> f64 {
        self.inner.kernelEndTime()
    }

    /// Begins a render encoder while borrowing the command buffer exclusively.
    pub fn render_encoder<'a>(
        &'a mut self,
        descriptor: &RenderPassDescriptor,
    ) -> Result<RenderCommandEncoder<'a>, Error> {
        self.inner
            .renderCommandEncoderWithDescriptor(&descriptor.inner)
            .map(|inner| RenderCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a render encoder"))
    }

    /// Begins a compute encoder while borrowing the command buffer exclusively.
    pub fn compute_encoder<'a>(
        &'a mut self,
        descriptor: &ComputePassDescriptor,
    ) -> Result<ComputeCommandEncoder<'a>, Error> {
        self.inner
            .computeCommandEncoderWithDescriptor(&descriptor.inner)
            .map(|inner| ComputeCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a compute encoder"))
    }

    /// Begins a compute encoder using Metal's default dispatch mode.
    pub fn compute_encoder_default<'a>(&'a mut self) -> Result<ComputeCommandEncoder<'a>, Error> {
        if !self.inner.respondsToSelector(sel!(computeCommandEncoder)) {
            return Err(Error::unsupported(
                "MTLCommandBuffer::computeCommandEncoder is unavailable",
            ));
        }
        self.inner
            .computeCommandEncoder()
            .map(|inner| ComputeCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a compute encoder"))
    }

    /// Begins a compute encoder with a validated serial or concurrent dispatch mode.
    pub fn compute_encoder_with_dispatch_type<'a>(
        &'a mut self,
        dispatch_type: crate::metal::generated_value_types::DispatchType,
    ) -> Result<ComputeCommandEncoder<'a>, Error> {
        if !dispatch_type.is_valid() {
            return Err(Error::invalid_argument(
                "dispatch type is not declared by Metal",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(computeCommandEncoderWithDispatchType:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::computeCommandEncoderWithDispatchType is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; DispatchType is validated
        // and has the same NSUInteger representation as MTLDispatchType. The
        // retained protocol result is borrowed exclusively through `self`.
        let inner: Option<Retained<ProtocolObject<dyn objc2_metal::MTLComputeCommandEncoder>>> = unsafe {
            msg_send![&*self.inner, computeCommandEncoderWithDispatchType: dispatch_type.as_raw()]
        };
        inner
            .map(|inner| ComputeCommandEncoder::new(inner, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a compute encoder"))
    }

    /// Begins a blit encoder while borrowing the command buffer exclusively.
    pub fn blit_encoder<'a>(&'a mut self) -> Result<BlitCommandEncoder<'a>, Error> {
        self.inner
            .blitCommandEncoder()
            .map(|inner| BlitCommandEncoder::new(inner, self.submission_id, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a blit encoder"))
    }

    /// Begins a descriptor-configured blit encoder while preserving the
    /// command-buffer exclusive borrow.
    pub fn blit_encoder_with_descriptor<'a>(
        &'a mut self,
        descriptor: &crate::metal::generated_object_types::metal::BlitPassDescriptor,
    ) -> Result<BlitCommandEncoder<'a>, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(blitCommandEncoderWithDescriptor:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::blitCommandEncoderWithDescriptor is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the descriptor wrapper
        // owns an MTLBlitPassDescriptor and the returned protocol object stays
        // alive inside the encoder's exclusive command-buffer borrow.
        let inner: Option<Retained<ProtocolObject<dyn objc2_metal::MTLBlitCommandEncoder>>> = unsafe {
            msg_send![&*self.inner, blitCommandEncoderWithDescriptor: descriptor.as_inner()]
        };
        inner
            .map(|inner| BlitCommandEncoder::new(inner, self.submission_id, self))
            .ok_or_else(|| Error::unsupported("Metal could not create a blit encoder"))
    }

    /// Encodes a wait on an event without exposing the Objective-C protocol object.
    pub fn encode_wait(
        &mut self,
        event: &crate::metal::generated_object_types::metal::Event,
        value: u64,
    ) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(encodeWaitForEvent:value:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::encodeWaitForEvent is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the generated Event
        // wrapper owns the Objective-C event for the duration of this call.
        unsafe {
            let _: () = msg_send![&*self.inner, encodeWaitForEvent: event.as_inner(), value: value];
        }
        Ok(())
    }

    /// Encodes an event signal without exposing the Objective-C protocol object.
    pub fn encode_signal_event(
        &mut self,
        event: &crate::metal::generated_object_types::metal::Event,
        value: u64,
    ) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(encodeSignalEvent:value:))
        {
            return Err(Error::unsupported(
                "MTLCommandBuffer::encodeSignalEvent is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the generated Event
        // wrapper owns the Objective-C event for the duration of this call.
        unsafe {
            let _: () = msg_send![&*self.inner, encodeSignalEvent: event.as_inner(), value: value];
        }
        Ok(())
    }

    /// Marks residency sets for this submission using a safe Rust slice.
    pub fn use_residency_sets(
        &mut self,
        sets: &[&crate::metal::generated_object_types::metal::ResidencySet],
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(useResidencySet:)) {
            return Err(Error::unsupported(
                "MTLCommandBuffer::useResidencySet is unavailable",
            ));
        }
        for set in sets {
            // SAFETY: selector availability was checked and each generated
            // wrapper owns a residency-set object through the call.
            unsafe {
                let _: () = msg_send![&*self.inner, useResidencySet: set.as_inner()];
            }
        }
        Ok(())
    }

    /// Explicitly enqueues this buffer once while it remains in recording state.
    pub fn enqueue(&mut self) -> Result<(), Error> {
        if self.enqueued {
            return Err(Error::invalid_argument(
                "a command buffer cannot be explicitly enqueued more than once",
            ));
        }
        self.inner.enqueue();
        self.enqueued = true;
        Ok(())
    }

    /// Commits the command buffer for execution.
    pub fn commit(self) -> SubmittedCommandBuffer {
        self.inner.commit();
        SubmittedCommandBuffer {
            inner: self.inner,
            submission_id: self.submission_id,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Schedules a drawable for presentation when this command buffer runs.
    pub fn present_drawable(&mut self, drawable: &Drawable) {
        let drawable = ProtocolObject::<dyn MTLDrawable>::from_ref(&*drawable.inner);
        self.inner.presentDrawable(drawable);
    }

    /// Returns the current command buffer state.
    #[must_use]
    pub fn status(&self) -> crate::metal::CommandBufferStatus {
        self.inner.status().into()
    }
    /// Returns the native execution error, if the buffer failed.
    #[must_use]
    pub fn error(&self) -> Option<Error> {
        self.inner.error().map(|error| metal_error(&error))
    }

    /// Returns the configured command-buffer error reporting options.
    pub fn error_options(
        &self,
    ) -> Result<crate::metal::generated_value_types::CommandBufferErrorOption, Error> {
        if !self.inner.respondsToSelector(sel!(errorOptions)) {
            return Err(Error::unsupported(
                "MTLCommandBuffer::errorOptions is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the option set uses
        // NSUInteger representation. System-returned bits are preserved.
        let raw: usize = unsafe { msg_send![&*self.inner, errorOptions] };
        Ok(crate::metal::generated_value_types::CommandBufferErrorOption::from_system_raw(raw))
    }
}

/// A staging buffer whose contents are bound to one command submission.
pub struct BufferReadback {
    pub(crate) buffer: Buffer,
    pub(crate) submission_id: u64,
    pub(crate) length: usize,
}

/// A staged two-dimensional texture readback bound to one submission.
pub struct TextureReadback {
    pub(crate) buffer: Buffer,
    pub(crate) submission_id: u64,
    pub(crate) length: usize,
    pub(crate) bytes_per_row: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

/// Owned texture bytes together with their validated row layout.
pub struct TextureReadbackData {
    /// Staging bytes, including any Metal-required row padding.
    pub bytes: Vec<u8>,
    /// Number of bytes between adjacent rows.
    pub bytes_per_row: usize,
    /// Readback width in texels.
    pub width: usize,
    /// Readback height in texels.
    pub height: usize,
}

/// A command buffer that has been submitted exactly once.
pub struct SubmittedCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    submission_id: u64,
    _thread_bound: ThreadBound,
}

impl SubmittedCommandBuffer {
    /// Waits until Metal schedules this already-submitted buffer.
    pub fn wait_until_scheduled(&self) -> Result<(), Error> {
        self.inner.waitUntilScheduled();
        if let Some(error) = self.inner.error() {
            return Err(metal_error(&error));
        }
        match crate::metal::CommandBufferStatus::from(self.inner.status()) {
            crate::metal::CommandBufferStatus::Scheduled
            | crate::metal::CommandBufferStatus::Completed => Ok(()),
            crate::metal::CommandBufferStatus::Error => Err(Error::unsupported(
                "Metal command buffer entered an error state while scheduling",
            )),
            _ => Err(Error::unsupported(
                "Metal command buffer did not reach the scheduled state",
            )),
        }
    }

    /// Waits for Metal to reach a terminal state.
    pub fn wait(self) -> Result<CompletedCommandBuffer, Error> {
        self.inner.waitUntilCompleted();
        if let Some(error) = self.inner.error() {
            return Err(metal_error(&error));
        }
        if crate::metal::CommandBufferStatus::from(self.inner.status())
            != crate::metal::CommandBufferStatus::Completed
        {
            return Err(Error::unsupported(
                "Metal command buffer did not reach the completed state",
            ));
        }
        Ok(CompletedCommandBuffer {
            inner: self.inner,
            submission_id: self.submission_id,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Returns the current submitted command-buffer state.
    #[must_use]
    pub fn status(&self) -> crate::metal::CommandBufferStatus {
        self.inner.status().into()
    }
}

/// A successfully completed command buffer.
pub struct CompletedCommandBuffer {
    inner: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    submission_id: u64,
    _thread_bound: ThreadBound,
}

impl CompletedCommandBuffer {
    /// Returns the owned function-log container after successful completion.
    pub fn logs(&self) -> Result<crate::metal::generated_object_types::metal::LogContainer, Error> {
        if !self.inner.respondsToSelector(sel!(logs)) {
            return Err(Error::unsupported("MTLCommandBuffer::logs is unavailable"));
        }
        // SAFETY: selector availability was checked; logs is only queried on
        // a successfully completed buffer, and the retained object is moved
        // into the generated owned wrapper.
        let logs: Option<Retained<AnyObject>> = unsafe { msg_send![&*self.inner, logs] };
        logs.map(crate::metal::generated_object_types::metal::LogContainer::from_inner)
            .ok_or_else(|| Error::unsupported("Metal returned no command-buffer logs"))
    }

    /// Resolves a staging readback created while recording this submission.
    pub fn resolve_buffer(&self, readback: BufferReadback) -> Result<Vec<u8>, Error> {
        if readback.submission_id != self.submission_id {
            return Err(Error::invalid_argument(
                "readback belongs to a different command submission",
            ));
        }
        readback.buffer.completed_bytes(readback.length)
    }

    /// Resolves a staged texture readback from this exact submission.
    pub fn resolve_texture(&self, readback: TextureReadback) -> Result<TextureReadbackData, Error> {
        if readback.submission_id != self.submission_id {
            return Err(Error::invalid_argument(
                "texture readback belongs to a different command submission",
            ));
        }
        Ok(TextureReadbackData {
            bytes: readback.buffer.completed_bytes(readback.length)?,
            bytes_per_row: readback.bytes_per_row,
            width: readback.width,
            height: readback.height,
        })
    }

    /// Returns the terminal command-buffer state.
    #[must_use]
    pub fn status(&self) -> crate::metal::CommandBufferStatus {
        self.inner.status().into()
    }
}
