use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf, Prefix},
};

use windows::{
    Win32::{
        Foundation::{
            ERROR_INVALID_FUNCTION, ERROR_LOCK_VIOLATION, ERROR_NOT_SUPPORTED,
            ERROR_SHARING_VIOLATION, HANDLE, WIN32_ERROR,
        },
        Storage::FileSystem::{
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW,
            GetVolumeInformationW, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
            UnlockFileEx,
        },
        System::{IO::OVERLAPPED, WindowsProgramming::DRIVE_FIXED},
    },
    core::{Error as WindowsError, HRESULT, PCWSTR},
};

use crate::{HomeCloseError, HomeDurabilityTier, HomeLockCapability, HomeOpenError, HomeOpenStage};

const LOCK_RANGE_BYTES: u32 = 1;
const FILESYSTEM_NAME_CAPACITY: usize = 64;

/// Canonical configured-home facts captured before lock acquisition.
///
/// This value retains no directory handle. The lock file is the sole lifetime
/// OS ownership authority.
pub(crate) struct CanonicalHomePath {
    configured_path: PathBuf,
    canonical_path: PathBuf,
    durability_tier: HomeDurabilityTier,
    #[cfg(feature = "test-faults")]
    test_seam: Option<crate::HomeOwnershipTestSeam>,
}

impl CanonicalHomePath {
    pub(crate) fn open(configured_path: &Path) -> Result<Self, HomeOpenError> {
        if !configured_path.is_absolute() {
            return Err(HomeOpenError::open(
                configured_path,
                HomeOpenStage::ValidateConfiguredPath,
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Beryl-home path must be absolute",
                ),
            ));
        }

        fs::create_dir_all(configured_path).map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::CreateHomeDirectory, source)
        })?;
        let canonical_path = fs::canonicalize(configured_path).map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::OpenHomeDirectory, source)
        })?;
        let durability_tier = classify_durability(&canonical_path);

        Ok(Self {
            configured_path: configured_path.to_path_buf(),
            canonical_path,
            durability_tier,
            #[cfg(feature = "test-faults")]
            test_seam: None,
        })
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn with_test_seam(mut self, seam: crate::HomeOwnershipTestSeam) -> Self {
        if matches!(
            seam,
            crate::HomeOwnershipTestSeam::UncPath | crate::HomeOwnershipTestSeam::MappedRemotePath
        ) {
            self.durability_tier = HomeDurabilityTier::BestEffort;
        }
        self.test_seam = Some(seam);
        self
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn with_durability_tier(mut self, durability_tier: HomeDurabilityTier) -> Self {
        self.durability_tier = durability_tier;
        self
    }

    pub(crate) fn acquire_lock(self, lock_path: &Path) -> Result<HomeOwnership, HomeOpenError> {
        HomeOwnership::acquire(self, lock_path)
    }
}

/// The sole retained OS object for Beryl-home ownership.
pub(crate) struct HomeOwnership {
    configured_path: PathBuf,
    canonical_path: PathBuf,
    durability_tier: HomeDurabilityTier,
    lock_file: File,
    locked: bool,
}

impl HomeOwnership {
    fn acquire(directory: CanonicalHomePath, lock_path: &Path) -> Result<Self, HomeOpenError> {
        #[cfg(feature = "test-faults")]
        if directory.test_seam == Some(crate::HomeOwnershipTestSeam::UnsupportedExclusiveLock) {
            return Err(lock_unsupported(
                &directory.configured_path,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "synthetic unsupported Beryl-home exclusive lock",
                ),
            ));
        }
        let lock_file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
            .open(lock_path)
        {
            Ok(file) => file,
            Err(source) if source.raw_os_error() == Some(ERROR_SHARING_VIOLATION.0 as i32) => {
                return Err(HomeOpenError::Busy {
                    path: directory.configured_path,
                });
            }
            Err(source) => {
                return Err(HomeOpenError::open(
                    &directory.configured_path,
                    HomeOpenStage::OpenLockFile,
                    source,
                ));
            }
        };

        let mut overlapped = OVERLAPPED::default();
        let result = unsafe {
            // SAFETY: `lock_file` owns a valid synchronous Windows file handle,
            // `overlapped` lives through the call, and every Beryl opener uses
            // the same bounded byte range `[0, 1)`.
            LockFileEx(
                HANDLE(lock_file.as_raw_handle()),
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                None,
                LOCK_RANGE_BYTES,
                0,
                &mut overlapped,
            )
        };

        match result {
            Ok(()) => Ok(Self {
                configured_path: directory.configured_path,
                canonical_path: directory.canonical_path,
                durability_tier: directory.durability_tier,
                lock_file,
                locked: true,
            }),
            Err(source) if is_win32(&source, ERROR_LOCK_VIOLATION) => Err(HomeOpenError::Busy {
                path: directory.configured_path,
            }),
            Err(source)
                if is_win32(&source, ERROR_INVALID_FUNCTION)
                    || is_win32(&source, ERROR_NOT_SUPPORTED) =>
            {
                Err(lock_unsupported(
                    &directory.configured_path,
                    windows_io_error(source),
                ))
            }
            Err(source) => Err(HomeOpenError::open(
                &directory.configured_path,
                HomeOpenStage::AcquireLock,
                windows_io_error(source),
            )),
        }
    }

    pub(crate) fn configured_path(&self) -> &Path {
        &self.configured_path
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(crate) const fn durability_tier(&self) -> HomeDurabilityTier {
        self.durability_tier
    }

    pub(crate) fn release(&mut self) -> Result<(), HomeCloseError> {
        if !std::mem::replace(&mut self.locked, false) {
            return Ok(());
        }

        let mut overlapped = OVERLAPPED::default();
        unsafe {
            // SAFETY: this exact handle successfully acquired `[0, 1)`, and
            // the stack-owned `OVERLAPPED` remains valid for the synchronous
            // unlock call.
            UnlockFileEx(
                HANDLE(self.lock_file.as_raw_handle()),
                None,
                LOCK_RANGE_BYTES,
                0,
                &mut overlapped,
            )
        }
        .map_err(|source| HomeCloseError::ownership(windows_io_error(source)))
    }
}

impl Drop for HomeOwnership {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn classify_durability(canonical_path: &Path) -> HomeDurabilityTier {
    let Ok(root) = volume_root(canonical_path) else {
        return HomeDurabilityTier::BestEffort;
    };
    let drive_type = unsafe {
        // SAFETY: `root` is a live null-terminated UTF-16 volume root for the
        // duration of this synchronous Windows query.
        GetDriveTypeW(PCWSTR(root.as_ptr()))
    };
    if drive_type != DRIVE_FIXED {
        return HomeDurabilityTier::BestEffort;
    }

    let mut filesystem_name = [0_u16; FILESYSTEM_NAME_CAPACITY];
    let result = unsafe {
        // SAFETY: `root` and the mutable filesystem-name buffer remain live
        // for the synchronous query. No query failure changes open admission.
        GetVolumeInformationW(
            PCWSTR(root.as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut filesystem_name),
        )
    };
    if result.is_err() {
        return HomeDurabilityTier::BestEffort;
    }
    let length = filesystem_name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filesystem_name.len());
    let name = String::from_utf16_lossy(&filesystem_name[..length]);
    if name.eq_ignore_ascii_case("NTFS") {
        HomeDurabilityTier::Full
    } else {
        HomeDurabilityTier::BestEffort
    }
}

fn volume_root(canonical_path: &Path) -> io::Result<Vec<u16>> {
    let root = match canonical_path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) => format!("{}:\\", char::from(letter)),
            Prefix::VerbatimDisk(letter) => format!(r"\\?\{}:\", char::from(letter)),
            Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "UNC homes are best-effort storage",
                ));
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "canonical Windows path has no volume root",
                ));
            }
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "canonical Windows path has no drive prefix",
            ));
        }
    };
    Ok(root.encode_utf16().chain(std::iter::once(0)).collect())
}

fn is_win32(source: &WindowsError, code: WIN32_ERROR) -> bool {
    source.code() == HRESULT::from_win32(code.0)
}

fn windows_io_error(source: WindowsError) -> io::Error {
    io::Error::other(source)
}

fn lock_unsupported(path: &Path, source: io::Error) -> HomeOpenError {
    HomeOpenError::LockUnsupported {
        path: path.to_path_buf(),
        capability: HomeLockCapability::ExclusiveFileLock,
        source,
    }
}
