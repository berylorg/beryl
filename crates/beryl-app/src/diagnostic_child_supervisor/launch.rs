use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::DiagnosticChildSupervisorError;

pub(crate) const MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticChildLaunch {
    child_home: PathBuf,
    executable_path: PathBuf,
}

impl DiagnosticChildLaunch {
    pub(crate) fn new(child_home: impl Into<PathBuf>, executable_path: impl Into<PathBuf>) -> Self {
        Self {
            child_home: child_home.into(),
            executable_path: executable_path.into(),
        }
    }

    pub(crate) fn current_executable(child_home: impl Into<PathBuf>) -> Result<Self, io::Error> {
        std::env::current_exe().map(|executable_path| Self::new(child_home, executable_path))
    }

    pub(crate) fn child_home(&self) -> &Path {
        &self.child_home
    }

    pub(crate) fn executable_path(&self) -> &Path {
        &self.executable_path
    }
}

pub(super) fn resolve_executable_path(
    path: &Path,
) -> Result<PathBuf, DiagnosticChildSupervisorError> {
    let path_label = path.display().to_string();
    if path_label.trim().is_empty() {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "must not be empty",
        });
    }
    if path_label.len() > MAX_DIAGNOSTIC_CHILD_EXECUTABLE_PATH_BYTES {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "exceeds the diagnostic child executable path byte limit",
        });
    }
    if !path.is_absolute() {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "must be absolute",
        });
    }

    let metadata = fs::metadata(path).map_err(|source| {
        DiagnosticChildSupervisorError::ExecutablePathAccess {
            path: path.to_path_buf(),
            source,
        }
    })?;
    if metadata.is_dir() {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "must be a file, not a directory",
        });
    }
    if !metadata.is_file() {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "must be a regular file",
        });
    }
    reject_non_executable_file(path, &metadata)?;

    Ok(fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(unix)]
fn reject_non_executable_file(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), DiagnosticChildSupervisorError> {
    use std::os::unix::fs::PermissionsExt;

    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(DiagnosticChildSupervisorError::InvalidExecutablePath {
            path: path.to_path_buf(),
            reason: "must have an executable permission bit",
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn reject_non_executable_file(
    _path: &Path,
    _metadata: &fs::Metadata,
) -> Result<(), DiagnosticChildSupervisorError> {
    Ok(())
}
