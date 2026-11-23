//! Weak-linked Foundation/Metal process-information constants.

use super::Error;
use std::ffi::{CStr, c_char, c_void};

#[link(name = "System")]
unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

fn integer_constant(symbols: &[&CStr]) -> Result<isize, Error> {
    // Darwin defines RTLD_DEFAULT as `(void *)-2`. Each candidate is a
    // NUL-terminated static string and dlsym does not retain its pointer.
    let handle = (-2_isize) as *mut c_void;
    for symbol in symbols {
        // SAFETY: `handle` is Darwin's RTLD_DEFAULT sentinel and `symbol`
        // points to a valid NUL-terminated C string for the duration of call.
        let address = unsafe { dlsym(handle, symbol.as_ptr()) };
        if !address.is_null() {
            // SAFETY: these exported symbols are declared by Apple's SDK as
            // immutable NSInteger constants, so the resolved address is
            // aligned, initialized, and valid for a read of one isize.
            return Ok(unsafe { address.cast::<isize>().read() });
        }
    }
    Err(Error::unsupported(format!(
        "runtime constant {:?} is unavailable",
        symbols[0]
    )))
}

/// Resolves `NSDeviceCertificationiPhonePerformanceGaming` when available.
pub fn device_certification_iphone_performance_gaming() -> Result<isize, Error> {
    integer_constant(&[
        c"NSDeviceCertificationiPhonePerformanceGaming",
        c"MTLDeviceCertificationiPhonePerformanceGaming",
    ])
}

/// Resolves `NSProcessPerformanceProfileDefault` when available.
pub fn process_performance_profile_default() -> Result<isize, Error> {
    integer_constant(&[
        c"NSProcessPerformanceProfileDefault",
        c"MTLProcessPerformanceProfileDefault",
    ])
}

/// Resolves `NSProcessPerformanceProfileSustained` when available.
pub fn process_performance_profile_sustained() -> Result<isize, Error> {
    integer_constant(&[
        c"NSProcessPerformanceProfileSustained",
        c"MTLProcessPerformanceProfileSustained",
    ])
}
