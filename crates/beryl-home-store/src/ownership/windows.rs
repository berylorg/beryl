use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io,
    mem::size_of,
    os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf, Prefix},
};

use windows::{
    core::{Error as WindowsError, HRESULT, PCWSTR},
    Win32::{
        Foundation::{
            ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_LOCK_VIOLATION, ERROR_NOACCESS,
            ERROR_NOT_SUPPORTED, ERROR_SHARING_VIOLATION, HANDLE, WIN32_ERROR,
        },
        Storage::FileSystem::{
            FileIdInfo, FileRemoteProtocolInfo, GetDriveTypeW, GetFileInformationByHandleEx,
            GetFinalPathNameByHandleW, LockFileEx, UnlockFileEx, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_ID_INFO, FILE_NAME_NORMALIZED, FILE_REMOTE_PROTOCOL_INFO, FILE_SHARE_READ,
            FILE_SHARE_WRITE, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        },
        System::{
            WindowsProgramming::{DRIVE_NO_ROOT_DIR, DRIVE_REMOTE, DRIVE_UNKNOWN},
            IO::OVERLAPPED,
        },
    },
};

use super::CanonicalHomeIdentity;
use crate::{HomeCloseError, HomeLockCapability, HomeOpenError, HomeOpenStage};

#[path = "windows/state_directory.rs"]
mod state_directory;

use state_directory::RetainedStateDirectory;

const INITIAL_FINAL_PATH_UNITS: usize = 512;
const MAX_FINAL_PATH_UNITS: usize = 32_768;
const LOCK_RANGE_BYTES: u32 = 1;

pub(crate) struct OpenedHomeDirectory {
    configured_path: PathBuf,
    canonical_path: PathBuf,
    canonical_identity: CanonicalHomeIdentity,
    _handle: File,
}

impl OpenedHomeDirectory {
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
        if path_is_unc(configured_path) {
            return Err(lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "generic UNC Beryl homes do not provide the required ownership and durability proof",
                ),
            ));
        }

        fs::create_dir_all(configured_path).map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::CreateHomeDirectory, source)
        })?;

        let handle = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(configured_path)
            .map_err(|source| {
                HomeOpenError::open(configured_path, HomeOpenStage::OpenHomeDirectory, source)
            })?;

        let canonical_identity = query_identity(&handle).map_err(|source| {
            lock_unsupported(
                configured_path,
                HomeLockCapability::OpenedObjectIdentity,
                windows_io_error(source),
            )
        })?;
        let canonical_path = query_final_path(&handle).map_err(|source| {
            lock_unsupported(
                configured_path,
                HomeLockCapability::OpenedObjectIdentity,
                source,
            )
        })?;

        if path_is_unc(&canonical_path) {
            return Err(lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "opened Beryl-home target is remote storage",
                ),
            ));
        }
        let drive_type = query_drive_type(&canonical_path).map_err(|source| {
            lock_unsupported(configured_path, HomeLockCapability::LocalStorage, source)
        })?;
        if drive_type == DRIVE_REMOTE {
            return Err(lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "opened Beryl-home target resolves to a remote drive",
                ),
            ));
        }
        if matches!(drive_type, DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR) {
            return Err(lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "opened Beryl-home target has no supported local drive root",
                ),
            ));
        }

        let remote_protocol = query_remote_protocol(&handle).map_err(|source| {
            lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                windows_io_error(source),
            )
        })?;
        if remote_protocol {
            return Err(lock_unsupported(
                configured_path,
                HomeLockCapability::LocalStorage,
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "opened Beryl-home target is remote storage",
                ),
            ));
        }

        Ok(Self {
            configured_path: configured_path.to_path_buf(),
            canonical_path,
            canonical_identity,
            _handle: handle,
        })
    }

    pub(crate) fn configured_path(&self) -> &Path {
        &self.configured_path
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub(crate) fn canonical_identity(&self) -> CanonicalHomeIdentity {
        self.canonical_identity
    }

    pub(crate) fn acquire_lock(self, lock_path: &Path) -> Result<HomeOwnership, HomeOpenError> {
        HomeOwnership::acquire(self, lock_path)
    }
}

pub(crate) struct HomeOwnership {
    directory: OpenedHomeDirectory,
    lock_file: File,
    state_directory: Option<RetainedStateDirectory>,
    locked: bool,
}

impl HomeOwnership {
    fn acquire(directory: OpenedHomeDirectory, lock_path: &Path) -> Result<Self, HomeOpenError> {
        let configured_path = directory.configured_path();
        let lock_file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .open(lock_path)
        {
            Ok(file) => file,
            Err(source) if source.raw_os_error() == Some(ERROR_SHARING_VIOLATION.0 as i32) => {
                return Err(HomeOpenError::Busy {
                    path: configured_path.to_path_buf(),
                });
            }
            Err(source) => {
                return Err(HomeOpenError::open(
                    configured_path,
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
                directory,
                lock_file,
                state_directory: None,
                locked: true,
            }),
            Err(source) if is_win32(&source, ERROR_LOCK_VIOLATION) => Err(HomeOpenError::Busy {
                path: configured_path.to_path_buf(),
            }),
            Err(source)
                if is_win32(&source, ERROR_INVALID_FUNCTION)
                    || is_win32(&source, ERROR_NOT_SUPPORTED) =>
            {
                Err(lock_unsupported(
                    configured_path,
                    HomeLockCapability::ExclusiveFileLock,
                    windows_io_error(source),
                ))
            }
            Err(source) => Err(HomeOpenError::open(
                configured_path,
                HomeOpenStage::AcquireLock,
                windows_io_error(source),
            )),
        }
    }

    pub(crate) fn configured_path(&self) -> &Path {
        self.directory.configured_path()
    }

    pub(crate) fn canonical_path(&self) -> &Path {
        self.directory.canonical_path()
    }

    pub(crate) fn canonical_identity(&self) -> CanonicalHomeIdentity {
        self.directory.canonical_identity()
    }

    pub(crate) fn retain_state_directory(&mut self, path: &Path) -> io::Result<()> {
        debug_assert!(self.state_directory.is_none());
        self.state_directory = Some(RetainedStateDirectory::open_or_create(
            path,
            &self.directory._handle,
        )?);
        Ok(())
    }

    pub(crate) fn require_same_state_directory(&self, path: &Path) -> io::Result<()> {
        self.state_directory
            .as_ref()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "the retained Beryl state directory is unavailable",
                )
            })?
            .require_same(path)
    }

    pub(crate) fn release(&mut self) -> Result<(), HomeCloseError> {
        drop(self.state_directory.take());
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
        .map_err(|source| HomeCloseError {
            source: windows_io_error(source),
        })
    }
}

impl Drop for HomeOwnership {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn query_identity(file: &File) -> Result<CanonicalHomeIdentity, WindowsError> {
    let mut info = FILE_ID_INFO::default();
    unsafe {
        // SAFETY: the handle is live, `info` is correctly sized for
        // `FileIdInfo`, and Windows writes only within that stack value.
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    }?;

    Ok(CanonicalHomeIdentity::new(
        info.VolumeSerialNumber,
        info.FileId.Identifier,
    ))
}

fn query_remote_protocol(file: &File) -> Result<bool, WindowsError> {
    let mut info = FILE_REMOTE_PROTOCOL_INFO {
        StructureVersion: 2,
        StructureSize: size_of::<FILE_REMOTE_PROTOCOL_INFO>() as u16,
        ..FILE_REMOTE_PROTOCOL_INFO::default()
    };
    let result = unsafe {
        // SAFETY: the handle is live, `info` is correctly sized for
        // `FileRemoteProtocolInfo`, and Windows writes only within it.
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileRemoteProtocolInfo,
            (&mut info as *mut FILE_REMOTE_PROTOCOL_INFO).cast(),
            size_of::<FILE_REMOTE_PROTOCOL_INFO>() as u32,
        )
    };

    match result {
        Ok(()) => Ok(true),
        Err(source)
            if is_win32(&source, ERROR_INVALID_PARAMETER)
                || is_win32(&source, ERROR_INVALID_FUNCTION)
                || is_win32(&source, ERROR_NOACCESS)
                || is_win32(&source, ERROR_NOT_SUPPORTED) =>
        {
            Ok(false)
        }
        Err(source) => Err(source),
    }
}

fn query_drive_type(canonical_path: &Path) -> io::Result<u32> {
    let drive = match canonical_path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) => format!("{}:\\", char::from(letter)),
            Prefix::VerbatimDisk(letter) => format!(r"\\?\{}:\", char::from(letter)),
            Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) => return Ok(DRIVE_REMOTE),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "canonical Windows path does not expose a supported drive root",
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

    let mut encoded: Vec<u16> = drive.encode_utf16().collect();
    encoded.push(0);
    let drive_type = unsafe {
        // SAFETY: `encoded` is a live, null-terminated UTF-16 root path for
        // the duration of the call.
        GetDriveTypeW(PCWSTR(encoded.as_ptr()))
    };
    Ok(drive_type)
}

fn query_final_path(file: &File) -> io::Result<PathBuf> {
    let mut buffer = vec![0; INITIAL_FINAL_PATH_UNITS];
    loop {
        let length = unsafe {
            // SAFETY: the handle is live and the generated wrapper receives a
            // valid writable UTF-16 buffer for the duration of the call.
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        };
        if length == 0 {
            return Err(windows_io_error(WindowsError::from_win32()));
        }

        let length = length as usize;
        if length < buffer.len() {
            buffer.truncate(length);
            return Ok(PathBuf::from(OsString::from_wide(&buffer)));
        }

        let required = length.saturating_add(1);
        if required > MAX_FINAL_PATH_UNITS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "canonical home path exceeds the supported Windows path bound",
            ));
        }
        buffer.resize(required, 0);
    }
}

fn path_is_unc(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _))
    )
}

fn is_win32(source: &WindowsError, code: WIN32_ERROR) -> bool {
    source.code() == HRESULT::from_win32(code.0)
}

fn windows_io_error(source: WindowsError) -> io::Error {
    io::Error::other(source)
}

fn lock_unsupported(
    path: &Path,
    capability: HomeLockCapability,
    source: io::Error,
) -> HomeOpenError {
    HomeOpenError::LockUnsupported {
        path: path.to_path_buf(),
        capability,
        source,
    }
}
