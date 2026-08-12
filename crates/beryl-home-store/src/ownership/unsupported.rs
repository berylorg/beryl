use std::{io, path::Path};

use crate::{HomeCloseError, HomeLockCapability, HomeOpenError};

pub(crate) struct CanonicalHomePath;

impl CanonicalHomePath {
    pub(crate) fn open(configured_path: &Path) -> Result<Self, HomeOpenError> {
        Err(HomeOpenError::LockUnsupported {
            path: configured_path.to_path_buf(),
            capability: HomeLockCapability::WindowsPlatform,
            source: io::Error::new(
                io::ErrorKind::Unsupported,
                "Beryl-home ownership is implemented only for Windows",
            ),
        })
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        unreachable!("unsupported platform never opens a home")
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn with_test_seam(self, _seam: crate::HomeOwnershipTestSeam) -> Self {
        unreachable!("unsupported platform never opens a home")
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn with_durability_tier(self, _tier: crate::HomeDurabilityTier) -> Self {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn acquire_lock(self, _lock_path: &Path) -> Result<HomeOwnership, HomeOpenError> {
        unreachable!("unsupported platform never opens a home")
    }
}

pub(crate) struct HomeOwnership;

impl HomeOwnership {
    pub(crate) fn configured_path(&self) -> &Path {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn durability_tier(&self) -> crate::HomeDurabilityTier {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn release(&mut self) -> Result<(), HomeCloseError> {
        unreachable!("unsupported platform never opens a home")
    }
}
