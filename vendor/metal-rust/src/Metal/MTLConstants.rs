//! Safe, weak-linked Metal string constants.

use crate::Error;
use crate::foundation::{ErrorDomain, ErrorUserInfoKey};
use metal_rust_ffi::MetalStringConstant as FfiConstant;

macro_rules! string_newtype {
    ($name:ident) => {
        #[doc = concat!("Owned Rust string value for Metal `", stringify!($name), "`.")]
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            /// Returns the copied framework identifier.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_newtype!(CommonCounter);
string_newtype!(CommonCounterSet);
string_newtype!(DeviceNotificationName);

fn resolve(value: FfiConstant) -> Result<String, Error> {
    metal_rust_ffi::metal_string_constant(value).map_err(Error::from_ffi)
}

macro_rules! counter_constructors {
    ($($name:ident => $variant:ident),+ $(,)?) => {
        $(
            #[doc = concat!("Resolves weak-linked Metal constant `", stringify!($variant), "`.")]
            pub fn $name() -> Result<Self, Error> {
                resolve(FfiConstant::$variant).map(Self)
            }
        )+
    };
}

impl CommonCounter {
    counter_constructors! {
        timestamp => CommonCounterTimestamp,
        tessellation_input_patches => CommonCounterTessellationInputPatches,
        vertex_invocations => CommonCounterVertexInvocations,
        post_tessellation_vertex_invocations => CommonCounterPostTessellationVertexInvocations,
        clipper_invocations => CommonCounterClipperInvocations,
        clipper_primitives_out => CommonCounterClipperPrimitivesOut,
        fragment_invocations => CommonCounterFragmentInvocations,
        fragments_passed => CommonCounterFragmentsPassed,
        compute_kernel_invocations => CommonCounterComputeKernelInvocations,
        total_cycles => CommonCounterTotalCycles,
        vertex_cycles => CommonCounterVertexCycles,
        tessellation_cycles => CommonCounterTessellationCycles,
        post_tessellation_vertex_cycles => CommonCounterPostTessellationVertexCycles,
        fragment_cycles => CommonCounterFragmentCycles,
        render_target_write_cycles => CommonCounterRenderTargetWriteCycles,
    }
}

impl CommonCounterSet {
    counter_constructors! {
        timestamp => CommonCounterSetTimestamp,
        stage_utilization => CommonCounterSetStageUtilization,
        statistic => CommonCounterSetStatistic,
    }
}

impl DeviceNotificationName {
    pub(crate) fn from_owned(value: String) -> Self {
        Self(value)
    }

    counter_constructors! {
        device_was_added => DeviceWasAddedNotification,
        device_removal_requested => DeviceRemovalRequestedNotification,
        device_was_removed => DeviceWasRemovedNotification,
    }
}

/// Resolves the binary-archive error domain.
pub fn binary_archive_error_domain() -> Result<ErrorDomain, Error> {
    resolve(FfiConstant::BinaryArchiveDomain).map(ErrorDomain::new)
}

/// Resolves the counter error domain.
pub fn counter_error_domain() -> Result<ErrorDomain, Error> {
    resolve(FfiConstant::CounterErrorDomain).map(ErrorDomain::new)
}

/// Resolves the Metal IO error domain.
pub fn io_error_domain() -> Result<ErrorDomain, Error> {
    resolve(FfiConstant::IOErrorDomain).map(ErrorDomain::new)
}

/// Resolves the log-state error domain.
pub fn log_state_error_domain() -> Result<ErrorDomain, Error> {
    resolve(FfiConstant::LogStateErrorDomain).map(ErrorDomain::new)
}

/// Resolves the tensor error domain.
pub fn tensor_error_domain() -> Result<ErrorDomain, Error> {
    resolve(FfiConstant::TensorDomain).map(ErrorDomain::new)
}

/// Resolves the command-buffer encoder-info user-info key.
pub fn command_buffer_encoder_info_error_key() -> Result<ErrorUserInfoKey, Error> {
    resolve(FfiConstant::CommandBufferEncoderInfoErrorKey).map(ErrorUserInfoKey::new)
}
