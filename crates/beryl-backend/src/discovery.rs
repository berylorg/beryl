use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use beryl_model::workspace::{RuntimeMode, WorkspaceId};
use thiserror::Error;

use crate::{BackendLaunchSpec, ManagedBackendSession, ThreadSummary};

const DEFAULT_WSL_DISCOVERY_CWD: &str = "/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiscoveryReport {
    runtime_mode: RuntimeMode,
    status: RuntimeDiscoveryStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeDiscoveryStatus {
    Available {
        workspaces: Vec<DiscoveredWorkspace>,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredWorkspace {
    workspace: WorkspaceId,
    threads: Vec<DiscoveredWorkspaceThread>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredWorkspaceThread {
    id: String,
    preview: String,
    updated_at: i64,
}

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

pub fn discover_host_runtime(
    launch_cwd: impl Into<PathBuf>,
    timeout: Duration,
) -> RuntimeDiscoveryReport {
    discover_runtime(RuntimeMode::HostWindows, launch_cwd.into(), timeout)
}

pub fn discover_wsl_runtime(
    distro_name: impl Into<String>,
    timeout: Duration,
) -> RuntimeDiscoveryReport {
    let distro_name = distro_name.into();
    discover_runtime(
        RuntimeMode::WslLinux {
            distro_name: distro_name.clone(),
        },
        PathBuf::from(DEFAULT_WSL_DISCOVERY_CWD),
        timeout,
    )
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

impl RuntimeDiscoveryReport {
    pub fn unavailable(runtime_mode: RuntimeMode, reason: impl Into<String>) -> Self {
        Self {
            runtime_mode,
            status: RuntimeDiscoveryStatus::Unavailable {
                reason: reason.into(),
            },
        }
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
    }

    pub fn status(&self) -> &RuntimeDiscoveryStatus {
        &self.status
    }
}

impl DiscoveredWorkspace {
    pub fn workspace(&self) -> &WorkspaceId {
        &self.workspace
    }

    pub fn threads(&self) -> &[DiscoveredWorkspaceThread] {
        &self.threads
    }
}

impl DiscoveredWorkspaceThread {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn preview(&self) -> &str {
        &self.preview
    }

    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

fn discover_runtime(
    runtime_mode: RuntimeMode,
    launch_cwd: PathBuf,
    timeout: Duration,
) -> RuntimeDiscoveryReport {
    let launch_spec = BackendLaunchSpec::managed_stdio(runtime_mode.clone(), launch_cwd);
    let status = match ManagedBackendSession::launch_and_probe(launch_spec, timeout) {
        Ok((mut session, _report)) => match session.list_threads(timeout) {
            Ok(threads) => RuntimeDiscoveryStatus::Available {
                workspaces: group_threads_by_workspace(runtime_mode.clone(), threads),
            },
            Err(error) => RuntimeDiscoveryStatus::Unavailable {
                reason: error.to_string(),
            },
        },
        Err(error) => RuntimeDiscoveryStatus::Unavailable {
            reason: error.to_string(),
        },
    };

    RuntimeDiscoveryReport {
        runtime_mode,
        status,
    }
}

fn group_threads_by_workspace(
    runtime_mode: RuntimeMode,
    mut threads: Vec<ThreadSummary>,
) -> Vec<DiscoveredWorkspace> {
    threads.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut workspaces = Vec::<DiscoveredWorkspace>::new();
    for thread in threads {
        if let Some(existing) = workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace.canonical_path() == thread.cwd)
        {
            existing.threads.push(DiscoveredWorkspaceThread {
                id: thread.id,
                preview: thread.preview,
                updated_at: thread.updated_at,
            });
            continue;
        }

        workspaces.push(DiscoveredWorkspace {
            workspace: WorkspaceId::from_parts(runtime_mode.clone(), thread.cwd),
            threads: vec![DiscoveredWorkspaceThread {
                id: thread.id,
                preview: thread.preview,
                updated_at: thread.updated_at,
            }],
        });
    }

    workspaces
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
