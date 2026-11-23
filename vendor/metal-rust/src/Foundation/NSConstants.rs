//! Rust-native Foundation constant values used by the safe facade.

use super::{ErrorDomain, ErrorUserInfoKey, NotificationName};

/// Notification posted after a bundle loads.
pub const BUNDLE_DID_LOAD_NOTIFICATION: NotificationName =
    NotificationName::from_static("NSBundleDidLoadNotification");
/// Notification posted when an on-demand bundle request encounters low disk space.
pub const BUNDLE_RESOURCE_REQUEST_LOW_DISK_SPACE_NOTIFICATION: NotificationName =
    NotificationName::from_static("NSBundleResourceRequestLowDiskSpaceNotification");
/// Notification posted when the process thermal state changes.
pub const PROCESS_INFO_THERMAL_STATE_DID_CHANGE_NOTIFICATION: NotificationName =
    NotificationName::from_static("NSProcessInfoThermalStateDidChangeNotification");
/// Notification posted when the process power state changes.
pub const PROCESS_INFO_POWER_STATE_DID_CHANGE_NOTIFICATION: NotificationName =
    NotificationName::from_static("NSProcessInfoPowerStateDidChangeNotification");
/// Notification posted when the process performance profile changes.
pub const PROCESS_INFO_PERFORMANCE_PROFILE_DID_CHANGE_NOTIFICATION: NotificationName =
    NotificationName::from_static("NSProcessInfoPerformanceProfileDidChangeNotification");

/// Foundation Cocoa error domain.
pub const COCOA_ERROR_DOMAIN: ErrorDomain = ErrorDomain::from_static("NSCocoaErrorDomain");
/// Foundation POSIX error domain.
pub const POSIX_ERROR_DOMAIN: ErrorDomain = ErrorDomain::from_static("NSPOSIXErrorDomain");
/// Foundation OSStatus error domain.
pub const OS_STATUS_ERROR_DOMAIN: ErrorDomain = ErrorDomain::from_static("NSOSStatusErrorDomain");
/// Foundation Mach error domain.
pub const MACH_ERROR_DOMAIN: ErrorDomain = ErrorDomain::from_static("NSMachErrorDomain");

/// Underlying-error user-info key.
pub const UNDERLYING_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSUnderlyingErrorKey");
/// Localized-description user-info key.
pub const LOCALIZED_DESCRIPTION_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSLocalizedDescription");
/// Localized failure-reason user-info key.
pub const LOCALIZED_FAILURE_REASON_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSLocalizedFailureReason");
/// Localized recovery-suggestion user-info key.
pub const LOCALIZED_RECOVERY_SUGGESTION_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSLocalizedRecoverySuggestion");
/// Localized recovery-options user-info key.
pub const LOCALIZED_RECOVERY_OPTIONS_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSLocalizedRecoveryOptions");
/// Recovery-attempter user-info key.
pub const RECOVERY_ATTEMPTER_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSRecoveryAttempter");
/// Help-anchor user-info key.
pub const HELP_ANCHOR_ERROR_KEY: ErrorUserInfoKey = ErrorUserInfoKey::from_static("NSHelpAnchor");
/// Debug-description user-info key.
pub const DEBUG_DESCRIPTION_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSDebugDescription");
/// Localized-failure user-info key.
pub const LOCALIZED_FAILURE_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSLocalizedFailure");
/// String-encoding user-info key.
pub const STRING_ENCODING_ERROR_KEY: ErrorUserInfoKey =
    ErrorUserInfoKey::from_static("NSStringEncodingErrorKey");
/// URL user-info key.
pub const URL_ERROR_KEY: ErrorUserInfoKey = ErrorUserInfoKey::from_static("NSURL");
/// File-path user-info key.
pub const FILE_PATH_ERROR_KEY: ErrorUserInfoKey = ErrorUserInfoKey::from_static("NSFilePath");

/// Maximum signed Foundation integer.
pub const INTEGER_MAX: isize = isize::MAX;
/// Minimum signed Foundation integer.
pub const INTEGER_MIN: isize = isize::MIN;
/// Maximum unsigned Foundation integer.
pub const UINTEGER_MAX: usize = usize::MAX;
/// Sentinel used when a Foundation search does not find a value.
pub const NOT_FOUND: isize = isize::MAX;
