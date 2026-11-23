//! Audited weak-link lookup for Metal string constants.

use crate::foundation::Error;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use std::ffi::{CStr, c_char, c_void};

#[link(name = "System")]
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// Closed set of Metal NSString constants that Metal-Rust may resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum MetalStringConstant {
    BinaryArchiveDomain,
    CounterErrorDomain,
    CommonCounterTimestamp,
    CommonCounterTessellationInputPatches,
    CommonCounterVertexInvocations,
    CommonCounterPostTessellationVertexInvocations,
    CommonCounterClipperInvocations,
    CommonCounterClipperPrimitivesOut,
    CommonCounterFragmentInvocations,
    CommonCounterFragmentsPassed,
    CommonCounterComputeKernelInvocations,
    CommonCounterTotalCycles,
    CommonCounterVertexCycles,
    CommonCounterTessellationCycles,
    CommonCounterPostTessellationVertexCycles,
    CommonCounterFragmentCycles,
    CommonCounterRenderTargetWriteCycles,
    CommonCounterSetTimestamp,
    CommonCounterSetStageUtilization,
    CommonCounterSetStatistic,
    DeviceWasAddedNotification,
    DeviceRemovalRequestedNotification,
    DeviceWasRemovedNotification,
    CommandBufferEncoderInfoErrorKey,
    IOErrorDomain,
    LogStateErrorDomain,
    TensorDomain,
}

impl MetalStringConstant {
    const fn symbol(self) -> &'static CStr {
        match self {
            Self::BinaryArchiveDomain => c"MTLBinaryArchiveDomain",
            Self::CounterErrorDomain => c"MTLCounterErrorDomain",
            Self::CommonCounterTimestamp => c"MTLCommonCounterTimestamp",
            Self::CommonCounterTessellationInputPatches => {
                c"MTLCommonCounterTessellationInputPatches"
            }
            Self::CommonCounterVertexInvocations => c"MTLCommonCounterVertexInvocations",
            Self::CommonCounterPostTessellationVertexInvocations => {
                c"MTLCommonCounterPostTessellationVertexInvocations"
            }
            Self::CommonCounterClipperInvocations => c"MTLCommonCounterClipperInvocations",
            Self::CommonCounterClipperPrimitivesOut => c"MTLCommonCounterClipperPrimitivesOut",
            Self::CommonCounterFragmentInvocations => c"MTLCommonCounterFragmentInvocations",
            Self::CommonCounterFragmentsPassed => c"MTLCommonCounterFragmentsPassed",
            Self::CommonCounterComputeKernelInvocations => {
                c"MTLCommonCounterComputeKernelInvocations"
            }
            Self::CommonCounterTotalCycles => c"MTLCommonCounterTotalCycles",
            Self::CommonCounterVertexCycles => c"MTLCommonCounterVertexCycles",
            Self::CommonCounterTessellationCycles => c"MTLCommonCounterTessellationCycles",
            Self::CommonCounterPostTessellationVertexCycles => {
                c"MTLCommonCounterPostTessellationVertexCycles"
            }
            Self::CommonCounterFragmentCycles => c"MTLCommonCounterFragmentCycles",
            Self::CommonCounterRenderTargetWriteCycles => {
                c"MTLCommonCounterRenderTargetWriteCycles"
            }
            Self::CommonCounterSetTimestamp => c"MTLCommonCounterSetTimestamp",
            Self::CommonCounterSetStageUtilization => c"MTLCommonCounterSetStageUtilization",
            Self::CommonCounterSetStatistic => c"MTLCommonCounterSetStatistic",
            Self::DeviceWasAddedNotification => c"MTLDeviceWasAddedNotification",
            Self::DeviceRemovalRequestedNotification => c"MTLDeviceRemovalRequestedNotification",
            Self::DeviceWasRemovedNotification => c"MTLDeviceWasRemovedNotification",
            Self::CommandBufferEncoderInfoErrorKey => c"MTLCommandBufferEncoderInfoErrorKey",
            Self::IOErrorDomain => c"MTLIOErrorDomain",
            Self::LogStateErrorDomain => c"MTLLogStateErrorDomain",
            Self::TensorDomain => c"MTLTensorDomain",
        }
    }
}

/// Copies one of the closed, audited Metal NSString constants into Rust.
pub fn metal_string_constant(value: MetalStringConstant) -> Result<String, Error> {
    let symbol = value.symbol();
    let handle = (-2_isize) as *mut c_void;
    // SAFETY: `handle` is Darwin RTLD_DEFAULT and the enum can only select a
    // static NUL-terminated symbol declared by an Apple Metal header.
    let address = unsafe { dlsym(handle, symbol.as_ptr()) };
    if address.is_null() {
        return Err(Error::unsupported(format!(
            "Metal constant {symbol:?} is unavailable"
        )));
    }
    // SAFETY: every enum variant names exported `NSString * const` storage.
    let object = unsafe { address.cast::<*mut NSString>().read() };
    // SAFETY: a non-null NSString constant is a valid process-lifetime object;
    // retaining it establishes owned access while its contents are copied.
    let object = unsafe { Retained::retain(object) }
        .ok_or_else(|| Error::unsupported("Metal exported a null string constant"))?;
    Ok(object.to_string())
}
