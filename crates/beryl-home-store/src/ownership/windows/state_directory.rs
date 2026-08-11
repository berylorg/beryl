use std::{
    fs::{self, File, OpenOptions},
    io,
    mem::size_of,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};

use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FileAttributeTagInfo, FileIdInfo, FlushFileBuffers, GetFileInformationByHandleEx,
        GetFileType, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_ID_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StateDirectoryIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

pub(super) struct RetainedStateDirectory {
    _handle: File,
    identity: StateDirectoryIdentity,
}

impl RetainedStateDirectory {
    pub(super) fn open_or_create(path: &Path, home: &File) -> io::Result<Self> {
        let directory = match Self::open(path) {
            Ok(directory) => directory,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                match fs::create_dir(path) {
                    Ok(()) => {}
                    Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(source) => return Err(source),
                }
                Self::open(path)?
            }
            Err(source) => return Err(source),
        };
        flush_directory(home)?;
        Ok(directory)
    }

    pub(super) fn require_same(&self, path: &Path) -> io::Result<()> {
        let candidate = Self::open(path)?;
        if candidate.identity != self.identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the retained Beryl state directory identity changed",
            ));
        }
        Ok(())
    }

    fn open(path: &Path) -> io::Result<Self> {
        let handle = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        validate_directory(&handle)?;
        let identity = query_identity(&handle)?;
        Ok(Self {
            _handle: handle,
            identity,
        })
    }
}

fn validate_directory(handle: &File) -> io::Result<()> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    unsafe {
        // SAFETY: `handle` is live, and `info` is correctly sized and
        // writable for the duration of the attribute query.
        GetFileInformationByHandleEx(
            HANDLE(handle.as_raw_handle()),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    }
    .map_err(io::Error::other)?;
    let file_type = unsafe {
        // SAFETY: `handle` owns a live handle for the duration of this
        // handle-type query.
        GetFileType(HANDLE(handle.as_raw_handle()))
    };
    if file_type != FILE_TYPE_DISK
        || info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0
        || info.FileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DEVICE.0) != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the reserved Beryl state path is not an ordinary directory",
        ));
    }
    Ok(())
}

fn query_identity(handle: &File) -> io::Result<StateDirectoryIdentity> {
    let mut info = FILE_ID_INFO::default();
    unsafe {
        // SAFETY: `handle` is live, and `info` is correctly sized for
        // `FileIdInfo` and remains writable for the duration of the call.
        GetFileInformationByHandleEx(
            HANDLE(handle.as_raw_handle()),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(io::Error::other)?;
    Ok(StateDirectoryIdentity {
        volume_serial_number: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

fn flush_directory(directory: &File) -> io::Result<()> {
    unsafe {
        // SAFETY: the retained home handle is live and was opened for the
        // metadata durability operation before any state-directory creation.
        FlushFileBuffers(HANDLE(directory.as_raw_handle()))
    }
    .map_err(io::Error::other)
}
