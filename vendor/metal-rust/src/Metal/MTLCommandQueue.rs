//! Safe Metal command queues.

use super::{CommandBuffer, CommandBufferDescriptor, Device, ResidencySet};
use crate::Error;

/// An owned Metal command queue.
pub struct CommandQueue {
    pub(crate) inner: metal_rust_ffi::CommandQueue,
}

impl CommandQueue {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::CommandQueue) -> Self {
        Self { inner }
    }

    /// Returns the device that created this queue.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_ffi(self.inner.device())
    }

    /// Returns the optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets a debug label.
    pub fn set_label(&self, label: Option<&str>) {
        self.inner.set_label(label);
    }

    /// Creates a command buffer with normal resource retention.
    pub fn command_buffer(&self) -> Result<CommandBuffer, Error> {
        self.inner
            .command_buffer()
            .map(CommandBuffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a command buffer using the requested retention and error
    /// reporting options.
    pub fn command_buffer_with_descriptor(
        &self,
        descriptor: &CommandBufferDescriptor,
    ) -> Result<CommandBuffer, Error> {
        if !descriptor.retained_references()? {
            return Err(Error::invalid_argument(
                "safe command buffers must retain every referenced resource",
            ));
        }
        self.inner
            .command_buffer_with_descriptor(&descriptor.inner)
            .map(CommandBuffer::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Inserts a debug capture boundary.
    pub fn insert_debug_capture_boundary(&self) {
        self.inner.insert_debug_capture_boundary();
    }

    /// Adds residency sets from a safe Rust slice.
    pub fn add_residency_sets(&self, sets: &[&ResidencySet]) -> Result<(), Error> {
        let sets = sets.iter().map(|set| &set.inner).collect::<Vec<_>>();
        self.inner
            .add_residency_sets(&sets)
            .map_err(Error::from_ffi)
    }

    /// Removes residency sets from a safe Rust slice.
    pub fn remove_residency_sets(&self, sets: &[&ResidencySet]) -> Result<(), Error> {
        let sets = sets.iter().map(|set| &set.inner).collect::<Vec<_>>();
        self.inner
            .remove_residency_sets(&sets)
            .map_err(Error::from_ffi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn descriptor_cannot_disable_resource_retention() {
        let Some(device) = Device::system_default() else {
            return;
        };
        let Ok(queue) = device.new_command_queue(None) else {
            return;
        };
        let Ok(descriptor) = CommandBufferDescriptor::new() else {
            return;
        };
        if descriptor.set_retained_references(false).is_err() {
            return;
        }

        let error = match queue.command_buffer_with_descriptor(&descriptor) {
            Ok(_) => panic!("non-retaining command buffer must be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    }
}
