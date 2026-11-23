use super::ErrorDomain;
use std::fmt;

/// High-level category for a Metal operation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The platform does not expose the requested Metal capability.
    Unsupported,
    /// The caller supplied an invalid safe API argument.
    InvalidArgument,
    /// Metal rejected an otherwise valid operation.
    Metal,
}

/// Owned error returned by the safe Metal API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    domain: Option<ErrorDomain>,
    code: Option<isize>,
    message: String,
}

impl Error {
    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::InvalidArgument,
            domain: None,
            code: None,
            message: message.into(),
        }
    }

    pub(crate) fn from_ffi(error: metal_rust_ffi::Error) -> Self {
        Self {
            kind: if error.invalid_argument {
                ErrorKind::InvalidArgument
            } else if error.code.is_some() {
                ErrorKind::Metal
            } else {
                ErrorKind::Unsupported
            },
            domain: error.domain.map(ErrorDomain::new),
            code: error.code,
            message: error.message,
        }
    }

    /// Returns the high-level error category.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the native NSError code when Metal supplied one.
    #[must_use]
    pub const fn code(&self) -> Option<isize> {
        self.code
    }

    /// Returns the native NSError domain when Metal supplied one.
    #[must_use]
    pub fn domain(&self) -> Option<&ErrorDomain> {
        self.domain.as_ref()
    }

    /// Returns an owned diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}
