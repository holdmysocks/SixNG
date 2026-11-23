//! Audited Foundation file-URL boundary.

use super::Error;
use crate::ThreadBound;
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL};
use std::ffi::CStr;
use std::path::{Path, PathBuf};

/// An owned Foundation file URL.
#[allow(clippy::upper_case_acronyms)]
pub struct URL {
    inner: Retained<NSURL>,
    _thread_bound: ThreadBound,
}

impl URL {
    /// Creates a file URL from a UTF-8-compatible Rust path.
    pub fn from_file_path(path: &Path) -> Result<Self, Error> {
        let path = path
            .to_str()
            .ok_or_else(|| Error::invalid_argument("file URL path is not valid UTF-8"))?;
        Ok(Self {
            inner: NSURL::fileURLWithPath(&NSString::from_str(path)),
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Copies the URL's file-system representation into an owned path.
    #[must_use]
    pub fn file_path(&self) -> PathBuf {
        let pointer = self.inner.fileSystemRepresentation();
        // SAFETY: Foundation promises a NUL-terminated representation whose
        // lifetime is at least that of the retained URL; it is copied here.
        let bytes = unsafe { CStr::from_ptr(pointer.as_ptr()) }.to_bytes();
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}
