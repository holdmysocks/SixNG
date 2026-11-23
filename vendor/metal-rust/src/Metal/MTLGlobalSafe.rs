//! Safe global Metal discovery and reference-version helpers.

use super::{Device, DeviceNotificationName};
use crate::Error;

/// RAII gate for a repeatable Metal log handler.
pub struct LogHandlerRegistration {
    _inner: metal_rust_ffi::LogHandlerRegistration,
}

impl super::LogState {
    /// Adds a Rust-native log handler and returns its delivery registration.
    pub fn add_log_handler(
        &self,
        handler: impl Fn(String, String, super::LogLevel, String) + Send + Sync + 'static,
    ) -> Result<LogHandlerRegistration, Error> {
        self.inner
            .add_log_handler(handler)
            .map(|inner| LogHandlerRegistration { _inner: inner })
            .map_err(Error::from_ffi)
    }
}

/// RAII registration for repeatable Metal device notifications.
///
/// Dropping this value unregisters the Objective-C observer, disables future
/// Rust callback entries, and waits for callbacks executing on other threads.
pub struct DeviceObserverRegistration {
    _inner: metal_rust_ffi::DeviceObserverRegistration,
}

impl Device {
    /// Returns every Metal device currently available as owned safe wrappers.
    #[must_use]
    pub fn all() -> Vec<Self> {
        metal_rust_ffi::all_devices()
            .into_iter()
            .map(Self::from_ffi)
            .collect()
    }

    /// Returns current devices and installs a repeatable RAII observer.
    pub fn all_with_observer(
        handler: impl Fn(Device, DeviceNotificationName) + Send + Sync + 'static,
    ) -> Result<(Vec<Self>, DeviceObserverRegistration), Error> {
        metal_rust_ffi::all_devices_with_observer(move |device, name| {
            handler(
                Device::from_ffi(device),
                DeviceNotificationName::from_owned(name),
            );
        })
        .map(|(devices, registration)| {
            (
                devices.into_iter().map(Self::from_ffi).collect(),
                DeviceObserverRegistration {
                    _inner: registration,
                },
            )
        })
        .map_err(Error::from_ffi)
    }
}

/// Major version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_MAJOR: u32 = metal_rust_ffi::METAL_CPP_VERSION_MAJOR;
/// Minor version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_MINOR: u32 = metal_rust_ffi::METAL_CPP_VERSION_MINOR;
/// Patch version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_PATCH: u32 = metal_rust_ffi::METAL_CPP_VERSION_PATCH;

/// Returns whether the local metal-cpp reference is at least the given version.
#[must_use]
pub const fn metal_cpp_supports_version(major: u32, minor: u32, patch: u32) -> bool {
    metal_rust_ffi::metal_cpp_supports_version(major, minor, patch)
}
