#[cfg(feature = "test-faults")]
use std::{cell::RefCell, io};

#[cfg(feature = "test-faults")]
thread_local! {
    static NEXT_DIRECTORY_SYNC_ERROR: RefCell<Option<io::ErrorKind>> = const { RefCell::new(None) };
}

#[cfg(feature = "test-faults")]
pub(crate) fn fail_next_directory_sync_for_tests(kind: io::ErrorKind) {
    NEXT_DIRECTORY_SYNC_ERROR.with(|next| *next.borrow_mut() = Some(kind));
}

#[cfg(feature = "test-faults")]
fn next_directory_sync_error_for_tests() -> Option<io::Error> {
    NEXT_DIRECTORY_SYNC_ERROR.with(|next| {
        next.borrow_mut()
            .take()
            .map(|kind| io::Error::new(kind, "synthetic sidecar directory synchronization result"))
    })
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::{
        ffi::OsStr,
        fs::{File, OpenOptions},
        io,
        mem::size_of,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        path::Path,
    };

    use windows::{
        Win32::{
            Foundation::{
                ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_INVALID_FUNCTION,
                ERROR_NOT_SUPPORTED, HANDLE, WIN32_ERROR,
            },
            Storage::FileSystem::{
                FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
                FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
                FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK, FileAttributeTagInfo,
                FlushFileBuffers, GetFileInformationByHandleEx, GetFileType,
                MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
        },
        core::{Error as WindowsError, HRESULT, PCWSTR},
    };

    #[derive(Debug)]
    pub(crate) enum OpenObjectError {
        Io(io::Error),
        InvalidLayout,
    }

    pub(crate) struct RetainedDirectory(File);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum RenameOutcome {
        Published,
        Collision,
    }

    pub(crate) fn create_temporary(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
    }

    pub(crate) fn open_directory(path: &Path) -> Result<RetainedDirectory, OpenObjectError> {
        let directory = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .map_err(OpenObjectError::Io)?;
        validate_object(&directory, true)?;
        Ok(RetainedDirectory(directory))
    }

    pub(crate) fn open_final_file(path: &Path) -> Result<File, OpenObjectError> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .map_err(OpenObjectError::Io)?;
        validate_object(&file, false)?;
        Ok(file)
    }

    pub(crate) fn rename_durable(source: &Path, target: &Path) -> io::Result<RenameOutcome> {
        let source = wide(source.as_os_str())?;
        let target = wide(target.as_os_str())?;
        let result = unsafe {
            // SAFETY: both buffers are live, null-terminated Windows paths and
            // the call receives no copy or replacement fallback flag.
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        };
        match result {
            Ok(()) => Ok(RenameOutcome::Published),
            Err(source)
                if is_win32(&source, ERROR_ALREADY_EXISTS)
                    || is_win32(&source, ERROR_FILE_EXISTS) =>
            {
                Ok(RenameOutcome::Collision)
            }
            Err(source) => Err(io::Error::other(source)),
        }
    }

    pub(crate) fn flush_directory(directory: &RetainedDirectory) -> io::Result<()> {
        #[cfg(feature = "test-faults")]
        if let Some(source) = super::next_directory_sync_error_for_tests() {
            return Err(source);
        }
        unsafe {
            // SAFETY: `directory` owns a live directory handle opened for the
            // explicit Windows metadata-flush operation.
            FlushFileBuffers(HANDLE(directory.0.as_raw_handle()))
        }
        .map_err(io::Error::other)
    }

    pub(crate) fn directory_sync_unsupported(source: &io::Error) -> bool {
        source.kind() == io::ErrorKind::Unsupported
            || matches!(
                source.raw_os_error(),
                Some(code)
                    if code == ERROR_INVALID_FUNCTION.0 as i32
                        || code == ERROR_NOT_SUPPORTED.0 as i32
            )
    }

    fn validate_object(file: &File, directory: bool) -> Result<(), OpenObjectError> {
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        unsafe {
            // SAFETY: `file` is live, and `info` is correctly sized and
            // writable for the duration of the attribute query.
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        }
        .map_err(|source| OpenObjectError::Io(io::Error::other(source)))?;

        let attributes = info.FileAttributes;
        let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
        let file_type = unsafe {
            // SAFETY: `file` owns a live handle for the duration of this
            // handle-type query.
            GetFileType(HANDLE(file.as_raw_handle()))
        };
        if file_type != FILE_TYPE_DISK
            || attributes & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DEVICE.0) != 0
            || is_directory != directory
        {
            return Err(OpenObjectError::InvalidLayout);
        }
        Ok(())
    }

    fn is_win32(source: &WindowsError, code: WIN32_ERROR) -> bool {
        source.code() == HRESULT::from_win32(code.0)
    }

    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut encoded: Vec<u16> = value.encode_wide().collect();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sidecar path contains an interior null",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    }
}

#[cfg(not(target_os = "windows"))]
mod windows_impl {
    use std::{fs, fs::File, io, path::Path};

    #[derive(Debug)]
    pub(crate) enum OpenObjectError {
        Io(io::Error),
        InvalidLayout,
    }

    pub(crate) struct RetainedDirectory(File);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum RenameOutcome {
        Published,
        Collision,
    }

    pub(crate) fn create_temporary(path: &Path) -> io::Result<File> {
        File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
    }

    pub(crate) fn open_directory(path: &Path) -> Result<RetainedDirectory, OpenObjectError> {
        let metadata = fs::symlink_metadata(path).map_err(OpenObjectError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(OpenObjectError::InvalidLayout);
        }
        File::open(path)
            .map(RetainedDirectory)
            .map_err(OpenObjectError::Io)
    }

    pub(crate) fn open_final_file(path: &Path) -> Result<File, OpenObjectError> {
        let metadata = fs::symlink_metadata(path).map_err(OpenObjectError::Io)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(OpenObjectError::InvalidLayout);
        }
        File::open(path).map_err(OpenObjectError::Io)
    }

    pub(crate) fn rename_durable(source: &Path, target: &Path) -> io::Result<RenameOutcome> {
        fs::rename(source, target)?;
        Ok(RenameOutcome::Published)
    }

    pub(crate) fn flush_directory(directory: &RetainedDirectory) -> io::Result<()> {
        #[cfg(feature = "test-faults")]
        if let Some(source) = super::next_directory_sync_error_for_tests() {
            return Err(source);
        }
        directory.0.sync_all()
    }

    pub(crate) fn directory_sync_unsupported(source: &io::Error) -> bool {
        source.kind() == io::ErrorKind::Unsupported
            || matches!(source.raw_os_error(), Some(22 | 95))
    }
}

pub(crate) use windows_impl::*;
