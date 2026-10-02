#![cfg(target_os = "windows")]

#[path = "../src/auth/private_file.rs"]
mod private_file;

use std::{io::Write, os::windows::ffi::OsStrExt};
use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{
                ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
                SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        },
    },
    core::{PCWSTR, PWSTR},
};

#[test]
fn token_file_is_owner_only_before_writing_and_never_replaces_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("token.txt");
    let mut file = private_file::create(&path).unwrap();
    let name = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let mut text = PWSTR::null();
    unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(name.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut descriptor,
        )
        .ok()
        .unwrap();
        let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        );
        LocalFree(Some(HLOCAL(descriptor.0)));
        converted.unwrap();
        let acl = text.to_string().unwrap();
        LocalFree(Some(HLOCAL(text.0.cast())));
        assert_eq!(acl, "D:P(A;;FA;;;OW)");
    }
    file.write_all(b"test-only").unwrap();
    drop(file);
    assert!(private_file::create(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"test-only");
}
