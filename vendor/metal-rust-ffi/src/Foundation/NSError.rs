use objc2_foundation::NSError;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Owned error information returned by an Apple framework call.
pub struct Error {
    /// The copied NSError domain, when one was returned.
    pub domain: Option<String>,
    /// The native NSError code, when one was returned.
    pub code: Option<isize>,
    /// The copied localized diagnostic string.
    pub message: String,
    /// Whether the FFI wrapper rejected an input before entering Metal.
    pub invalid_argument: bool,
}

impl Error {
    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            domain: None,
            code: None,
            message: message.into(),
            invalid_argument: true,
        }
    }

    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self {
            domain: None,
            code: None,
            message: message.into(),
            invalid_argument: false,
        }
    }
}

pub(crate) fn metal_error(error: &NSError) -> Error {
    Error {
        domain: Some(error.domain().to_string()),
        code: Some(error.code()),
        message: error.localizedDescription().to_string(),
        invalid_argument: false,
    }
}
