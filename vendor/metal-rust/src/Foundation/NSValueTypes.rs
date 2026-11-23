use std::borrow::Cow;
use std::fmt;

macro_rules! string_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Cow<'static, str>);

        impl $name {
            /// Creates a value from an owned or borrowed Rust string.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(Cow::Owned(value.into()))
            }

            /// Creates a value from a static framework identifier.
            #[must_use]
            pub const fn from_static(value: &'static str) -> Self {
                Self(Cow::Borrowed(value))
            }

            /// Returns the underlying string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_ref()
            }

            /// Consumes the newtype and returns its owned string.
            #[must_use]
            pub fn into_string(self) -> String {
                self.0.into_owned()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(Cow::Owned(value))
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(Cow::Owned(value.to_owned()))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_newtype!(ErrorDomain, "Owned Foundation error-domain identifier.");
string_newtype!(
    ErrorUserInfoKey,
    "Owned Foundation error user-info dictionary key."
);
string_newtype!(
    NotificationName,
    "Owned Foundation notification identifier."
);

/// A runtime-validated Foundation device-certification tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeviceCertification(isize);

impl DeviceCertification {
    /// Returns the iPhone-performance gaming tier when this SDK/runtime exports it.
    pub fn iphone_performance_gaming() -> Result<Self, super::Error> {
        metal_rust_ffi::device_certification_iphone_performance_gaming()
            .map(Self)
            .map_err(super::Error::from_ffi)
    }

    pub(crate) const fn as_raw(self) -> isize {
        self.0
    }
}

/// A runtime-validated Foundation process-performance profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessPerformanceProfile(isize);

impl ProcessPerformanceProfile {
    /// Returns the default performance profile when this SDK/runtime exports it.
    pub fn default_profile() -> Result<Self, super::Error> {
        metal_rust_ffi::process_performance_profile_default()
            .map(Self)
            .map_err(super::Error::from_ffi)
    }

    /// Returns the sustained performance profile when this SDK/runtime exports it.
    pub fn sustained() -> Result<Self, super::Error> {
        metal_rust_ffi::process_performance_profile_sustained()
            .map(Self)
            .map_err(super::Error::from_ffi)
    }

    pub(crate) const fn as_raw(self) -> isize {
        self.0
    }
}

/// Safe Rust representation of an immutable Foundation numeric value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    /// Boolean value.
    Bool(bool),
    /// Signed integer value.
    Signed(i64),
    /// Unsigned integer value.
    Unsigned(u64),
    /// IEEE-754 floating-point value.
    Float(f64),
}

impl From<bool> for Number {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        Self::Signed(value)
    }
}

impl From<u64> for Number {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl Number {
    /// Converts to a signed integer using Rust's saturating float-cast rules.
    #[must_use]
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Bool(value) => i64::from(value),
            Self::Signed(value) => value,
            Self::Unsigned(value) => value as i64,
            Self::Float(value) => value as i64,
        }
    }

    /// Converts to an unsigned integer using Rust's saturating float-cast rules.
    #[must_use]
    pub fn as_u64(self) -> u64 {
        match self {
            Self::Bool(value) => u64::from(value),
            Self::Signed(value) => value as u64,
            Self::Unsigned(value) => value,
            Self::Float(value) => value as u64,
        }
    }

    /// Converts to a double-precision floating-point value.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Bool(value) => f64::from(value),
            Self::Signed(value) => value as f64,
            Self::Unsigned(value) => value as f64,
            Self::Float(value) => value,
        }
    }

    /// Converts to a Boolean using zero/non-zero semantics.
    #[must_use]
    pub fn as_bool(self) -> bool {
        match self {
            Self::Bool(value) => value,
            Self::Signed(value) => value != 0,
            Self::Unsigned(value) => value != 0,
            Self::Float(value) => value != 0.0,
        }
    }

    /// Compares numeric values; returns `None` for NaN.
    #[must_use]
    pub fn compare(self, other: Self) -> Option<std::cmp::Ordering> {
        self.as_f64().partial_cmp(&other.as_f64())
    }
}
