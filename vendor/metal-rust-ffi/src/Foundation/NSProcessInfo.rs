//! Audited Foundation process-information boundary.

use crate::metal::generated_struct_types::OperatingSystemVersion;
use crate::metal::generated_value_types::ProcessInfoThermalState;
use crate::{Error, ThreadBound};
use objc2::rc::Retained;
use objc2::{msg_send, sel};
use objc2_foundation::{NSObjectProtocol, NSOperatingSystemVersion, NSProcessInfo, NSString};

/// An owned Foundation process-information object.
pub struct ProcessInfo {
    inner: Retained<NSProcessInfo>,
    _thread_bound: ThreadBound,
}

impl ProcessInfo {
    /// Returns the process-wide information object.
    #[must_use]
    pub fn current() -> Self {
        Self {
            inner: NSProcessInfo::processInfo(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Sets the current process name from an owned Rust string view.
    pub fn set_process_name(&self, name: &str) {
        self.inner.setProcessName(&NSString::from_str(name));
    }

    /// Returns the process identifier.
    #[must_use]
    pub fn process_identifier(&self) -> i32 {
        self.inner.processIdentifier()
    }

    /// Returns the operating-system version.
    #[must_use]
    pub fn operating_system_version(&self) -> OperatingSystemVersion {
        let value = self.inner.operatingSystemVersion();
        OperatingSystemVersion {
            major_version: value.majorVersion,
            minor_version: value.minorVersion,
            patch_version: value.patchVersion,
        }
    }

    /// Tests an operating-system version requirement.
    #[must_use]
    pub fn is_operating_system_at_least(&self, version: &OperatingSystemVersion) -> bool {
        self.inner
            .isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
                majorVersion: version.major_version,
                minorVersion: version.minor_version,
                patchVersion: version.patch_version,
            })
    }

    /// Returns the configured processor count.
    #[must_use]
    pub fn processor_count(&self) -> usize {
        self.inner.processorCount()
    }

    /// Returns the currently active processor count.
    #[must_use]
    pub fn active_processor_count(&self) -> usize {
        self.inner.activeProcessorCount()
    }

    /// Returns physical memory in bytes.
    #[must_use]
    pub fn physical_memory(&self) -> u64 {
        self.inner.physicalMemory()
    }

    /// Returns system uptime in seconds.
    #[must_use]
    pub fn system_uptime(&self) -> f64 {
        self.inner.systemUptime()
    }

    /// Disables sudden termination for this process.
    pub fn disable_sudden_termination(&self) {
        self.inner.disableSuddenTermination();
    }

    /// Re-enables sudden termination for this process.
    pub fn enable_sudden_termination(&self) {
        self.inner.enableSuddenTermination();
    }

    /// Returns whether automatic termination is supported.
    #[must_use]
    pub fn automatic_termination_support_enabled(&self) -> bool {
        self.inner.automaticTerminationSupportEnabled()
    }

    /// Enables or disables automatic termination support.
    pub fn set_automatic_termination_support_enabled(&self, enabled: bool) {
        self.inner.setAutomaticTerminationSupportEnabled(enabled);
    }

    /// Returns the current thermal state.
    #[must_use]
    pub fn thermal_state(&self) -> ProcessInfoThermalState {
        ProcessInfoThermalState::from_system_raw(self.inner.thermalState().0)
    }

    /// Returns whether low-power mode is enabled.
    #[must_use]
    pub fn is_low_power_mode_enabled(&self) -> bool {
        self.inner.isLowPowerModeEnabled()
    }

    /// Returns whether this process is an iOS application on macOS.
    #[must_use]
    pub fn is_ios_app_on_mac(&self) -> bool {
        self.inner.isiOSAppOnMac()
    }

    /// Returns whether this process is a Mac Catalyst application.
    #[must_use]
    pub fn is_mac_catalyst_app(&self) -> bool {
        self.inner.isMacCatalystApp()
    }

    /// Queries a device-certification tier when the runtime selector exists.
    pub fn is_device_certified(&self, tier: isize) -> Result<bool, Error> {
        if !self.inner.respondsToSelector(sel!(isDeviceCertified:)) {
            return Err(Error::unsupported(
                "NSProcessInfo::isDeviceCertified is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and metal-cpp declares an
        // NSInteger input with a Boolean return value.
        Ok(unsafe { msg_send![&*self.inner, isDeviceCertified: tier] })
    }

    /// Queries a performance profile when the runtime selector exists.
    pub fn has_performance_profile(&self, profile: isize) -> Result<bool, Error> {
        if !self.inner.respondsToSelector(sel!(hasPerformanceProfile:)) {
            return Err(Error::unsupported(
                "NSProcessInfo::hasPerformanceProfile is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and metal-cpp declares an
        // NSInteger input with a Boolean return value.
        Ok(unsafe { msg_send![&*self.inner, hasPerformanceProfile: profile] })
    }
}
