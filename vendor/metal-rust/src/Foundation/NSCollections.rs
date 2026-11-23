//! Rust-native substitutes for Foundation ownership and collection helpers.

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};

/// Rust-native substitute for `NS::Array`.
pub type Array<T> = Vec<T>;
/// Rust-native substitute for `NS::Data`.
pub type Data = Vec<u8>;
/// Rust-native substitute for `NS::Date`.
pub type Date = SystemTime;
/// Rust-native substitute for `NS::Dictionary`.
pub type Dictionary<K, V> = HashMap<K, V>;
/// Rust-native substitute for `NS::Set`.
pub type Set<T> = HashSet<T>;
/// Rust-native substitute for `NS::String`.
pub type String = std::string::String;
/// Iterator substitute for Foundation enumerators.
pub type Enumerator<T> = std::vec::IntoIter<T>;

/// Maps Foundation locking to a safe mutex.
pub trait Locking<T> {
    /// Executes a closure while holding the lock.
    fn with_lock<R>(&self, operation: impl FnOnce(&mut T) -> R) -> R;
}

impl<T> Locking<T> for Mutex<T> {
    fn with_lock<R>(&self, operation: impl FnOnce(&mut T) -> R) -> R {
        let mut value = self.lock().unwrap_or_else(|error| error.into_inner());
        operation(&mut value)
    }
}

/// Converts an owned array into its consuming Foundation-style enumerator.
#[must_use]
pub fn enumerate<T>(values: Array<T>) -> Enumerator<T> {
    values.into_iter()
}

/// Returns a new string containing both inputs.
#[must_use]
pub fn append_string(left: &str, right: &str) -> String {
    let mut value = String::with_capacity(left.len().saturating_add(right.len()));
    value.push_str(left);
    value.push_str(right);
    value
}

/// Creates a date offset from the current system time.
#[must_use]
pub fn date_with_time_interval_since_now(seconds: f64) -> Option<Date> {
    if !seconds.is_finite() {
        return None;
    }
    let duration = Duration::from_secs_f64(seconds.abs());
    if seconds.is_sign_negative() {
        SystemTime::now().checked_sub(duration)
    } else {
        SystemTime::now().checked_add(duration)
    }
}

/// Safe, typed substitute for Foundation's untyped `NS::Value` byte/pointer API.
#[derive(Clone)]
pub struct Value(Arc<dyn Any + Send + Sync>);

impl Value {
    /// Stores an owned, thread-safe Rust value.
    #[must_use]
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    /// Attempts to borrow the stored value with its original Rust type.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }
}

/// Marker mapping metal-cpp copy helpers to Rust `Clone`.
pub trait Copying: Clone {
    /// Returns an owned copy.
    #[must_use]
    fn copy_owned(&self) -> Self {
        self.clone()
    }
}

impl<T: Clone> Copying for T {}

/// Rust-native object semantics based on `Eq` and `Hash`.
pub trait Object: Clone + Eq + Hash {
    /// Returns the Rust hash used by the safe substitute.
    #[must_use]
    fn object_hash(&self) -> u64 {
        hash_value(self)
    }

    /// Tests value equality without exposing Objective-C identity pointers.
    #[must_use]
    fn object_is_equal(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: Clone + Eq + Hash> Object for T {}

/// Marker for values whose Rust-owned representation can cross persistence
/// boundaries without exposing Objective-C coder pointers.
pub trait SecureCoding: Clone + Send + Sync {}

impl<T: Clone + Send + Sync> SecureCoding for T {}

/// A safe condition variable with internal predicate state.
#[derive(Default)]
pub struct Condition {
    state: Mutex<bool>,
    condition: Condvar,
}

impl Condition {
    /// Creates an unsignalled condition.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Blocks until signalled, recovering a poisoned mutex state.
    pub fn wait(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        while !*state {
            state = self
                .condition
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        *state = false;
    }

    /// Waits until the deadline and reports whether a signal arrived.
    #[must_use]
    pub fn wait_until(&self, deadline: Date) -> bool {
        let duration = deadline
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO);
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (mut state, timeout) = self
            .condition
            .wait_timeout_while(state, duration, |ready| !*ready)
            .unwrap_or_else(|error| error.into_inner());
        let signalled = *state && !timeout.timed_out();
        if signalled {
            *state = false;
        }
        signalled
    }

    /// Wakes one waiter.
    pub fn signal(&self) {
        *self.state.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.condition.notify_one();
    }

    /// Wakes every current waiter.
    pub fn broadcast(&self) {
        *self.state.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.condition.notify_all();
    }
}

/// Extension methods used in place of Foundation string pointer APIs.
pub trait StringExt {
    /// Returns the Unicode scalar at a character index.
    #[must_use]
    fn character(&self, index: usize) -> Option<char>;
    /// Finds a substring and returns its byte range.
    #[must_use]
    fn range_of(&self, needle: &str) -> Option<std::ops::Range<usize>>;
    /// Compares strings using Unicode lowercase conversion.
    #[must_use]
    fn compare_case_insensitive(&self, other: &str) -> std::cmp::Ordering;
}

impl StringExt for str {
    fn character(&self, index: usize) -> Option<char> {
        self.chars().nth(index)
    }

    fn range_of(&self, needle: &str) -> Option<std::ops::Range<usize>> {
        self.find(needle).map(|start| start..start + needle.len())
    }

    fn compare_case_insensitive(&self, other: &str) -> std::cmp::Ordering {
        self.to_lowercase().cmp(&other.to_lowercase())
    }
}

/// Returns whether two hashable Rust-native Foundation values are equal.
#[must_use]
pub fn is_equal<T: Eq>(left: &T, right: &T) -> bool {
    left == right
}

/// Returns a stable hash value for the current process.
#[must_use]
pub fn hash_value<T: Hash>(value: &T) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}
