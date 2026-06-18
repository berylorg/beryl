use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeDiscoveryError {
    #[error("failed to launch wsl.exe while listing distros")]
    ListWslDistros {
        #[source]
        source: io::Error,
    },
    #[error("wsl.exe returned non-text distro output")]
    InvalidWslListEncoding,
}

#[derive(Debug, Error)]
pub enum WorkspacePathError {
    #[error("failed to canonicalize host path {path}")]
    CanonicalizeHostPath {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to launch wsl.exe for distro {distro_name}")]
    LaunchWslCanonicalization {
        distro_name: String,
        #[source]
        source: io::Error,
    },
    #[error("wsl.exe failed to canonicalize {path} in distro {distro_name}: {detail}")]
    WslCanonicalizationFailed {
        distro_name: String,
        path: String,
        detail: String,
    },
    #[error("wsl.exe returned non-UTF-8 canonical path output for distro {distro_name}")]
    InvalidWslCanonicalPathEncoding { distro_name: String },
    #[error("wsl.exe returned an empty canonical path for {path} in distro {distro_name}")]
    EmptyWslCanonicalPath { distro_name: String, path: String },
}

pub fn list_wsl_distros() -> Result<Vec<String>, RuntimeDiscoveryError> {
    let output = Command::new("wsl.exe")
        .arg("-l")
        .arg("-q")
        .output()
        .map_err(|source| RuntimeDiscoveryError::ListWslDistros { source })?;
    let output = decode_wsl_output(&output.stdout)?;

    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub fn canonicalize_host_path(path: &Path) -> Result<PathBuf, WorkspacePathError> {
    std::fs::canonicalize(path)
        .map(strip_windows_extended_prefix)
        .map_err(|source| WorkspacePathError::CanonicalizeHostPath {
            path: path.display().to_string(),
            source,
        })
}

pub fn canonicalize_wsl_path(
    distro_name: &str,
    path: &Path,
) -> Result<PathBuf, WorkspacePathError> {
    let output = Command::new("wsl.exe")
        .arg("--distribution")
        .arg(distro_name)
        .arg("--cd")
        .arg(path)
        .arg("--exec")
        .arg("pwd")
        .arg("-P")
        .output()
        .map_err(|source| WorkspacePathError::LaunchWslCanonicalization {
            distro_name: distro_name.to_string(),
            source,
        })?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorkspacePathError::WslCanonicalizationFailed {
            distro_name: distro_name.to_string(),
            path: path.display().to_string(),
            detail: if detail.is_empty() {
                "wsl.exe exited unsuccessfully".to_string()
            } else {
                detail
            },
        });
    }

    let canonical = String::from_utf8(output.stdout).map_err(|_| {
        WorkspacePathError::InvalidWslCanonicalPathEncoding {
            distro_name: distro_name.to_string(),
        }
    })?;
    let canonical = canonical.trim();
    if canonical.is_empty() {
        return Err(WorkspacePathError::EmptyWslCanonicalPath {
            distro_name: distro_name.to_string(),
            path: path.display().to_string(),
        });
    }

    Ok(PathBuf::from(canonical))
}

pub fn canonicalize_wsl_home_path(distro_name: &str) -> Result<PathBuf, WorkspacePathError> {
    let output = Command::new("wsl.exe")
        .arg("--distribution")
        .arg(distro_name)
        .arg("--exec")
        .arg("pwd")
        .arg("-P")
        .output()
        .map_err(|source| WorkspacePathError::LaunchWslCanonicalization {
            distro_name: distro_name.to_string(),
            source,
        })?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorkspacePathError::WslCanonicalizationFailed {
            distro_name: distro_name.to_string(),
            path: "~".to_string(),
            detail: if detail.is_empty() {
                "wsl.exe exited unsuccessfully".to_string()
            } else {
                detail
            },
        });
    }

    let canonical = String::from_utf8(output.stdout).map_err(|_| {
        WorkspacePathError::InvalidWslCanonicalPathEncoding {
            distro_name: distro_name.to_string(),
        }
    })?;
    let canonical = canonical.trim();
    if canonical.is_empty() {
        return Err(WorkspacePathError::EmptyWslCanonicalPath {
            distro_name: distro_name.to_string(),
            path: "~".to_string(),
        });
    }

    Ok(PathBuf::from(canonical))
}

pub fn strip_windows_extended_prefix(path: PathBuf) -> PathBuf {
    let Some(path_string) = path.to_str() else {
        return path;
    };

    if let Some(stripped) = path_string.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{stripped}"));
    }

    if let Some(stripped) = path_string.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }

    path
}

fn decode_wsl_output(bytes: &[u8]) -> Result<String, RuntimeDiscoveryError> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    if bytes.contains(&0) {
        if bytes.len() % 2 != 0 {
            return Err(RuntimeDiscoveryError::InvalidWslListEncoding);
        }

        let code_units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        return String::from_utf16(&code_units)
            .map_err(|_| RuntimeDiscoveryError::InvalidWslListEncoding);
    }

    String::from_utf8(bytes.to_vec()).map_err(|_| RuntimeDiscoveryError::InvalidWslListEncoding)
}
