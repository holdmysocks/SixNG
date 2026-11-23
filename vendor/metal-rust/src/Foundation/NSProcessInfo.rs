//! Safe Foundation process-information facade.

use super::{
    DeviceCertification, Error, OperatingSystemVersion, ProcessInfoThermalState,
    ProcessPerformanceProfile,
};

/// An owned view of Foundation process information.
pub struct ProcessInfo {
    inner: metal_rust_ffi::ProcessInfo,
}

impl ProcessInfo {
    /// Returns the process-wide information object.
    #[must_use]
    pub fn current() -> Self {
        Self {
            inner: metal_rust_ffi::ProcessInfo::current(),
        }
    }

    /// Sets the process name.
    pub fn set_process_name(&self, name: &str) {
        self.inner.set_process_name(name);
    }

    /// Returns the process identifier.
    #[must_use]
    pub fn process_identifier(&self) -> i32 {
        self.inner.process_identifier()
    }

    /// Returns the operating-system version.
    #[must_use]
    pub fn operating_system_version(&self) -> OperatingSystemVersion {
        self.inner.operating_system_version()
    }

    /// Tests an operating-system version requirement.
    #[must_use]
    pub fn is_operating_system_at_least(&self, version: &OperatingSystemVersion) -> bool {
        self.inner.is_operating_system_at_least(version)
    }

    /// Returns the configured processor count.
    #[must_use]
    pub fn processor_count(&self) -> usize {
        self.inner.processor_count()
    }

    /// Returns the currently active processor count.
    #[must_use]
    pub fn active_processor_count(&self) -> usize {
        self.inner.active_processor_count()
    }

    /// Returns physical memory in bytes.
    #[must_use]
    pub fn physical_memory(&self) -> u64 {
        self.inner.physical_memory()
    }

    /// Returns system uptime in seconds.
    #[must_use]
    pub fn system_uptime(&self) -> f64 {
        self.inner.system_uptime()
    }

    /// Disables sudden termination.
    pub fn disable_sudden_termination(&self) {
        self.inner.disable_sudden_termination();
    }

    /// Enables sudden termination.
    pub fn enable_sudden_termination(&self) {
        self.inner.enable_sudden_termination();
    }

    /// Returns whether automatic termination support is enabled.
    #[must_use]
    pub fn automatic_termination_support_enabled(&self) -> bool {
        self.inner.automatic_termination_support_enabled()
    }

    /// Sets automatic termination support.
    pub fn set_automatic_termination_support_enabled(&self, enabled: bool) {
        self.inner
            .set_automatic_termination_support_enabled(enabled);
    }

    /// Returns the current thermal state.
    #[must_use]
    pub fn thermal_state(&self) -> ProcessInfoThermalState {
        self.inner.thermal_state()
    }

    /// Returns whether low-power mode is enabled.
    #[must_use]
    pub fn is_low_power_mode_enabled(&self) -> bool {
        self.inner.is_low_power_mode_enabled()
    }

    /// Returns whether this is an iOS application running on macOS.
    #[must_use]
    pub fn is_ios_app_on_mac(&self) -> bool {
        self.inner.is_ios_app_on_mac()
    }

    /// Returns whether this is a Mac Catalyst application.
    #[must_use]
    pub fn is_mac_catalyst_app(&self) -> bool {
        self.inner.is_mac_catalyst_app()
    }

    /// Queries a device-certification tier with an availability check.
    pub fn is_device_certified(&self, tier: DeviceCertification) -> Result<bool, Error> {
        self.inner
            .is_device_certified(tier.as_raw())
            .map_err(Error::from_ffi)
    }

    /// Queries a performance profile with an availability check.
    pub fn has_performance_profile(
        &self,
        profile: ProcessPerformanceProfile,
    ) -> Result<bool, Error> {
        self.inner
            .has_performance_profile(profile.as_raw())
            .map_err(Error::from_ffi)
    }
}
