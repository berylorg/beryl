use crate::{
    LoadedThreadSession, ManagedBackendReleaseAdmission, ManagedBackendServer,
    ManagedBackendSession, server::ManagedLaunchProvenance,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId, GeneratedSourceIncarnation, RuntimeNativePath};
use std::{
    fs::File,
    io,
    sync::{
        Arc, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) mod authority;
pub(crate) mod source;
pub use source::{
    GeneratedOutputCloseFailure, GeneratedOutputCursor, GeneratedOutputOpenFailure,
    GeneratedOutputSource,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GeneratedOutputError {
    #[error("generated source owner is busy")]
    Busy,
    #[error("the original generated source authority is unavailable")]
    LostSourceAuthority,
    #[error("generated source reading was cancelled")]
    Cancelled,
    #[error("generated source path is invalid")]
    InvalidPath,
    #[error("generated source is missing")]
    Missing,
    #[error("generated source is unsupported")]
    Unsupported,
    #[error("generated source changed")]
    Changed,
    #[error("generated source reading failed")]
    Io,
    #[error("generated source protocol failed")]
    Protocol,
    #[error("generated source disposal remains pending")]
    CleanupPending,
}
impl GeneratedOutputError {
    pub fn into_io_error(self) -> io::Error {
        io::Error::other(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedReaderAllowance(pub(crate) u32);
impl GeneratedReaderAllowance {
    pub fn new(workers: usize) -> Result<Self, GeneratedOutputError> {
        let value = u32::try_from(workers).map_err(|_| GeneratedOutputError::Unsupported)?;
        if value == 0 {
            return Err(GeneratedOutputError::Unsupported);
        }
        Ok(Self(value))
    }
}

pub struct GeneratedOutputTarget {
    resource: [u8; 16],
    turn: CasTurnId,
    item: CasItemId,
}
impl GeneratedOutputTarget {
    pub fn new(resource: [u8; 16], turn: CasTurnId, item: CasItemId) -> Self {
        Self {
            resource,
            turn,
            item,
        }
    }
}
pub struct GeneratedOutputPermit {
    pub(crate) launch: crate::ManagedBackendLaunchIdentity,
    loaded: Arc<LoadedSourceAuthority>,
    target: GeneratedOutputTarget,
}

#[derive(Debug)]
pub(crate) struct LoadedSourceAuthority {
    thread: CasThreadId,
    pub(crate) incarnation: GeneratedSourceIncarnation,
    pub(crate) live: AtomicBool,
}
impl PartialEq for LoadedSourceAuthority {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl Eq for LoadedSourceAuthority {}

#[derive(Debug, Default)]
pub(crate) struct ProcessSourceFiles {
    retired: AtomicBool,
    files: Mutex<Vec<Weak<HostFile>>>,
    allowance: Mutex<Option<u32>>,
}
impl PartialEq for ProcessSourceFiles {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
impl Eq for ProcessSourceFiles {}
#[derive(Debug)]
struct HostFile {
    file: Mutex<Option<File>>,
}
pub(crate) fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
impl ProcessSourceFiles {
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
        for file in lock(&self.files).iter().filter_map(Weak::upgrade) {
            lock(&file.file).take();
        }
    }
}
