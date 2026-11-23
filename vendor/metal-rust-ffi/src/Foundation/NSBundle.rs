//! Audited Foundation bundle boundary.

use super::{Error, metal_error};
use crate::ThreadBound;
use objc2::rc::Retained;
use objc2_foundation::NSBundle;

/// An owned Foundation bundle.
pub struct Bundle {
    inner: Retained<NSBundle>,
    _thread_bound: ThreadBound,
}

impl Bundle {
    pub(crate) fn as_inner(&self) -> &NSBundle {
        &self.inner
    }

    /// Returns the process main bundle.
    #[must_use]
    pub fn main() -> Self {
        Self {
            inner: NSBundle::mainBundle(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Loads the retained bundle after Foundation preflight validation.
    pub fn load(&self) -> Result<(), Error> {
        self.inner
            .preflightAndReturnError()
            .map_err(|error| metal_error(&error))?;
        // SAFETY: preflight succeeded, the bundle is retained for the call,
        // and no Objective-C class or symbol pointer is exposed to safe Rust.
        unsafe { self.inner.loadAndReturnError() }.map_err(|error| metal_error(&error))
    }

    /// Returns whether the bundle is currently loaded.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.inner.isLoaded()
    }
}
