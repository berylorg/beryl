use std::{fs::File, path::PathBuf, sync::Arc};

use crate::ManagedBackendError;

#[derive(Clone, Debug)]
pub struct WslSupervisorArtifact {
    host_path: PathBuf,
    linux_path: String,
    digest: [u8; 32],
    protocol_version: u16,
    _pin: Arc<File>,
}

impl WslSupervisorArtifact {
    pub fn from_verified_release(
        host_path: PathBuf,
        linux_path: String,
        digest: [u8; 32],
        protocol_version: u16,
        pin: Arc<File>,
    ) -> Result<Self, ManagedBackendError> {
        let drive_projection = linux_path
            .strip_prefix("/mnt/")
            .and_then(|path| path.split_once('/'))
            .is_some_and(|(drive, suffix)| {
                drive.len() == 1 && drive.as_bytes()[0].is_ascii_lowercase() && !suffix.is_empty()
            });
        if protocol_version != beryl_wsl_supervisor::PROTOCOL_VERSION
            || !drive_projection
            || !linux_path.starts_with("/mnt/")
            || linux_path.len() > 4096
            || linux_path.contains('\0')
            || linux_path
                .split('/')
                .any(|part| part == ".." || part == ".")
        {
            return Err(ManagedBackendError::WslArtifactUnavailable);
        }
        Ok(Self {
            host_path,
            linux_path,
            digest,
            protocol_version,
            _pin: pin,
        })
    }

    pub fn host_path(&self) -> &std::path::Path {
        &self.host_path
    }
    pub fn linux_path(&self) -> &str {
        &self.linux_path
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn protocol_version(&self) -> u16 {
        self.protocol_version
    }
}

impl PartialEq for WslSupervisorArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.host_path == other.host_path
            && self.linux_path == other.linux_path
            && self.digest == other.digest
            && self.protocol_version == other.protocol_version
            && Arc::ptr_eq(&self._pin, &other._pin)
    }
}
impl Eq for WslSupervisorArtifact {}
