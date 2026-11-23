//! Audited resource lifecycle and resource-view-pool operations.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{Resource, ResourceViewPool};
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::generated_value_types::PurgeableState;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_foundation::NSRange;
use objc2_metal::MTLResourceID;
use std::ops::Range;

type MachPort = u32;
type KernReturn = i32;

const KERN_SUCCESS: KernReturn = 0;

unsafe extern "C" {
    static mach_task_self_: MachPort;
    fn task_create_identity_token(task: MachPort, token: *mut MachPort) -> KernReturn;
    fn mach_port_deallocate(task: MachPort, name: MachPort) -> KernReturn;
}

fn responds_to(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: every Objective-C object implements respondsToSelector:, and
    // selector and bool use the runtime's declared ABI encodings.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

fn require_selector(object: &AnyObject, selector: Sel, message: &'static str) -> Result<(), Error> {
    if responds_to(object, selector) {
        Ok(())
    } else {
        Err(Error::unsupported(message))
    }
}

/// Owned task identity used for Metal VM-accounting ownership.
///
/// The Mach send right is acquired and released inside the implementation
/// boundary, so callers never handle a raw port name or its lifetime rules.
#[derive(Debug)]
pub struct ResourceOwnerIdentity {
    token: MachPort,
}

impl ResourceOwnerIdentity {
    /// Creates an identity token for the current process.
    pub fn current_process() -> Result<Self, Error> {
        let mut token = 0;
        // SAFETY: `token` points to initialized writable storage and
        // `mach_task_self_` is the current task's live send right.
        let result = unsafe { task_create_identity_token(mach_task_self_, &mut token) };
        if result != KERN_SUCCESS || token == 0 {
            return Err(Error::unsupported(format!(
                "failed to create the current task identity token (kern_return_t={result})"
            )));
        }
        Ok(Self { token })
    }
}

impl Drop for ResourceOwnerIdentity {
    fn drop(&mut self) {
        // SAFETY: `token` is the owned send right returned by
        // task_create_identity_token, and this Drop releases it exactly once.
        let _ = unsafe { mach_port_deallocate(mach_task_self_, self.token) };
    }
}

impl Resource {
    /// Returns whether this heap allocation has been made aliasable.
    pub fn is_aliasable(&self) -> Result<bool, Error> {
        require_selector(
            self.as_inner(),
            sel!(isAliasable),
            "resource alias-state queries are unavailable",
        )?;
        // SAFETY: the selector is present and returns Objective-C BOOL.
        Ok(unsafe { msg_send![self.as_inner(), isAliasable] })
    }

    /// Makes a direct heap allocation available for future aliasing.
    pub fn make_aliasable(&self) -> Result<(), Error> {
        if self.heap()?.is_none() {
            return Err(Error::invalid_argument(
                "only resources allocated directly from a heap can be made aliasable",
            ));
        }

        // Metal explicitly rejects texture views. The optional rootResource
        // selector distinguishes those from their direct heap allocation.
        if responds_to(self.as_inner(), sel!(rootResource)) {
            // SAFETY: rootResource is present and has an object-or-nil result.
            let root: Option<Retained<AnyObject>> =
                unsafe { msg_send![self.as_inner(), rootResource] };
            if root.is_some() {
                return Err(Error::invalid_argument(
                    "texture views cannot be made aliasable independently of their root resource",
                ));
            }
        }

        require_selector(
            self.as_inner(),
            sel!(makeAliasable),
            "resource aliasing is unavailable",
        )?;
        // SAFETY: the selector is present; the heap and root-resource checks
        // above exclude the illegal receiver categories documented by Metal.
        unsafe {
            let _: () = msg_send![self.as_inner(), makeAliasable];
        }
        Ok(())
    }

    /// Assigns VM-accounting ownership using an opaque, owned task identity.
    pub fn set_owner(&self, identity: &ResourceOwnerIdentity) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setOwnerWithIdentity:),
            "resource owner assignment is unavailable",
        )?;
        // SAFETY: the selector is present and `identity` keeps the Mach task
        // identity send right live for the duration of the call.
        let result: KernReturn =
            unsafe { msg_send![self.as_inner(), setOwnerWithIdentity: identity.token] };
        if result == KERN_SUCCESS {
            Ok(())
        } else {
            Err(Error::unsupported(format!(
                "Metal rejected the resource owner identity (kern_return_t={result})"
            )))
        }
    }

    /// Changes the resource's purgeable state after validating the enum.
    pub fn set_purgeable_state(&self, state: PurgeableState) -> Result<PurgeableState, Error> {
        if !state.is_valid() {
            return Err(Error::invalid_argument("invalid resource purgeable state"));
        }
        require_selector(
            self.as_inner(),
            sel!(setPurgeableState:),
            "resource purgeable-state mutation is unavailable",
        )?;
        // SAFETY: the selector is present and the input has a declared Metal
        // enumeration value.
        let raw: usize = unsafe { msg_send![self.as_inner(), setPurgeableState: state.as_raw()] };
        PurgeableState::try_from(raw)
            .map_err(|()| Error::unsupported("Metal returned an unknown purgeable state"))
    }
}

fn checked_copy_ranges(
    source_count: usize,
    destination_count: usize,
    source_range: Range<usize>,
    destination_index: usize,
) -> Result<NSRange, Error> {
    if source_range.start > source_range.end || source_range.end > source_count {
        return Err(Error::invalid_argument(
            "resource-view source range is out of bounds",
        ));
    }
    let length = source_range.end - source_range.start;
    if length == 0 {
        return Err(Error::invalid_argument(
            "resource-view source range must not be empty",
        ));
    }
    let destination_end = destination_index
        .checked_add(length)
        .ok_or_else(|| Error::invalid_argument("resource-view destination range overflow"))?;
    if destination_end > destination_count {
        return Err(Error::invalid_argument(
            "resource-view destination range is out of bounds",
        ));
    }
    Ok(NSRange::new(source_range.start, length))
}

fn resource_id_snapshot(value: MTLResourceID) -> ResourceID {
    ResourceID {
        _impl: value.to_raw(),
    }
}

impl ResourceViewPool {
    /// Returns an owned snapshot of the pool's base resource identifier.
    pub fn base_resource_id(&self) -> Result<ResourceID, Error> {
        require_selector(
            self.as_inner(),
            sel!(baseResourceID),
            "resource-view base identifier is unavailable",
        )?;
        // SAFETY: the selector is present and returns MTLResourceID by value.
        let value: MTLResourceID = unsafe { msg_send![self.as_inner(), baseResourceID] };
        Ok(resource_id_snapshot(value))
    }

    /// Copies a checked source range and returns the destination ID snapshot.
    pub fn copy_resource_views_from_pool(
        &self,
        source: &ResourceViewPool,
        source_range: Range<usize>,
        destination_index: usize,
    ) -> Result<ResourceID, Error> {
        let source_device = source
            .device()?
            .ok_or_else(|| Error::unsupported("source resource-view pool device is unavailable"))?;
        let destination_device = self.device()?.ok_or_else(|| {
            Error::unsupported("destination resource-view pool device is unavailable")
        })?;
        if !std::ptr::eq(
            source_device.as_any_object(),
            destination_device.as_any_object(),
        ) {
            return Err(Error::invalid_argument(
                "resource-view pools must belong to the same Metal device",
            ));
        }
        let range = checked_copy_ranges(
            source.resource_view_count()?,
            self.resource_view_count()?,
            source_range,
            destination_index,
        )?;
        require_selector(
            self.as_inner(),
            sel!(copyResourceViewsFromPool:sourceRange:destinationIndex:),
            "resource-view pool copying is unavailable",
        )?;
        // SAFETY: the selector is present, both pool wrappers keep their
        // objects alive, and both source and destination ranges were checked.
        let value: MTLResourceID = unsafe {
            msg_send![
                self.as_inner(),
                copyResourceViewsFromPool: source.as_inner(),
                sourceRange: range,
                destinationIndex: destination_index
            ]
        };
        Ok(resource_id_snapshot(value))
    }
}

#[cfg(test)]
mod tests {
    use super::checked_copy_ranges;

    #[test]
    fn resource_view_copy_ranges_are_checked() {
        let range = checked_copy_ranges(8, 8, 2..6, 1).expect("valid range");
        assert_eq!(range.location, 2);
        assert_eq!(range.length, 4);
        assert!(checked_copy_ranges(8, 8, 2..2, 0).is_err());
        assert!(checked_copy_ranges(8, 8, 7..9, 0).is_err());
        assert!(checked_copy_ranges(8, 8, 0..4, 6).is_err());
        assert!(checked_copy_ranges(8, usize::MAX, 0..2, usize::MAX).is_err());
    }
}
