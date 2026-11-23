//! Safe resource lifecycle and resource-view-pool extensions.

use super::{PurgeableState, Resource, ResourceID, ResourceViewPool};
use crate::Error;
use std::ops::Range;

/// Opaque owned identity used for Metal resource VM accounting.
///
/// Create this value through [`ResourceOwnerIdentity::current_process`]; raw
/// Mach port names are never exposed.
pub struct ResourceOwnerIdentity {
    inner: metal_rust_ffi::ResourceOwnerIdentity,
}

impl ResourceOwnerIdentity {
    /// Creates an owned identity token for the current process.
    pub fn current_process() -> Result<Self, Error> {
        metal_rust_ffi::ResourceOwnerIdentity::current_process()
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
}

impl Resource {
    /// Returns whether this heap allocation has been made aliasable.
    pub fn is_aliasable(&self) -> Result<bool, Error> {
        self.inner.is_aliasable().map_err(Error::from_ffi)
    }

    /// Makes a validated direct heap allocation available for future aliasing.
    pub fn make_aliasable(&self) -> Result<(), Error> {
        self.inner.make_aliasable().map_err(Error::from_ffi)
    }

    /// Assigns VM-accounting ownership through an opaque owned identity.
    pub fn set_owner(&self, identity: &ResourceOwnerIdentity) -> Result<(), Error> {
        self.inner
            .set_owner(&identity.inner)
            .map_err(Error::from_ffi)
    }

    /// Changes the resource's purgeable state using a checked enum value.
    pub fn set_purgeable_state(&self, state: PurgeableState) -> Result<PurgeableState, Error> {
        self.inner
            .set_purgeable_state(state)
            .map_err(Error::from_ffi)
    }
}

impl ResourceViewPool {
    /// Returns an owned snapshot of the pool's base resource identifier.
    pub fn base_resource_id(&self) -> Result<ResourceID, Error> {
        self.inner.base_resource_id().map_err(Error::from_ffi)
    }

    /// Copies a checked source range and returns the destination ID snapshot.
    pub fn copy_resource_views_from_pool(
        &self,
        source: &ResourceViewPool,
        source_range: Range<usize>,
        destination_index: usize,
    ) -> Result<ResourceID, Error> {
        self.inner
            .copy_resource_views_from_pool(&source.inner, source_range, destination_index)
            .map_err(Error::from_ffi)
    }
}
