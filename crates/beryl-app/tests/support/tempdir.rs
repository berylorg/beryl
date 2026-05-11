use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

pub struct TestTempDir {
    inner: tempfile::TempDir,
}

impl TestTempDir {
    pub fn path(&self) -> &Path {
        self.inner.path()
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path().join(path)
    }

    pub fn close(self) -> io::Result<()> {
        self.inner.close()
    }
}

impl AsRef<Path> for TestTempDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TestTempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl From<&TestTempDir> for PathBuf {
    fn from(value: &TestTempDir) -> Self {
        value.path().to_path_buf()
    }
}

pub fn temp_dir(prefix: impl AsRef<str>) -> TestTempDir {
    TestTempDir {
        inner: tempfile::Builder::new()
            .prefix(prefix.as_ref())
            .tempdir()
            .expect("temporary directory should be created"),
    }
}

#[allow(dead_code)]
pub fn temp_leaf(prefix: impl AsRef<str>) -> String {
    let temp_dir = temp_dir(prefix);
    let leaf = temp_dir
        .path()
        .file_name()
        .expect("temporary directory should have a leaf name")
        .to_string_lossy()
        .into_owned();
    temp_dir.close().unwrap();
    leaf
}

#[cfg(windows)]
#[allow(dead_code)]
pub fn lock_file_against_replacement(path: &Path) -> io::Result<std::fs::File> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    const FILE_SHARE_READ: u32 = 0x00000001;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}
