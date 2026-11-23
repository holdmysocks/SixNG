//! Safe facade for Metal event notifications and drawable presentation.

use super::{Drawable, SharedEvent, SharedEventHandle, SharedEventListener};
use crate::Error;
use crate::quartz_core::Drawable as LayerDrawable;
use std::time::Duration;

/// RAII registration for a group of shared-event value notifications.
/// Dropping it prevents all later delivery to the Rust callback.
pub struct SharedEventNotificationRegistration {
    _inner: metal_rust_ffi::SharedEventNotificationRegistration,
}

impl Drawable {
    /// Presents this drawable immediately.
    pub fn present(&self) -> Result<(), Error> {
        self.inner.present().map_err(Error::from_ffi)
    }

    /// Presents no earlier than a finite, non-negative duration in seconds.
    pub fn present_after_minimum_duration(&self, duration: f64) -> Result<(), Error> {
        self.inner
            .present_after_minimum_duration(duration)
            .map_err(Error::from_ffi)
    }

    /// Presents at a finite, non-negative host time in seconds.
    pub fn present_at_time(&self, presentation_time: f64) -> Result<(), Error> {
        self.inner
            .present_at_time(presentation_time)
            .map_err(Error::from_ffi)
    }

    /// Returns a finite, non-negative presentation host time.
    pub fn validated_presented_time(&self) -> Result<f64, Error> {
        self.inner
            .validated_presented_time()
            .map_err(Error::from_ffi)
    }

    /// Installs a one-shot callback invoked after on-screen presentation.
    pub fn on_presented(
        &self,
        handler: impl FnOnce(Drawable) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .on_presented(move |inner| handler(Drawable::from_ffi(inner)))
            .map_err(Error::from_ffi)
    }
}

impl LayerDrawable {
    /// Returns this layer drawable's monotonically increasing identifier.
    pub fn drawable_id(&self) -> Result<usize, Error> {
        self.inner.drawable_id().map_err(Error::from_ffi)
    }

    /// Presents this layer drawable immediately.
    pub fn present(&self) -> Result<(), Error> {
        self.inner.present().map_err(Error::from_ffi)
    }

    /// Presents no earlier than a finite, non-negative duration in seconds.
    pub fn present_after_minimum_duration(&self, duration: f64) -> Result<(), Error> {
        self.inner
            .present_after_minimum_duration(duration)
            .map_err(Error::from_ffi)
    }

    /// Presents at a finite, non-negative host time in seconds.
    pub fn present_at_time(&self, presentation_time: f64) -> Result<(), Error> {
        self.inner
            .present_at_time(presentation_time)
            .map_err(Error::from_ffi)
    }

    /// Returns a finite, non-negative presentation host time.
    pub fn presented_time(&self) -> Result<f64, Error> {
        self.inner.presented_time().map_err(Error::from_ffi)
    }

    /// Installs a one-shot callback invoked after on-screen presentation.
    pub fn on_presented(
        &self,
        handler: impl FnOnce(LayerDrawable) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .on_presented(move |inner| handler(LayerDrawable { inner }))
            .map_err(Error::from_ffi)
    }
}

impl SharedEventListener {
    /// Returns Metal's process-wide listener without exposing a dispatch queue.
    pub fn shared() -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::SharedEventListener::shared()
            .map(Self::from_ffi)
            .map_err(Error::from_ffi)
    }
}

impl SharedEvent {
    /// Creates an owned handle suitable for sharing this event.
    pub fn new_shared_event_handle(&self) -> Result<SharedEventHandle, Error> {
        self.inner
            .new_shared_event_handle()
            .map(SharedEventHandle::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Waits for the requested value for at most the checked duration.
    pub fn wait_until_signaled_value(&self, value: u64, timeout: Duration) -> Result<bool, Error> {
        self.inner
            .wait_until_signaled_value(value, timeout)
            .map_err(Error::from_ffi)
    }

    /// Installs one one-shot value notification.
    pub fn notify_at(
        &self,
        listener: &SharedEventListener,
        value: u64,
        handler: impl FnOnce(u64) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .notify_at(&listener.inner, value, handler)
            .map_err(Error::from_ffi)
    }

    /// Registers repeatable callbacks at strictly increasing event values.
    pub fn notify_values(
        &self,
        listener: &SharedEventListener,
        values: &[u64],
        handler: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<SharedEventNotificationRegistration, Error> {
        self.inner
            .notify_values(&listener.inner, values, handler)
            .map(|_inner| SharedEventNotificationRegistration { _inner })
            .map_err(Error::from_ffi)
    }
}
