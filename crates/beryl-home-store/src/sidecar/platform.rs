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
        core::{Error as WindowsError, HRESULT, PCWSTR},
        Win32::{
            Foundation::{ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, HANDLE, WIN32_ERROR},
            Storage::FileSystem::{
                FileAttributeTagInfo, FileIdInfo, FlushFileBuffers, GetFileInformationByHandleEx,
                GetFileType, MoveFileExW, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY,
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE,
                FILE_TYPE_DISK, MOVEFILE_WRITE_THROUGH,
            },
        },
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct FileIdentity {
        volume_serial_number: u64,
        file_id: [u8; 16],
    }

    #[derive(Debug)]
    pub(crate) enum OpenObjectError {
        Io(io::Error),
        InvalidLayout,
    }

    pub(crate) struct RetainedDirectory(File);

    pub(crate) struct RetainedFile {
        pub(crate) file: File,
        pub(crate) identity: FileIdentity,
    }

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

    pub(crate) fn open_retained_file(path: &Path) -> Result<RetainedFile, OpenObjectError> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .map_err(OpenObjectError::Io)?;
        validate_object(&file, false)?;
        let identity = file_identity(&file).map_err(OpenObjectError::Io)?;
        Ok(RetainedFile { file, identity })
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
        unsafe {
            // SAFETY: `directory` owns a live directory handle opened for the
            // explicit Windows metadata-flush operation.
            FlushFileBuffers(HANDLE(directory.0.as_raw_handle()))
        }
        .map_err(io::Error::other)
    }

    pub(crate) fn file_identity(file: &File) -> io::Result<FileIdentity> {
        let mut info = FILE_ID_INFO::default();
        unsafe {
            // SAFETY: `file` is live, and `info` is correctly sized and
            // writable for the duration of the `FileIdInfo` query.
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                size_of::<FILE_ID_INFO>() as u32,
            )
        }
        .map_err(io::Error::other)?;
        Ok(FileIdentity {
            volume_serial_number: info.VolumeSerialNumber,
            file_id: info.FileId.Identifier,
        })
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

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) struct FileIdentity;

    #[derive(Debug)]
    pub(crate) enum OpenObjectError {
        Io(io::Error),
        InvalidLayout,
    }

    pub(crate) struct RetainedDirectory(File);

    pub(crate) struct RetainedFile {
        pub(crate) file: File,
        pub(crate) identity: FileIdentity,
    }

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

    pub(crate) fn open_retained_file(path: &Path) -> Result<RetainedFile, OpenObjectError> {
        let metadata = fs::symlink_metadata(path).map_err(OpenObjectError::Io)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(OpenObjectError::InvalidLayout);
        }
        File::open(path)
            .map(|file| RetainedFile {
                file,
                identity: FileIdentity,
            })
            .map_err(OpenObjectError::Io)
    }

    pub(crate) fn rename_durable(source: &Path, target: &Path) -> io::Result<RenameOutcome> {
        fs::rename(source, target)?;
        Ok(RenameOutcome::Published)
    }

    pub(crate) fn flush_directory(directory: &RetainedDirectory) -> io::Result<()> {
        directory.0.sync_all()
    }

    pub(crate) fn file_identity(_file: &File) -> io::Result<FileIdentity> {
        Ok(FileIdentity)
    }
}

pub(crate) use windows_impl::*;
