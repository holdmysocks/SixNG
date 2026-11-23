//! Audited command queue ownership and submission boundary.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::{CommandBuffer, Device};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, sel};
use objc2_foundation::NSObjectProtocol;
use objc2_metal::MTLCommandQueue;

/// An owned Metal command queue.
pub struct CommandQueue {
    pub(super) inner: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    _thread_bound: ThreadBound,
}

impl CommandQueue {
    pub(super) const fn new(inner: Retained<ProtocolObject<dyn MTLCommandQueue>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the device that created this queue.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|label| label.to_string())
    }

    /// Sets a debug label.
    pub fn set_label(&self, label: Option<&str>) {
        let label = label.map(objc2_foundation::NSString::from_str);
        self.inner.setLabel(label.as_deref());
    }

    /// Creates a retained command buffer with normal resource retention.
    pub fn command_buffer(&self) -> Result<CommandBuffer, Error> {
        self.inner
            .commandBuffer()
            .map(CommandBuffer::new)
            .ok_or_else(|| Error::unsupported("Metal could not create a command buffer"))
    }

    /// Creates a command buffer using a checked descriptor.
    pub fn command_buffer_with_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::CommandBufferDescriptor,
    ) -> Result<CommandBuffer, Error> {
        if !descriptor.retained_references()? {
            return Err(Error::invalid_argument(
                "safe command buffers must retain every referenced resource",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(commandBufferWithDescriptor:))
        {
            return Err(Error::unsupported(
                "MTLCommandQueue::commandBufferWithDescriptor is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the descriptor wrapper
        // owns an MTLCommandBufferDescriptor and the retained result uses the
        // declared MTLCommandBuffer protocol return convention.
        let inner: Option<Retained<ProtocolObject<dyn objc2_metal::MTLCommandBuffer>>> =
            unsafe { msg_send![&*self.inner, commandBufferWithDescriptor: descriptor.as_inner()] };
        inner.map(CommandBuffer::new).ok_or_else(|| {
            Error::unsupported("Metal could not create the requested command buffer")
        })
    }

    /// Inserts a debug capture boundary.
    #[allow(deprecated)]
    pub fn insert_debug_capture_boundary(&self) {
        self.inner.insertDebugCaptureBoundary();
    }

    /// Adds each residency set without exposing pointer arrays.
    pub fn add_residency_sets(
        &self,
        sets: &[&crate::metal::generated_object_types::metal::ResidencySet],
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(addResidencySet:)) {
            return Err(Error::unsupported(
                "MTLCommandQueue::addResidencySet is unavailable",
            ));
        }
        for set in sets {
            // SAFETY: the fixed selector accepts one MTLResidencySet object;
            // the generated owned wrapper retains the exact runtime object.
            unsafe {
                let _: () = msg_send![&*self.inner, addResidencySet: set.as_inner()];
            }
        }
        Ok(())
    }

    /// Removes each residency set without exposing pointer arrays.
    pub fn remove_residency_sets(
        &self,
        sets: &[&crate::metal::generated_object_types::metal::ResidencySet],
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(removeResidencySet:)) {
            return Err(Error::unsupported(
                "MTLCommandQueue::removeResidencySet is unavailable",
            ));
        }
        for set in sets {
            // SAFETY: the fixed selector accepts one MTLResidencySet object;
            // the generated owned wrapper retains the exact runtime object.
            unsafe {
                let _: () = msg_send![&*self.inner, removeResidencySet: set.as_inner()];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_cannot_disable_resource_retention() {
        let Some(device) = Device::system_default() else {
            return;
        };
        let Ok(queue) = device.new_command_queue(None) else {
            return;
        };
        let Ok(descriptor) =
            crate::metal::generated_object_types::metal::CommandBufferDescriptor::new()
        else {
            return;
        };
        if descriptor.set_retained_references(false).is_err() {
            return;
        }

        let error = match queue.command_buffer_with_descriptor(&descriptor) {
            Ok(_) => panic!("unsafe unretained command buffer must be rejected"),
            Err(error) => error,
        };
        assert!(error.invalid_argument);
    }
}
