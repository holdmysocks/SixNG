//! Audited Foundation notification callback boundary.

use crate::ThreadBound;
use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::{Arc, Condvar, Mutex};

thread_local! {
    /// Callback states currently executing on this thread. This makes
    /// unregistering from inside the callback non-blocking without weakening
    /// the wait performed by an unregistering thread outside the callback.
    static EXECUTING_CALLBACKS: std::cell::RefCell<Vec<usize>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[derive(Debug)]
struct DeliveryStatus {
    active: bool,
    in_flight: usize,
}

#[derive(Debug)]
struct DeliveryState {
    status: Mutex<DeliveryStatus>,
    drained: Condvar,
}

impl DeliveryState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Mutex::new(DeliveryStatus {
                active: true,
                in_flight: 0,
            }),
            drained: Condvar::new(),
        })
    }

    fn enter(state: &Arc<Self>) -> Option<DeliveryGuard> {
        let mut status = state
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !status.active {
            return None;
        }
        status.in_flight = status
            .in_flight
            .checked_add(1)
            .expect("notification callback count overflowed");
        drop(status);

        let identity = Arc::as_ptr(state) as usize;
        EXECUTING_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(identity));
        Some(DeliveryGuard {
            state: Arc::clone(state),
            identity,
        })
    }

    fn deactivate(&self) {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active = false;
    }

    fn wait_until_drained(&self) {
        let identity = self as *const Self as usize;
        let callbacks_on_this_thread = EXECUTING_CALLBACKS.with(|callbacks| {
            callbacks
                .borrow()
                .iter()
                .filter(|callback| **callback == identity)
                .count()
        });

        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A callback unregistering itself cannot wait for its own guard (or
        // reentrant parent guards on the same thread), but it still waits for
        // callbacks executing concurrently on other threads.
        while status.in_flight > callbacks_on_this_thread {
            status = self
                .drained
                .wait(status)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

struct DeliveryGuard {
    state: Arc<DeliveryState>,
    identity: usize,
}

impl Drop for DeliveryGuard {
    fn drop(&mut self) {
        EXECUTING_CALLBACKS.with(|callbacks| {
            let mut callbacks = callbacks.borrow_mut();
            let position = callbacks
                .iter()
                .rposition(|identity| *identity == self.identity)
                .expect("notification callback guard was not registered");
            callbacks.remove(position);
        });

        let mut status = self
            .state
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.in_flight = status
            .in_flight
            .checked_sub(1)
            .expect("notification callback count underflowed");
        self.state.drained.notify_all();
    }
}

fn invoke_handler(handler: &(dyn Fn(Notification) + Send + Sync), notification: Notification) {
    let _ = catch_unwind(AssertUnwindSafe(|| handler(notification)));
}

/// Owned notification values delivered to safe Rust callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    name: String,
    object_description: Option<String>,
    user_info_description: Option<String>,
}

impl Notification {
    /// Returns the copied notification name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns an owned description of the untyped notification object.
    #[must_use]
    pub fn object_description(&self) -> Option<&str> {
        self.object_description.as_deref()
    }

    /// Returns an owned description of the untyped user-info dictionary.
    #[must_use]
    pub fn user_info_description(&self) -> Option<&str> {
        self.user_info_description.as_deref()
    }
}

/// RAII observer registration. Dropping it prevents further Rust callback
/// delivery and unregisters the exact Objective-C observer token.
pub struct Registration {
    center: Retained<NSNotificationCenter>,
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    delivery: Arc<DeliveryState>,
    _thread_bound: ThreadBound,
}

impl Drop for Registration {
    fn drop(&mut self) {
        // Deactivation is the linearization point: callbacks must acquire the
        // same mutex before entering the Rust handler.
        self.delivery.deactivate();
        let observer: &AnyObject = self.observer.as_ref();
        // SAFETY: `observer` is the exact retained token returned by this
        // notification center, and it remains alive for the call.
        unsafe { self.center.removeObserver(observer) };
        // Outside the callback, Drop does not return until every handler which
        // entered before deactivation has completed. Inside the callback this
        // intentionally does not wait for itself; no later handler can enter.
        self.delivery.wait_until_drained();
    }
}

/// The process default Foundation notification center.
pub struct NotificationCenter {
    inner: Retained<NSNotificationCenter>,
    _thread_bound: ThreadBound,
}

impl NotificationCenter {
    /// Returns the process default notification center.
    #[must_use]
    pub fn default_center() -> Self {
        Self {
            inner: NSNotificationCenter::defaultCenter(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Registers a repeatable, thread-safe callback for a notification name.
    pub fn add_observer(
        &self,
        name: &str,
        handler: impl Fn(Notification) + Send + Sync + 'static,
    ) -> Registration {
        let delivery = DeliveryState::new();
        let callback_delivery = Arc::clone(&delivery);
        let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
            let Some(_delivery_guard) = DeliveryState::enter(&callback_delivery) else {
                return;
            };
            // SAFETY: Foundation invokes this block with a non-null
            // NSNotification for the duration of the callback. Only owned
            // strings are copied out before returning.
            let notification = unsafe { notification.as_ref() };
            let value = Notification {
                name: notification.name().to_string(),
                object_description: notification.object().map(|value| {
                    // SAFETY: notification objects conform to NSObject's
                    // description selector; the retained NSString is copied.
                    let description: Retained<NSString> =
                        unsafe { msg_send![&*value, description] };
                    description.to_string()
                }),
                user_info_description: notification
                    .userInfo()
                    .map(|value| value.description().to_string()),
            };
            invoke_handler(&handler, value);
        });
        let name = NSString::from_str(name);
        // SAFETY: no object filter or operation queue is supplied; the block
        // is sendable, retained by Foundation, and copies all callback data.
        let observer = unsafe {
            self.inner
                .addObserverForName_object_queue_usingBlock(Some(&name), None, None, &block)
        };
        Registration {
            center: self.inner.clone(),
            observer,
            delivery,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Posts a name-only notification synchronously.
    pub fn post(&self, name: &str) {
        let name = NSString::from_str(name);
        // SAFETY: both untyped payload positions are intentionally None, so
        // no object or dictionary generic invariant crosses the boundary.
        unsafe {
            self.inner
                .postNotificationName_object_userInfo(&name, None, None)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn deactivation_rejects_later_callbacks() {
        let state = DeliveryState::new();
        state.deactivate();
        assert!(DeliveryState::enter(&state).is_none());
    }

    #[test]
    fn unregister_waits_for_an_in_flight_callback() {
        let state = DeliveryState::new();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let callback_state = Arc::clone(&state);
        let callback = thread::spawn(move || {
            let guard = DeliveryState::enter(&callback_state).expect("callback should enter");
            entered_tx
                .send(())
                .expect("test receiver should remain live");
            release_rx.recv().expect("test sender should remain live");
            drop(guard);
        });
        entered_rx.recv().expect("callback should report entry");

        state.deactivate();
        let (drained_tx, drained_rx) = mpsc::channel();
        let waiting_state = Arc::clone(&state);
        let waiter = thread::spawn(move || {
            waiting_state.wait_until_drained();
            drained_tx
                .send(())
                .expect("test receiver should remain live");
        });
        assert!(drained_rx.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).expect("callback should remain live");
        drained_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("unregistration should finish after the callback");
        callback.join().expect("callback thread should not panic");
        waiter.join().expect("waiting thread should not panic");
    }

    #[test]
    fn unregistering_from_current_callback_does_not_deadlock() {
        let state = DeliveryState::new();
        let guard = DeliveryState::enter(&state).expect("callback should enter");
        state.deactivate();
        state.wait_until_drained();
        assert!(DeliveryState::enter(&state).is_none());
        drop(guard);
        state.wait_until_drained();
    }

    #[test]
    fn unregistering_callback_still_waits_for_concurrent_callbacks() {
        let state = DeliveryState::new();
        let current_guard = DeliveryState::enter(&state).expect("callback should enter");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let concurrent_state = Arc::clone(&state);
        let concurrent = thread::spawn(move || {
            let guard =
                DeliveryState::enter(&concurrent_state).expect("concurrent callback should enter");
            entered_tx
                .send(())
                .expect("test receiver should remain live");
            release_rx.recv().expect("test sender should remain live");
            drop(guard);
        });
        entered_rx
            .recv()
            .expect("concurrent callback should report entry");
        state.deactivate();

        let (released_tx, released_rx) = mpsc::channel();
        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            released_tx
                .send(())
                .expect("test receiver should remain live");
            release_tx
                .send(())
                .expect("concurrent callback should remain live");
        });
        state.wait_until_drained();
        released_rx
            .try_recv()
            .expect("self-unregistration must wait for the concurrent callback");
        drop(current_guard);
        releaser.join().expect("releaser should not panic");
        concurrent
            .join()
            .expect("concurrent callback should not panic");
    }

    #[test]
    fn handler_panic_is_isolated_and_releases_delivery_guard() {
        let state = DeliveryState::new();
        let guard = DeliveryState::enter(&state).expect("callback should enter");
        let notification = Notification {
            name: "panic-test".to_owned(),
            object_description: None,
            user_info_description: None,
        };
        invoke_handler(&|_| panic!("expected callback panic"), notification);
        drop(guard);
        state.deactivate();
        state.wait_until_drained();
    }
}
