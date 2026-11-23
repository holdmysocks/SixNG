//! Safe Foundation bundle facade.

use super::Error;

/// An owned Foundation bundle with no class or symbol pointer escape hatch.
pub struct Bundle {
    inner: metal_rust_ffi::Bundle,
}

impl Bundle {
    pub(crate) const fn as_ffi(&self) -> &metal_rust_ffi::Bundle {
        &self.inner
    }

    /// Returns the process main bundle.
    #[must_use]
    pub fn main() -> Self {
        Self {
            inner: metal_rust_ffi::Bundle::main(),
        }
    }

    /// Preflights and loads the bundle, returning an owned error on failure.
    pub fn load(&self) -> Result<(), Error> {
        self.inner.load().map_err(Error::from_ffi)
    }

    /// Returns whether the bundle is loaded.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.inner.is_loaded()
    }
}
