//! Audited callbacks and timing operations for Metal events and drawables.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    Drawable, SharedEvent, SharedEventHandle, SharedEventListener,
};
use crate::quartz_core::Drawable as LayerDrawable;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject, Sel};
use objc2::{msg_send, sel};
use objc2_quartz_core::CAMetalDrawable;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;

fn responds_to(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: every Objective-C object implements `respondsToSelector:` and
    // both arguments use their runtime-declared ABI representations.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

fn checked_seconds(value: f64, description: &str) -> Result<f64, Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::invalid_argument(format!(
            "{description} must be finite and non-negative"
        )));
    }
    Ok(value)
}

fn duration_milliseconds(value: Duration) -> Result<u64, Error> {
    let whole = value.as_secs().checked_mul(1_000).ok_or_else(|| {
        Error::invalid_argument("shared-event timeout cannot be represented in milliseconds")
    })?;
    let fractional = u64::from(value.subsec_nanos().div_ceil(1_000_000));
    whole.checked_add(fractional).ok_or_else(|| {
        Error::invalid_argument("shared-event timeout cannot be represented in milliseconds")
    })
}

fn validate_notification_values(values: &[u64]) -> Result<(), Error> {
    if values.is_empty() {
        return Err(Error::invalid_argument(
            "shared-event notification values cannot be empty",
        ));
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Error::invalid_argument(
            "shared-event notification values must be strictly increasing",
        ));
    }
    Ok(())
}

struct DeliveryStatus {
    active: bool,
    owner: Option<ThreadId>,
    depth: usize,
}

struct DeliveryState {
    status: Mutex<DeliveryStatus>,
    drained: Condvar,
}

impl DeliveryState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(DeliveryStatus {
                active: true,
                owner: None,
                depth: 0,
            }),
            drained: Condvar::new(),
        })
    }

    fn try_enter(self: &Arc<Self>) -> Option<DeliveryGuard> {
        let current = thread::current().id();
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if !status.active {
                return None;
            }
            match status.owner {
                None => {
                    status.owner = Some(current);
                    status.depth = 1;
                    return Some(DeliveryGuard {
                        state: Arc::clone(self),
                        _not_send: PhantomData,
                    });
                }
                Some(owner) if owner == current => {
                    status.depth = status.depth.checked_add(1)?;
                    return Some(DeliveryGuard {
                        state: Arc::clone(self),
                        _not_send: PhantomData,
                    });
                }
                Some(_) => {
                    status = self
                        .drained
                        .wait(status)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            }
        }
    }

    fn deactivate_and_wait(self: &Arc<Self>) {
        let current = thread::current().id();
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.active = false;
        self.drained.notify_all();
        // Handlers are serialized across threads. If Drop runs from this
        // registration's handler, only reentrant frames on this same thread
        // can be active, so returning cannot release another pending handler.
        if status.owner == Some(current) {
            return;
        }
        while status.depth != 0 {
            status = self
                .drained
                .wait(status)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

struct DeliveryGuard {
    state: Arc<DeliveryState>,
    // The guard owns one entry in a thread-local stack and must be dropped on
    // the same callback thread that created it.
    _not_send: PhantomData<Rc<()>>,
}

impl Drop for DeliveryGuard {
    fn drop(&mut self) {
        let mut status = self
            .state
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.depth = status.depth.saturating_sub(1);
        if status.depth == 0 {
            status.owner = None;
            self.state.drained.notify_all();
        }
    }
}

/// RAII state for a group of repeatable shared-event notifications.
///
/// Metal does not expose cancellation for notifications that have already
/// been submitted to a listener. Drop disables new Rust handler entries and
/// waits for handlers on other threads to finish. Dropping from inside the
/// callback, including a reentrant callback, is supported without deadlock.
pub struct SharedEventNotificationRegistration {
    state: Arc<DeliveryState>,
}

impl Drop for SharedEventNotificationRegistration {
    fn drop(&mut self) {
        self.state.deactivate_and_wait();
    }
}

impl Drawable {
    /// Presents this drawable immediately when the runtime supports it.
    pub fn present(&self) -> Result<(), Error> {
        if !responds_to(self.as_inner(), sel!(present)) {
            return Err(Error::unsupported("MTLDrawable::present is unavailable"));
        }
        // SAFETY: selector availability was checked and it takes no arguments.
        unsafe {
            let _: () = msg_send![self.as_inner(), present];
        }
        Ok(())
    }

    /// Presents no earlier than the supplied finite, non-negative duration.
    pub fn present_after_minimum_duration(&self, duration: f64) -> Result<(), Error> {
        let duration = checked_seconds(duration, "minimum presentation duration")?;
        if !responds_to(self.as_inner(), sel!(presentAfterMinimumDuration:)) {
            return Err(Error::unsupported(
                "MTLDrawable::presentAfterMinimumDuration is unavailable",
            ));
        }
        // SAFETY: selector availability and the finite CFTimeInterval input
        // were checked before sending the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), presentAfterMinimumDuration: duration];
        }
        Ok(())
    }

    /// Presents at the supplied finite, non-negative host time.
    pub fn present_at_time(&self, presentation_time: f64) -> Result<(), Error> {
        let presentation_time = checked_seconds(presentation_time, "presentation time")?;
        if !responds_to(self.as_inner(), sel!(presentAtTime:)) {
            return Err(Error::unsupported(
                "MTLDrawable::presentAtTime is unavailable",
            ));
        }
        // SAFETY: selector availability and the finite CFTimeInterval input
        // were checked before sending the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), presentAtTime: presentation_time];
        }
        Ok(())
    }

    /// Returns a finite, non-negative presentation time.
    pub fn validated_presented_time(&self) -> Result<f64, Error> {
        let value = self.presented_time()?;
        checked_seconds(value, "reported presentation time")
    }

    /// Installs a one-shot callback for on-screen presentation.
    ///
    /// A panic in the Rust callback is contained at the Objective-C block
    /// boundary and Metal cannot invoke the `FnOnce` more than once.
    pub fn on_presented(
        &self,
        handler: impl FnOnce(Drawable) + Send + 'static,
    ) -> Result<(), Error> {
        if !responds_to(self.as_inner(), sel!(addPresentedHandler:)) {
            return Err(Error::unsupported(
                "MTLDrawable::addPresentedHandler is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |drawable: NonNull<AnyObject>| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            // SAFETY: Metal supplies a live, non-null MTLDrawable for the
            // callback duration. Retaining establishes independent ownership.
            let drawable = unsafe { Retained::retain(drawable.as_ptr()) };
            let Some(drawable) = drawable else {
                return;
            };
            let drawable = Drawable::from_inner(drawable);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(drawable)));
        });
        // SAFETY: selector availability was checked and `block` has the exact
        // `void (^)(id<MTLDrawable>)` ABI. Metal copies it for later delivery.
        unsafe {
            let _: () = msg_send![self.as_inner(), addPresentedHandler: &*block];
        }
        Ok(())
    }
}

impl LayerDrawable {
    fn metal_object(&self) -> &AnyObject {
        let drawable: &ProtocolObject<dyn CAMetalDrawable> = &self.inner;
        drawable.as_ref()
    }

    /// Returns this layer drawable's monotonically increasing identifier.
    pub fn drawable_id(&self) -> Result<usize, Error> {
        if !responds_to(self.metal_object(), sel!(drawableID)) {
            return Err(Error::unsupported("MTLDrawable::drawableID is unavailable"));
        }
        // SAFETY: selector availability was checked and NSUInteger maps to usize.
        Ok(unsafe { msg_send![self.metal_object(), drawableID] })
    }

    /// Presents this layer drawable immediately.
    pub fn present(&self) -> Result<(), Error> {
        if !responds_to(self.metal_object(), sel!(present)) {
            return Err(Error::unsupported("MTLDrawable::present is unavailable"));
        }
        // SAFETY: selector availability was checked and it takes no arguments.
        unsafe {
            let _: () = msg_send![self.metal_object(), present];
        }
        Ok(())
    }

    /// Presents no earlier than the supplied finite, non-negative duration.
    pub fn present_after_minimum_duration(&self, duration: f64) -> Result<(), Error> {
        let duration = checked_seconds(duration, "minimum presentation duration")?;
        if !responds_to(self.metal_object(), sel!(presentAfterMinimumDuration:)) {
            return Err(Error::unsupported(
                "MTLDrawable::presentAfterMinimumDuration is unavailable",
            ));
        }
        // SAFETY: selector availability and the finite CFTimeInterval input
        // were checked before sending the message.
        unsafe {
            let _: () = msg_send![self.metal_object(), presentAfterMinimumDuration: duration];
        }
        Ok(())
    }

    /// Presents at the supplied finite, non-negative host time.
    pub fn present_at_time(&self, presentation_time: f64) -> Result<(), Error> {
        let presentation_time = checked_seconds(presentation_time, "presentation time")?;
        if !responds_to(self.metal_object(), sel!(presentAtTime:)) {
            return Err(Error::unsupported(
                "MTLDrawable::presentAtTime is unavailable",
            ));
        }
        // SAFETY: selector availability and the finite CFTimeInterval input
        // were checked before sending the message.
        unsafe {
            let _: () = msg_send![self.metal_object(), presentAtTime: presentation_time];
        }
        Ok(())
    }

    /// Returns a finite, non-negative presentation host time.
    pub fn presented_time(&self) -> Result<f64, Error> {
        if !responds_to(self.metal_object(), sel!(presentedTime)) {
            return Err(Error::unsupported(
                "MTLDrawable::presentedTime is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and CFTimeInterval is f64.
        let value: f64 = unsafe { msg_send![self.metal_object(), presentedTime] };
        checked_seconds(value, "reported presentation time")
    }

    /// Installs a one-shot callback for on-screen presentation.
    pub fn on_presented(
        &self,
        handler: impl FnOnce(LayerDrawable) + Send + 'static,
    ) -> Result<(), Error> {
        if !responds_to(self.metal_object(), sel!(addPresentedHandler:)) {
            return Err(Error::unsupported(
                "MTLDrawable::addPresentedHandler is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |drawable: NonNull<AnyObject>| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            // SAFETY: this handler was installed on a CAMetalDrawable, and
            // Metal passes that presented drawable back to its handler.
            let drawable = unsafe { Retained::retain(drawable.as_ptr()) };
            let Some(drawable) = drawable else {
                return;
            };
            // SAFETY: the callback object is the CAMetalDrawable receiver and
            // therefore conforms to CAMetalDrawable for the retained lifetime.
            let inner = unsafe {
                Retained::cast_unchecked::<ProtocolObject<dyn CAMetalDrawable>>(drawable)
            };
            let _ = catch_unwind(AssertUnwindSafe(|| callback(LayerDrawable::new(inner))));
        });
        // SAFETY: selector availability was checked and `block` has the exact
        // MTLDrawable presentation-handler ABI. Metal copies it for delivery.
        unsafe {
            let _: () = msg_send![self.metal_object(), addPresentedHandler: &*block];
        }
        Ok(())
    }
}

impl SharedEventListener {
    /// Returns Metal's process-wide listener without exposing its dispatch queue.
    pub fn shared() -> Result<Self, Error> {
        let class = AnyClass::get(c"MTLSharedEventListener").ok_or_else(|| {
            Error::unsupported("MTLSharedEventListener is unavailable on this system")
        })?;
        if !responds_to(class.as_ref(), sel!(sharedListener)) {
            return Err(Error::unsupported(
                "MTLSharedEventListener::sharedListener is unavailable",
            ));
        }
        // SAFETY: the class and selector were checked; the retained result is
        // stored by the owned wrapper and no dispatch queue crosses the API.
        let inner: Option<Retained<AnyObject>> = unsafe { msg_send![class, sharedListener] };
        inner
            .map(Self::from_inner)
            .ok_or_else(|| Error::unsupported("Metal returned no shared event listener"))
    }
}

impl SharedEvent {
    /// Creates an owned cross-process handle for this shared event.
    pub fn new_shared_event_handle(&self) -> Result<SharedEventHandle, Error> {
        if !responds_to(self.as_inner(), sel!(newSharedEventHandle)) {
            return Err(Error::unsupported(
                "MTLSharedEvent::newSharedEventHandle is unavailable",
            ));
        }
        // SAFETY: selector availability was checked; the `new` method family
        // returns an owned object which is transferred into the safe wrapper.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), newSharedEventHandle] };
        inner
            .map(SharedEventHandle::from_inner)
            .ok_or_else(|| Error::unsupported("Metal returned no shared event handle"))
    }

    /// Waits for a value using a checked Rust duration.
    pub fn wait_until_signaled_value(&self, value: u64, timeout: Duration) -> Result<bool, Error> {
        let milliseconds = duration_milliseconds(timeout)?;
        if !responds_to(self.as_inner(), sel!(waitUntilSignaledValue:timeoutMS:)) {
            return Err(Error::unsupported(
                "MTLSharedEvent::waitUntilSignaledValue is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and both values exactly
        // match the two uint64_t parameters declared by Metal.
        Ok(unsafe {
            msg_send![self.as_inner(), waitUntilSignaledValue: value, timeoutMS: milliseconds]
        })
    }

    /// Registers a one-shot callback for one signaled-value threshold.
    pub fn notify_at(
        &self,
        listener: &SharedEventListener,
        value: u64,
        handler: impl FnOnce(u64) + Send + 'static,
    ) -> Result<(), Error> {
        if !responds_to(self.as_inner(), sel!(notifyListener:atValue:block:)) {
            return Err(Error::unsupported(
                "MTLSharedEvent::notifyListener is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |_event: NonNull<AnyObject>, delivered_value: u64| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(callback) = callback {
                let _ = catch_unwind(AssertUnwindSafe(|| callback(delivered_value)));
            }
        });
        // SAFETY: selector availability was checked; listener is retained by
        // its wrapper and the block has Metal's `(MTLSharedEvent*, uint64_t)`
        // ABI. Metal copies the block for asynchronous delivery.
        unsafe {
            let _: () = msg_send![
                self.as_inner(),
                notifyListener: listener.as_inner(),
                atValue: value,
                block: &*block
            ];
        }
        Ok(())
    }

    /// Registers repeatable delivery for a strictly increasing set of values.
    ///
    /// Dropping the returned registration prevents every later Rust callback.
    pub fn notify_values(
        &self,
        listener: &SharedEventListener,
        values: &[u64],
        handler: impl Fn(u64) + Send + Sync + 'static,
    ) -> Result<SharedEventNotificationRegistration, Error> {
        validate_notification_values(values)?;
        if !responds_to(self.as_inner(), sel!(notifyListener:atValue:block:)) {
            return Err(Error::unsupported(
                "MTLSharedEvent::notifyListener is unavailable",
            ));
        }

        let state = DeliveryState::new();
        let handler: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(handler);
        for &value in values {
            let callback_state = Arc::clone(&state);
            let callback_handler = Arc::clone(&handler);
            let block = RcBlock::new(move |_event: NonNull<AnyObject>, delivered_value: u64| {
                let Some(_delivery) = callback_state.try_enter() else {
                    return;
                };
                let _ = catch_unwind(AssertUnwindSafe(|| callback_handler(delivered_value)));
            });
            // SAFETY: selector availability was checked; every block has the
            // exact notification ABI and is copied by Metal before this loop
            // releases its local owner.
            unsafe {
                let _: () = msg_send![
                    self.as_inner(),
                    notifyListener: listener.as_inner(),
                    atValue: value,
                    block: &*block
                ];
            }
        }
        Ok(SharedEventNotificationRegistration { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_rounds_sub_millisecond_up() {
        assert_eq!(duration_milliseconds(Duration::from_nanos(1)).unwrap(), 1);
        assert_eq!(duration_milliseconds(Duration::from_millis(3)).unwrap(), 3);
        assert!(duration_milliseconds(Duration::MAX).is_err());
    }

    #[test]
    fn timing_validation_rejects_invalid_values() {
        assert!(checked_seconds(f64::NAN, "time").is_err());
        assert!(checked_seconds(f64::INFINITY, "time").is_err());
        assert!(checked_seconds(-0.1, "time").is_err());
        assert_eq!(checked_seconds(0.0, "time").unwrap(), 0.0);
    }

    #[test]
    fn registration_drop_disables_delivery() {
        let state = DeliveryState::new();
        let registration = SharedEventNotificationRegistration {
            state: Arc::clone(&state),
        };
        drop(registration);
        assert!(state.try_enter().is_none());
    }

    #[test]
    fn registration_drop_waits_for_in_flight_delivery() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::mpsc;
        use std::thread;

        let state = DeliveryState::new();
        let registration = SharedEventNotificationRegistration {
            state: Arc::clone(&state),
        };
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let callback_state = Arc::clone(&state);
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let callback = thread::spawn(move || {
            let _delivery = callback_state.try_enter().unwrap();
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            callback_calls.fetch_add(1, Ordering::Release);
        });
        entered_rx.recv().unwrap();

        let (dropped_tx, dropped_rx) = mpsc::sync_channel(0);
        let dropper = thread::spawn(move || {
            drop(registration);
            dropped_tx.send(()).unwrap();
        });
        assert!(dropped_rx.recv_timeout(Duration::from_millis(50)).is_err());
        release_tx.send(()).unwrap();
        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        callback.join().unwrap();
        dropper.join().unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert!(state.try_enter().is_none());
    }

    #[test]
    fn callback_can_drop_registration_during_reentrant_delivery() {
        use std::sync::mpsc;
        use std::thread;

        let state = DeliveryState::new();
        let registration = SharedEventNotificationRegistration {
            state: Arc::clone(&state),
        };
        let (finished_tx, finished_rx) = mpsc::sync_channel(0);
        thread::spawn(move || {
            let outer = state.try_enter().unwrap();
            let inner = state.try_enter().unwrap();
            drop(registration);
            drop(inner);
            drop(outer);
            finished_tx.send(()).unwrap();
        });
        finished_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn callback_drop_cancels_handler_waiting_on_another_thread() {
        use std::sync::mpsc;
        use std::thread;

        let state = DeliveryState::new();
        let registration = SharedEventNotificationRegistration {
            state: Arc::clone(&state),
        };
        let (owner_ready_tx, owner_ready_rx) = mpsc::sync_channel(0);
        let (drop_tx, drop_rx) = mpsc::sync_channel(0);
        let owner_state = Arc::clone(&state);
        let owner = thread::spawn(move || {
            let delivery = owner_state.try_enter().unwrap();
            owner_ready_tx.send(()).unwrap();
            drop_rx.recv().unwrap();
            drop(registration);
            drop(delivery);
        });
        owner_ready_rx.recv().unwrap();

        let (waiter_started_tx, waiter_started_rx) = mpsc::sync_channel(0);
        let (waiter_result_tx, waiter_result_rx) = mpsc::sync_channel(0);
        let waiter_state = Arc::clone(&state);
        let waiter = thread::spawn(move || {
            waiter_started_tx.send(()).unwrap();
            waiter_result_tx
                .send(waiter_state.try_enter().is_some())
                .unwrap();
        });
        waiter_started_rx.recv().unwrap();
        drop_tx.send(()).unwrap();

        assert!(
            !waiter_result_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
        );
        owner.join().unwrap();
        waiter.join().unwrap();
    }

    #[test]
    fn repeated_values_must_be_nonempty_and_increasing() {
        assert!(validate_notification_values(&[]).is_err());
        assert!(validate_notification_values(&[2, 2]).is_err());
        assert!(validate_notification_values(&[3, 2]).is_err());
        assert!(validate_notification_values(&[2, 3]).is_ok());
    }
}
