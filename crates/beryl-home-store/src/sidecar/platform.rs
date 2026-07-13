#[cfg(target_os = "windows")]
mod windows_impl {
    use std::{
        ffi::OsStr,
        fs::{File, OpenOptions},
        io,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        path::Path,
    };

    use windows::{
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, FlushFileBuffers,
                MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
        },
        core::PCWSTR,
    };

    pub(crate) fn create_temporary(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
    }

    pub(crate) fn open_retained(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
    }

    pub(crate) fn rename_durable(source: &Path, target: &Path) -> io::Result<()> {
        let source = wide(source.as_os_str())?;
        let target = wide(target.as_os_str())?;
        unsafe {
            // SAFETY: both buffers are live, null-terminated Windows paths and
            // the call receives no copy or replacement fallback flag.
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(io::Error::other)
    }

    pub(crate) fn flush_directory(path: &Path) -> io::Result<()> {
        let directory = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)?;
        unsafe {
            // SAFETY: `directory` owns a live directory handle opened for the
            // explicit Windows metadata-flush operation.
            FlushFileBuffers(HANDLE(directory.as_raw_handle()))
        }
        .map_err(io::Error::other)
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

    pub(crate) fn create_temporary(path: &Path) -> io::Result<File> {
        File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
    }

    pub(crate) fn open_retained(path: &Path) -> io::Result<File> {
        File::open(path)
    }

    pub(crate) fn rename_durable(source: &Path, target: &Path) -> io::Result<()> {
        fs::rename(source, target)
    }

    pub(crate) fn flush_directory(path: &Path) -> io::Result<()> {
        File::open(path)?.sync_all()
    }
}

pub(crate) use windows_impl::*;
