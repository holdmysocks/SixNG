//! Audited wrappers for Metal's global device discovery functions.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::Device;
use crate::metal::generated_object_types::metal::LogState;
use crate::metal::generated_value_types::LogLevel;
use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCopyAllDevices, MTLCopyAllDevicesWithObserver, MTLDevice, MTLRemoveDeviceObserver,
};
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::{Arc, Condvar, Mutex};

thread_local! {
    static EXECUTING_DEVICE_CALLBACKS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

struct DeliveryStatus {
    active: bool,
    in_flight: usize,
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
        status.in_flight = status.in_flight.checked_add(1)?;
        drop(status);
        let identity = Arc::as_ptr(state) as usize;
        EXECUTING_DEVICE_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(identity));
        Some(DeliveryGuard {
            state: Arc::clone(state),
            identity,
        })
    }

    fn deactivate_and_wait(&self) {
        let identity = self as *const Self as usize;
        let executing_here = EXECUTING_DEVICE_CALLBACKS.with(|callbacks| {
            callbacks
                .borrow()
                .iter()
                .filter(|candidate| **candidate == identity)
                .count()
        });
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.active = false;
        while status.in_flight > executing_here {
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
        EXECUTING_DEVICE_CALLBACKS.with(|callbacks| {
            let mut callbacks = callbacks.borrow_mut();
            if let Some(position) = callbacks
                .iter()
                .rposition(|identity| *identity == self.identity)
            {
                callbacks.remove(position);
            }
        });
        let mut status = self
            .state
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.in_flight = status.in_flight.saturating_sub(1);
        self.state.drained.notify_all();
    }
}

/// RAII registration for Metal device add/remove notifications.
pub struct DeviceObserverRegistration {
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    delivery: Arc<DeliveryState>,
    handler: Arc<Mutex<Option<Arc<DeviceNotificationHandler>>>>,
    _thread_bound: ThreadBound,
}

type DeviceNotificationHandler = dyn Fn(Device, String) + Send + Sync + 'static;
type DeviceNotificationBlock = dyn Fn(NonNull<ProtocolObject<dyn MTLDevice>>, NonNull<NSString>);

/// RAII gate for a repeatable Metal log handler.
pub struct LogHandlerRegistration {
    delivery: Arc<DeliveryState>,
    handler: Arc<Mutex<Option<Arc<LogHandler>>>>,
    _thread_bound: ThreadBound,
}

type LogHandler = dyn Fn(String, String, LogLevel, String) + Send + Sync + 'static;

impl Drop for LogHandlerRegistration {
    fn drop(&mut self) {
        self.delivery.deactivate_and_wait();
        self.handler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

impl LogState {
    /// Adds a panic-isolated log handler whose Rust delivery is disabled on Drop.
    pub fn add_log_handler(
        &self,
        handler: impl Fn(String, String, LogLevel, String) + Send + Sync + 'static,
    ) -> Result<LogHandlerRegistration, Error> {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // the selector argument has Objective-C's stable Sel representation.
        let available: bool =
            unsafe { msg_send![self.as_inner(), respondsToSelector: sel!(addLogHandler:)] };
        if !available {
            return Err(Error::unsupported("Metal log handlers are unavailable"));
        }
        let delivery = DeliveryState::new();
        let callback_delivery = Arc::clone(&delivery);
        let handler: Arc<LogHandler> = Arc::new(handler);
        let handler = Arc::new(Mutex::new(Some(handler)));
        let callback_handler = Arc::clone(&handler);
        let block = RcBlock::new(
            move |subsystem: NonNull<NSString>,
                  category: NonNull<NSString>,
                  level: isize,
                  message: NonNull<NSString>| {
                let Some(_guard) = DeliveryState::enter(&callback_delivery) else {
                    return;
                };
                // SAFETY: Metal supplies non-null NSString values for the
                // callback duration; each is copied before the callback ends.
                let subsystem = unsafe { subsystem.as_ref() }.to_string();
                // SAFETY: see the callback ABI argument proof above.
                let category = unsafe { category.as_ref() }.to_string();
                // SAFETY: see the callback ABI argument proof above.
                let message = unsafe { message.as_ref() }.to_string();
                let handler = callback_handler
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                let Some(handler) = handler else {
                    return;
                };
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    handler(
                        subsystem,
                        category,
                        LogLevel::from_system_raw(level),
                        message,
                    )
                }));
            },
        );
        // SAFETY: selector presence is checked and the block has the exact
        // NSString/NSInteger callback ABI declared by MTLLogState.
        unsafe {
            let _: () = msg_send![self.as_inner(), addLogHandler: &*block];
        }
        Ok(LogHandlerRegistration {
            delivery,
            handler,
            _thread_bound: ThreadBound::new(),
        })
    }
}

impl Drop for DeviceObserverRegistration {
    fn drop(&mut self) {
        {
            let mut status = self
                .delivery
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            status.active = false;
        }
        // SAFETY: `observer` is exactly the +1 observer returned by
        // MTLCopyAllDevicesWithObserver and remains alive for this call.
        unsafe { MTLRemoveDeviceObserver(&self.observer) };
        self.delivery.deactivate_and_wait();
        self.handler
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

/// Returns owned wrappers for every Metal device currently available.
#[must_use]
pub fn all_devices() -> Vec<Device> {
    MTLCopyAllDevices()
        .to_vec()
        .into_iter()
        .map(Device::from_inner)
        .collect()
}

/// Returns all current devices and installs a repeatable RAII observer.
pub fn all_devices_with_observer(
    handler: impl Fn(Device, String) + Send + Sync + 'static,
) -> Result<(Vec<Device>, DeviceObserverRegistration), Error> {
    let delivery = DeliveryState::new();
    let callback_delivery = Arc::clone(&delivery);
    let handler: Arc<DeviceNotificationHandler> = Arc::new(handler);
    let handler = Arc::new(Mutex::new(Some(handler)));
    let callback_handler = Arc::clone(&handler);
    let block = RcBlock::new(
        move |device: NonNull<ProtocolObject<dyn MTLDevice>>, name: NonNull<NSString>| {
            let Some(_guard) = DeliveryState::enter(&callback_delivery) else {
                return;
            };
            // SAFETY: Metal provides both non-null callback arguments for the
            // callback duration. Retaining the device gives Rust independent
            // ownership; the notification name is copied into a Rust String.
            let device = unsafe { Retained::retain(device.as_ptr()) }.map(Device::from_inner);
            // SAFETY: the non-null NSString is valid throughout this callback.
            let name = unsafe { name.as_ref() }.to_string();
            if let Some(device) = device {
                let handler = callback_handler
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                if let Some(handler) = handler {
                    let _ = catch_unwind(AssertUnwindSafe(|| handler(device, name)));
                }
            }
        },
    );
    let block: &DynBlock<DeviceNotificationBlock> = &block;
    let mut observer = std::ptr::null_mut::<ProtocolObject<dyn NSObjectProtocol>>();
    // SAFETY: `observer` is a valid writable out pointer and `block` has the
    // exact MTLDeviceNotificationHandler ABI. Metal copies the block.
    let devices = unsafe {
        MTLCopyAllDevicesWithObserver(
            NonNull::from(&mut observer),
            std::ptr::from_ref(block).cast_mut(),
        )
    };
    // SAFETY: Metal documents the non-null observer out value as returned at
    // +1 retain count. A null defensive check is converted into a Rust error.
    let observer = unsafe { Retained::from_raw(observer) }
        .ok_or_else(|| Error::unsupported("Metal did not return a device observer"))?;
    let devices = devices
        .to_vec()
        .into_iter()
        .map(Device::from_inner)
        .collect();
    Ok((
        devices,
        DeviceObserverRegistration {
            observer,
            delivery,
            handler,
            _thread_bound: ThreadBound::new(),
        },
    ))
}

/// Major version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_MAJOR: u32 = 381;
/// Minor version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_MINOR: u32 = 0;
/// Patch version of the local metal-cpp reference used for inventory.
pub const METAL_CPP_VERSION_PATCH: u32 = 0;

const fn version_key(major: u32, minor: u32, patch: u32) -> u128 {
    ((major as u128) << 64) | ((minor as u128) << 32) | patch as u128
}

/// Returns whether the local metal-cpp reference is at least the given version.
#[must_use]
pub const fn metal_cpp_supports_version(major: u32, minor: u32, patch: u32) -> bool {
    version_key(major, minor, patch)
        <= version_key(
            METAL_CPP_VERSION_MAJOR,
            METAL_CPP_VERSION_MINOR,
            METAL_CPP_VERSION_PATCH,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deactivation_closes_callback_entry() {
        let state = DeliveryState::new();
        let guard = DeliveryState::enter(&state).expect("initial callback entry");
        state.deactivate_and_wait();
        drop(guard);
        assert!(DeliveryState::enter(&state).is_none());
    }

    #[test]
    fn version_query_matches_metal_cpp_macro_ordering() {
        assert!(metal_cpp_supports_version(381, 0, 0));
        assert!(metal_cpp_supports_version(380, u32::MAX, u32::MAX));
        assert!(!metal_cpp_supports_version(381, 0, 1));
    }
}
