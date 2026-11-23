//! Private ABI bridge for MetalFX SIMD matrices.

use crate::Matrix4x4;
use objc2::runtime::AnyObject;

unsafe extern "C" {
    fn metal_rust_metalfx_get_world_to_view(object: *const AnyObject, output: *mut Matrix4x4);
    fn metal_rust_metalfx_set_world_to_view(object: *const AnyObject, value: Matrix4x4);
    fn metal_rust_metalfx_get_view_to_clip(object: *const AnyObject, output: *mut Matrix4x4);
    fn metal_rust_metalfx_set_view_to_clip(object: *const AnyObject, value: Matrix4x4);
}

pub(crate) fn world_to_view(object: &AnyObject) -> Matrix4x4 {
    let mut value = Matrix4x4::identity();
    // SAFETY: the generator calls this only after checking the exact selector;
    // the output has the C shim's matching 16-byte-aligned matrix layout.
    unsafe { metal_rust_metalfx_get_world_to_view(object, &mut value) };
    value
}

pub(crate) fn set_world_to_view(object: &AnyObject, value: Matrix4x4) {
    // SAFETY: selector availability is checked by the caller and Matrix4x4 has
    // the exact size and alignment asserted by the Objective-C shim.
    unsafe { metal_rust_metalfx_set_world_to_view(object, value) };
}

pub(crate) fn view_to_clip(object: &AnyObject) -> Matrix4x4 {
    let mut value = Matrix4x4::identity();
    // SAFETY: see `world_to_view`; this uses the matching view-to-clip selector.
    unsafe { metal_rust_metalfx_get_view_to_clip(object, &mut value) };
    value
}

pub(crate) fn set_view_to_clip(object: &AnyObject, value: Matrix4x4) {
    // SAFETY: selector availability is checked by the caller and the value has
    // the exact shim layout.
    unsafe { metal_rust_metalfx_set_view_to_clip(object, value) };
}
