use std::{
    fs::File,
    io,
    os::windows::{ffi::OsStrExt, io::FromRawHandle},
    path::Path,
};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_READ,
            GetVolumeInformationByHandleW,
        },
    },
    core::{PCWSTR, w},
};

struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}

pub(super) fn create(path: &Path) -> io::Result<File> {
    let mut descriptor = Descriptor(PSECURITY_DESCRIPTOR::default());
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:P(A;;FA;;;OW)"),
            SDDL_REVISION_1,
            &mut descriptor.0,
            None,
        )
        .map_err(io::Error::other)?;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    let encoded = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            PCWSTR(encoded.as_ptr()),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_READ,
            Some(&attributes),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(io::Error::other)?;
    let file = unsafe { File::from_raw_handle(handle.0) };
    let mut flags = 0;
    let security =
        unsafe { GetVolumeInformationByHandleW(handle, None, None, None, Some(&mut flags), None) }
            .map_err(io::Error::other)
            .and_then(|()| {
                // FILE_PERSISTENT_ACLS certifies enforcement, not just descriptor acceptance.
                if flags & 0x00000008 != 0 {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "token storage does not enforce private file access",
                    ))
                }
            });
    if let Err(error) = security {
        drop(file);
        return match std::fs::remove_file(path) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(io::Error::other(format!(
                "{error}; empty token file cleanup failed: {cleanup}"
            ))),
        };
    }
    Ok(file)
}
