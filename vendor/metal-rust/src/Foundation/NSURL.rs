//! Safe Foundation file-URL facade.

use super::Error;
use std::path::{Path, PathBuf};

/// An owned Foundation URL with Rust path conversion.
#[allow(clippy::upper_case_acronyms)]
pub struct URL {
    inner: metal_rust_ffi::URL,
}

impl URL {
    /// Creates a file URL from a Rust path.
    pub fn from_file_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        metal_rust_ffi::URL::from_file_path(path.as_ref())
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }

    /// Returns an owned file-system path.
    #[must_use]
    pub fn file_path(&self) -> PathBuf {
        self.inner.file_path()
    }
}
