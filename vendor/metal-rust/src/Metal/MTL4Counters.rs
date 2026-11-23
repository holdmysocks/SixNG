use std::ops::Range;

/// Maximum timestamp entries accepted by the safe Metal 4 API.
pub const MAX_TIMESTAMP_COUNTERS: usize = metal_rust_ffi::MAX_TIMESTAMP_COUNTERS;

/// An owned Metal 4 timestamp counter heap.
pub struct TimestampCounterHeap {
    pub(crate) inner: metal_rust_ffi::TimestampCounterHeap,
}

/// An opaque counter readback tied to one Metal 4 submission.
pub struct CounterReadback {
    pub(crate) inner: metal_rust_ffi::CounterReadback,
}

impl TimestampCounterHeap {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::TimestampCounterHeap) -> Self {
        Self { inner }
    }

    /// Returns the number of timestamp entries in the heap.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.inner.count()
    }

    /// Returns the optional diagnostic label.
    pub fn label(&self) -> Result<Option<String>, crate::Error> {
        self.inner.label().map_err(crate::Error::from_ffi)
    }

    /// Sets or clears the diagnostic label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), crate::Error> {
        self.inner.set_label(label).map_err(crate::Error::from_ffi)
    }

    /// Returns the checked heap type reported by Metal.
    pub fn heap_type(&self) -> Result<crate::metal4::CounterHeapType, crate::Error> {
        self.inner.heap_type().map_err(crate::Error::from_ffi)
    }

    /// Invalidates a checked counter range while this heap is CPU-owned.
    pub fn invalidate_range(&mut self, range: Range<usize>) -> Result<(), crate::Error> {
        self.inner
            .invalidate_range(range)
            .map_err(crate::Error::from_ffi)
    }
}
