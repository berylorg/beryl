use std::{fs::File, io, path::Path};

#[cfg(target_os = "windows")]
pub(crate) fn replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::PCWSTR,
    };

    fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
        let mut encoded: Vec<u16> = value.encode_wide().collect();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "theme path contains an interior null",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    }
    let source = wide(source.as_os_str())?;
    let target = wide(target.as_os_str())?;
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(io::Error::other)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn replace(source: &Path, target: &Path) -> io::Result<()> {
    std::fs::rename(source, target)
}

pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, FlushFileBuffers,
            },
        };
        let directory = File::options()
            .read(true)
            .write(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)?;
        unsafe { FlushFileBuffers(HANDLE(directory.as_raw_handle())) }.map_err(io::Error::other)
    }
    #[cfg(not(target_os = "windows"))]
    {
        File::open(path)?.sync_all()
    }
}
