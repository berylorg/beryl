#[path = "wsl_supervisor_artifact/identity.rs"]
mod identity;

use beryl_backend::WslSupervisorArtifact;
use std::sync::Arc;

include!(concat!(env!("OUT_DIR"), "/wsl_supervisor_release.rs"));

pub(crate) fn bundled() -> Option<Arc<WslSupervisorArtifact>> {
    let (digest, version) = BUNDLED_ARTIFACT?;
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let executable = std::env::current_exe().ok()?;
        open_sibling(&executable, digest, version)
    }
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        let _ = (digest, version);
        None
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) fn open_sibling(
    executable: &std::path::Path,
    expected: [u8; 32],
    version: u16,
) -> Option<Arc<WslSupervisorArtifact>> {
    use std::{
        fs::{File, OpenOptions},
        os::windows::fs::OpenOptionsExt,
    };
    let executable = std::fs::canonicalize(executable).ok()?;
    let directory = executable.parent()?;
    let sibling = directory.join("beryl-wsl-supervisor-linux-x86_64");
    let path = std::fs::canonicalize(sibling).ok()?;
    if path.parent()? != directory
        || path.file_name()? != std::ffi::OsStr::new("beryl-wsl-supervisor-linux-x86_64")
    {
        return None;
    }
    let linux_path = identity::linux_path(path.to_str()?)?;
    let mut pin: File = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .ok()?;
    if !pin.metadata().ok()?.is_file() || identity::digest(&mut pin).ok()? != expected {
        return None;
    }
    WslSupervisorArtifact::from_verified_release(path, linux_path, expected, version, Arc::new(pin))
        .ok()
        .map(Arc::new)
}
