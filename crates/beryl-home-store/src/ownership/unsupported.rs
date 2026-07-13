use std::{io, path::Path};

use super::CanonicalHomeIdentity;
use crate::{HomeCloseError, HomeLockCapability, HomeOpenError};

pub(crate) struct OpenedHomeDirectory;

impl OpenedHomeDirectory {
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

    pub(crate) fn configured_path(&self) -> &Path {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn canonical_identity(&self) -> CanonicalHomeIdentity {
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

    pub(crate) fn canonical_identity(&self) -> CanonicalHomeIdentity {
        unreachable!("unsupported platform never opens a home")
    }

    pub(crate) fn release(&mut self) -> Result<(), HomeCloseError> {
        unreachable!("unsupported platform never opens a home")
    }
}
