use std::ops::Range;

use crate::ThreadBound;
use crate::foundation::Error;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{MTL4CounterHeap, MTL4CounterHeapType};

/// Maximum timestamp entries accepted by the safe Metal 4 wrapper.
pub const MAX_TIMESTAMP_COUNTERS: usize = 4096;

/// An owned Metal 4 timestamp counter heap.
pub struct TimestampCounterHeap {
    inner: Retained<ProtocolObject<dyn MTL4CounterHeap>>,
    count: usize,
    _thread_bound: ThreadBound,
}

/// An opaque counter readback bound to one Metal 4 submission.
pub struct CounterReadback {
    pub(super) heap: TimestampCounterHeap,
    pub(super) buffer: crate::Buffer,
    pub(super) submission_id: u64,
    pub(super) count: usize,
}

impl TimestampCounterHeap {
    pub(super) const fn new(
        inner: Retained<ProtocolObject<dyn MTL4CounterHeap>>,
        count: usize,
    ) -> Self {
        Self {
            inner,
            count,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the number of timestamp entries in the heap.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns the optional diagnostic label after checking runtime support.
    pub fn label(&self) -> Result<Option<String>, Error> {
        let object = self.as_any_object();
        require_selector(object, sel!(label), "MTL4::CounterHeap::label")?;
        // SAFETY: Selector availability and the nullable retained NSString
        // return ABI were checked before dispatch.
        let value: Option<Retained<NSString>> = unsafe { msg_send![object, label] };
        Ok(value.map(|value| value.to_string()))
    }

    /// Sets or clears the diagnostic label after checking runtime support.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), Error> {
        let object = self.as_any_object();
        require_selector(object, sel!(setLabel:), "MTL4::CounterHeap::setLabel")?;
        let label = label.map(NSString::from_str);
        // SAFETY: Selector availability was checked and the optional NSString
        // remains alive for the duration of the synchronous message.
        unsafe { msg_send![object, setLabel: label.as_deref()] }
        Ok(())
    }

    /// Returns the checked counter-heap type reported by Metal.
    pub fn heap_type(&self) -> Result<crate::metal::generated_value_types::CounterHeapType, Error> {
        let object = self.as_any_object();
        require_selector(object, sel!(type), "MTL4::CounterHeap::type")?;
        // SAFETY: Selector availability was checked and MTL4CounterHeapType is
        // an NSInteger-backed enumeration.
        let value: MTL4CounterHeapType = unsafe { msg_send![object, type] };
        Ok(crate::metal::generated_value_types::CounterHeapType::from_system_raw(value.0))
    }

    /// Invalidates a checked range while the heap remains CPU-owned.
    ///
    /// The canonical wrapper does not expose the underlying heap object. GPU
    /// APIs that use the heap must consume this CPU-owned state and return it
    /// only after completion, preserving the synchronization precondition of
    /// the native method inside Metal-Rust.
    pub fn invalidate_range(&mut self, range: Range<usize>) -> Result<(), Error> {
        let range = checked_range(range, self.count, "invalidateCounterRange")?;
        let object = self.as_any_object();
        require_selector(
            object,
            sel!(invalidateCounterRange:),
            "MTL4::CounterHeap::invalidateCounterRange",
        )?;
        // SAFETY: The range was checked against the immutable heap count. The
        // facade owns this non-cloneable heap state and exposes no GPU-use
        // escape while this CPU method is callable.
        unsafe { msg_send![object, invalidateCounterRange: range] }
        Ok(())
    }

    pub(super) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: The protocol object is backed by the same Objective-C object
        // pointer; this only erases its protocol type for checked dispatch.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }
}

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<(), Error> {
    // SAFETY: Metal protocol objects implement NSObject's respondsToSelector:
    // and BOOL has the Rust bool ABI used throughout objc2.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if available {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{name} is unavailable")))
    }
}

pub(super) fn checked_range(
    range: Range<usize>,
    count: usize,
    name: &str,
) -> Result<NSRange, Error> {
    let length = range
        .end
        .checked_sub(range.start)
        .ok_or_else(|| Error::invalid_argument(format!("{name} has an inverted range")))?;
    if range.end > count {
        return Err(Error::invalid_argument(format!(
            "{name} range {:?} exceeds counter heap count {count}",
            range
        )));
    }
    Ok(NSRange {
        location: range.start,
        length,
    })
}

#[cfg(test)]
mod tests {
    use super::checked_range;
    use std::ops::Range;

    #[test]
    fn checked_range_accepts_empty_and_full_ranges() {
        let empty = checked_range(3..3, 4, "test").unwrap();
        assert_eq!((empty.location, empty.length), (3, 0));
        let full = checked_range(0..4, 4, "test").unwrap();
        assert_eq!((full.location, full.length), (0, 4));
    }

    #[test]
    fn checked_range_rejects_inversion_and_overrun() {
        assert!(checked_range(Range { start: 3, end: 2 }, 4, "test").is_err());
        assert!(checked_range(2..5, 4, "test").is_err());
    }
}
