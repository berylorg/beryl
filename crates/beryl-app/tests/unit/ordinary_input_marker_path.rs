use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};

use super::*;

#[test]
fn wsl_projection_preserves_components_for_disk_and_verbatim_disk_paths() {
    for path in [
        Path::new(r"C:\Users\Operator\.beryl\sidecars\image.bin"),
        Path::new(r"\\?\C:\Users\Operator\.beryl\sidecars\image.bin"),
    ] {
        assert_eq!(
            project_runtime_path(path, &RuntimeMode::wsl("Ubuntu-24.04").unwrap())
                .unwrap()
                .as_ref(),
            "/mnt/c/Users/Operator/.beryl/sidecars/image.bin"
        );
    }
}

#[test]
fn wsl_projection_rejects_unc_relative_and_non_drive_paths() {
    for path in [
        Path::new(r"\\server\share\image.bin"),
        Path::new(r"relative\image.bin"),
        Path::new(r"\rooted\image.bin"),
    ] {
        assert!(matches!(
            project_runtime_path(path, &RuntimeMode::wsl("Ubuntu-24.04").unwrap()),
            Err(MarkerReplayError::RuntimePathUnmappable)
        ));
    }
}

#[test]
fn runtime_projection_rejects_non_unicode_paths_without_lossy_conversion() {
    let path = PathBuf::from(OsString::from_wide(&[
        u16::from(b'C'),
        u16::from(b':'),
        u16::from(b'\\'),
        0xd800,
    ]));
    for mode in [
        RuntimeMode::host(),
        RuntimeMode::wsl("Ubuntu-24.04").unwrap(),
    ] {
        assert!(matches!(
            project_runtime_path(&path, &mode),
            Err(MarkerReplayError::RuntimePathNotUnicode)
        ));
    }
}
