//! Safe Foundation notification facade.

/// An owned notification value delivered to Rust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    inner: metal_rust_ffi::Notification,
}

impl Notification {
    /// Returns the copied notification name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Returns an owned textual substitute for the untyped Objective-C object.
    #[must_use]
    pub fn object_description(&self) -> Option<&str> {
        self.inner.object_description()
    }

    /// Returns an owned textual substitute for the untyped user-info dictionary.
    #[must_use]
    pub fn user_info_description(&self) -> Option<&str> {
        self.inner.user_info_description()
    }
}

/// RAII notification observer. Drop disables delivery and unregisters it.
pub struct Registration {
    _inner: metal_rust_ffi::Registration,
}

/// The process default Foundation notification center.
pub struct NotificationCenter {
    inner: metal_rust_ffi::NotificationCenter,
}

impl NotificationCenter {
    /// Returns the process default notification center.
    #[must_use]
    pub fn default_center() -> Self {
        Self {
            inner: metal_rust_ffi::NotificationCenter::default_center(),
        }
    }

    /// Registers a repeatable callback. Dropping the returned registration
    /// prevents later callback delivery.
    #[must_use]
    pub fn add_observer(
        &self,
        name: impl AsRef<str>,
        handler: impl Fn(Notification) + Send + Sync + 'static,
    ) -> Registration {
        let inner = self.inner.add_observer(name.as_ref(), move |inner| {
            handler(Notification { inner });
        });
        Registration { _inner: inner }
    }

    /// Posts a name-only notification synchronously.
    pub fn post(&self, name: impl AsRef<str>) {
        self.inner.post(name.as_ref());
    }
}
