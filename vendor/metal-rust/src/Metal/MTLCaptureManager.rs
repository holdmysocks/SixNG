//! Safe capture sessions and balanced capture scopes.

use super::{CaptureDescriptor, CaptureDestination, CaptureScope, CommandQueue, Device};
use crate::Error;
use crate::metal4::CommandQueue as Metal4CommandQueue;
use std::path::{Path, PathBuf};

fn capture_path(path: Option<&Path>) -> Result<Option<&str>, Error> {
    path.map(|path| {
        if !path.is_absolute() {
            return Err(Error::invalid_argument(
                "capture output path must be absolute",
            ));
        }
        let path = path
            .to_str()
            .ok_or_else(|| Error::invalid_argument("capture output path must be UTF-8"))?;
        if path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("capture output path contains NUL"));
        }
        Ok(path)
    })
    .transpose()
}

impl CaptureDescriptor {
    /// Returns the descriptor's typed capture target.
    pub fn capture_target(&self) -> Result<Option<CaptureTarget>, Error> {
        if let Some(device) = self.inner.capture_device().map_err(Error::from_ffi)? {
            return Ok(Some(CaptureTarget::Device(Device::from_ffi(device))));
        }
        if let Some(queue) = self
            .inner
            .capture_command_queue()
            .map_err(Error::from_ffi)?
        {
            return Ok(Some(CaptureTarget::CommandQueue(CommandQueue::from_ffi(
                queue,
            ))));
        }
        if let Some(scope) = self.inner.capture_scope().map_err(Error::from_ffi)? {
            return Ok(Some(CaptureTarget::Scope(CaptureScope::from_ffi(scope))));
        }
        self.inner
            .validate_capture_object_type()
            .map_err(Error::from_ffi)?;
        Ok(None)
    }

    /// Selects a device as this descriptor's capture target.
    pub fn set_capture_device(&self, device: &Device) -> Result<(), Error> {
        self.inner
            .set_capture_device(&device.inner)
            .map_err(Error::from_ffi)
    }

    /// Selects a command queue as this descriptor's capture target.
    pub fn set_capture_command_queue(&self, queue: &CommandQueue) -> Result<(), Error> {
        self.inner
            .set_capture_command_queue(&queue.inner)
            .map_err(Error::from_ffi)
    }

    /// Selects a capture scope as this descriptor's capture target.
    pub fn set_capture_scope(&self, scope: &CaptureScope) -> Result<(), Error> {
        self.inner
            .set_capture_scope(&scope.inner)
            .map_err(Error::from_ffi)
    }

    /// Sets the destination and its optional absolute output file path.
    ///
    /// Developer Tools capture requires `None`; GPU trace document capture
    /// requires `Some(absolute_path)`.
    pub fn set_capture_output(
        &self,
        destination: CaptureDestination,
        output_path: Option<&Path>,
    ) -> Result<(), Error> {
        let output_path = capture_path(output_path)?;
        self.inner
            .set_capture_output(destination, output_path)
            .map_err(Error::from_ffi)
    }

    /// Returns the configured output file path, when present.
    pub fn output_path(&self) -> Result<Option<PathBuf>, Error> {
        self.inner.output_path().map_err(Error::from_ffi)
    }
}

/// A retained, type-checked capture descriptor target.
pub enum CaptureTarget {
    /// All command queues belonging to a device.
    Device(Device),
    /// One Metal command queue.
    CommandQueue(CommandQueue),
    /// Commands bracketed by a capture scope.
    Scope(CaptureScope),
}

impl CaptureScope {
    /// Runs an operation inside a balanced capture scope.
    ///
    /// Metal-Rust ends the scope during normal return and panic unwinding, so
    /// callers never issue an unpaired `beginScope` or `endScope` operation.
    pub fn with_scope<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Error> {
        self.inner.with_scope(operation).map_err(Error::from_ffi)
    }

    /// Returns the device associated with this scope.
    pub fn device(&self) -> Result<Device, Error> {
        self.inner
            .device()
            .map(Device::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the queue associated with this scope, if queue-specific.
    pub fn command_queue(&self) -> Result<Option<CommandQueue>, Error> {
        self.inner
            .command_queue()
            .map(|queue| queue.map(CommandQueue::from_ffi))
            .map_err(Error::from_ffi)
    }
}

/// An active device, queue, or scope Metal capture.
///
/// Dropping the session stops capture. Call [`finish`](Self::finish) when an
/// explicit frame boundary is available.
pub struct CaptureSession {
    inner: metal_rust_ffi::CaptureSession,
}

impl CaptureSession {
    /// Returns whether the current runtime supports a destination.
    pub fn supports_destination(destination: CaptureDestination) -> Result<bool, Error> {
        metal_rust_ffi::CaptureSession::supports_destination(destination).map_err(Error::from_ffi)
    }

    /// Returns whether the process currently has an active capture.
    pub fn is_capturing() -> Result<bool, Error> {
        metal_rust_ffi::CaptureSession::is_capturing().map_err(Error::from_ffi)
    }

    /// Creates a device-wide capture scope.
    pub fn new_scope_for_device(device: &Device) -> Result<CaptureScope, Error> {
        metal_rust_ffi::CaptureSession::new_scope_for_device(&device.inner)
            .map(CaptureScope::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a scope limited to one command queue.
    pub fn new_scope_for_command_queue(queue: &CommandQueue) -> Result<CaptureScope, Error> {
        metal_rust_ffi::CaptureSession::new_scope_for_command_queue(&queue.inner)
            .map(CaptureScope::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a scope limited to one Metal 4 command queue.
    pub fn new_scope_for_mtl4_command_queue(
        queue: &Metal4CommandQueue,
    ) -> Result<CaptureScope, Error> {
        metal_rust_ffi::CaptureSession::new_scope_for_mtl4_command_queue(queue.as_ffi())
            .map(CaptureScope::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the process default capture scope.
    pub fn default_scope() -> Result<Option<CaptureScope>, Error> {
        metal_rust_ffi::CaptureSession::default_scope()
            .map(|scope| scope.map(CaptureScope::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the process default capture scope.
    pub fn set_default_scope(scope: Option<&CaptureScope>) -> Result<(), Error> {
        metal_rust_ffi::CaptureSession::set_default_scope(scope.map(|scope| &scope.inner))
            .map_err(Error::from_ffi)
    }

    /// Starts capture using a fully configured descriptor.
    pub fn start_descriptor(descriptor: &CaptureDescriptor) -> Result<Self, Error> {
        metal_rust_ffi::CaptureSession::start_descriptor(&descriptor.inner)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Starts device-wide GPU trace capture at an absolute output path.
    pub fn start(device: &Device, output_path: &Path) -> Result<Self, Error> {
        let path = capture_path(Some(output_path))?.expect("a path was supplied");
        metal_rust_ffi::CaptureSession::start(&device.inner, path)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Starts a device capture using a validated destination configuration.
    pub fn start_for_device(
        device: &Device,
        destination: CaptureDestination,
        output_path: Option<&Path>,
    ) -> Result<Self, Error> {
        let output_path = capture_path(output_path)?;
        metal_rust_ffi::CaptureSession::start_for_device(&device.inner, destination, output_path)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Starts a command-queue capture using a validated destination.
    pub fn start_for_command_queue(
        queue: &CommandQueue,
        destination: CaptureDestination,
        output_path: Option<&Path>,
    ) -> Result<Self, Error> {
        let output_path = capture_path(output_path)?;
        metal_rust_ffi::CaptureSession::start_for_command_queue(
            &queue.inner,
            destination,
            output_path,
        )
        .map(|inner| Self { inner })
        .map_err(Error::from_ffi)
    }

    /// Starts capture of commands bracketed by a capture scope.
    pub fn start_for_scope(
        scope: &CaptureScope,
        destination: CaptureDestination,
        output_path: Option<&Path>,
    ) -> Result<Self, Error> {
        let output_path = capture_path(output_path)?;
        metal_rust_ffi::CaptureSession::start_for_scope(&scope.inner, destination, output_path)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Stops capture immediately and consumes the session.
    pub fn finish(self) {
        self.inner.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::capture_path;
    use crate::ErrorKind;
    use std::path::Path;

    #[test]
    fn capture_path_requires_an_absolute_path() {
        let error = capture_path(Some(Path::new("trace.gputrace"))).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    }

    #[test]
    fn capture_path_preserves_none_for_developer_tools() {
        assert_eq!(capture_path(None).unwrap(), None);
    }
}
