#[cfg(target_os = "windows")]
#[path = "ownership/windows.rs"]
mod platform;
#[cfg(not(target_os = "windows"))]
#[path = "ownership/unsupported.rs"]
mod platform;

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub(crate) use platform::{CanonicalHomePath, HomeOwnership};

/// Process-local lifetime owner for one successfully opened home.
///
/// The mutable ownership lock remains private behind the custodian. Both the
/// live store and any retained reconciliation registry hold this same value,
/// so descriptor custody cannot release the home merely by outliving a store.
pub(crate) struct HomeLifecycleCustodian {
    configured_path: PathBuf,
    canonical_path: PathBuf,
    durability_tier: crate::HomeDurabilityTier,
    ownership: Mutex<Option<HomeOwnership>>,
}

impl HomeLifecycleCustodian {
    pub(crate) fn new(ownership: HomeOwnership) -> Self {
        Self {
            configured_path: ownership.configured_path().to_path_buf(),
            canonical_path: ownership.canonical_path().to_path_buf(),
            durability_tier: ownership.durability_tier(),
            ownership: Mutex::new(Some(ownership)),
        }
    }

    pub(crate) fn configured_path(&self) -> &Path {
        &self.configured_path
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(crate) const fn durability_tier(&self) -> crate::HomeDurabilityTier {
        self.durability_tier
    }

    pub(crate) fn release(&self) -> Result<(), crate::HomeCloseError> {
        let mut ownership = self
            .ownership
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match ownership.as_mut() {
            Some(ownership) => ownership.release(),
            None => Ok(()),
        }
    }
}
