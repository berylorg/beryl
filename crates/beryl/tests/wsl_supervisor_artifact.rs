#[path = "../src/wsl_supervisor_artifact/identity.rs"]
mod identity;

use std::io::Cursor;

fn static_elf() -> Vec<u8> {
    let mut bytes = vec![0; 128];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16..18].copy_from_slice(&2_u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&62_u16.to_le_bytes());
    bytes[32..40].copy_from_slice(&64_u64.to_le_bytes());
    bytes[54..56].copy_from_slice(&56_u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&1_u16.to_le_bytes());
    bytes[64..68].copy_from_slice(&1_u32.to_le_bytes());
    bytes
}

#[test]
fn static_identity_rejects_wrong_machine_interpreter_truncation_and_table_overflow() {
    let bytes = static_elf();
    let digest = identity::digest(&mut Cursor::new(&bytes)).unwrap();
    let mut changed = bytes.clone();
    changed[127] = 1;
    assert_ne!(digest, identity::digest(&mut Cursor::new(changed)).unwrap());
    for (offset, value) in [(18, 3), (64, 3), (4, 1)] {
        let mut invalid = bytes.clone();
        invalid[offset] = value;
        assert!(identity::digest(&mut Cursor::new(invalid)).is_err());
    }
    assert!(identity::digest(&mut Cursor::new(&bytes[..80])).is_err());
    let mut invalid = bytes;
    invalid[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(identity::digest(&mut Cursor::new(invalid)).is_err());
}

#[test]
fn mapping_accepts_only_canonical_drive_backed_release_paths() {
    assert_eq!(
        identity::linux_path(r"\\?\C:\Release\beryl-wsl-supervisor-linux-x86_64"),
        Some("/mnt/c/Release/beryl-wsl-supervisor-linux-x86_64".into())
    );
    for path in [
        r"\\server\share\companion",
        r"\\?\UNC\server\share\companion",
        r"\\.\C:\companion",
        r"C:\release\..\companion",
        r"C:companion",
        r"C:\release\companion:stream",
        r"C:\release/companion",
    ] {
        assert!(
            identity::linux_path(path).is_none(),
            "accepted unsupported path {path}"
        );
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[path = "../src/wsl_supervisor_artifact.rs"]
mod artifact;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn sibling_verification_fails_closed_and_keeps_exact_file_pinned() {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
    };
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("beryl.exe");
    fs::write(&executable, b"desktop fixture").unwrap();
    let path = directory.path().join("beryl-wsl-supervisor-linux-x86_64");
    let bytes = static_elf();
    let digest = identity::digest(&mut Cursor::new(&bytes)).unwrap();
    assert!(artifact::open_sibling(&executable, digest, 1).is_none());
    fs::write(&path, &bytes).unwrap();
    assert!(artifact::open_sibling(&executable, [0; 32], 1).is_none());
    assert!(artifact::open_sibling(&executable, digest, 0).is_none());
    let descriptor = artifact::open_sibling(&executable, digest, 1).unwrap();
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
    let retained = descriptor.clone();
    drop(descriptor);
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    drop(retained);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    assert!(artifact::open_sibling(&executable, digest, 1).is_none());
}
