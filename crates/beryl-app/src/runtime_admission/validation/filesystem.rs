use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

use beryl_backend::{
    ManagedBackendError, ManagedBackendLaunchCleanup, WslFilesystemObservation,
    WslFilesystemOperation, WslSupervisorArtifact,
};
use beryl_home_store::CommandCancellation;
use beryl_model::{PathFlavor, RuntimeMode};

use super::{
    ValidationError, ValidationIssue, check_running,
    seams::{ValidationCleanup, ValidationFilesystem, ValidationFilesystemPath},
};

pub(super) struct ProductionFilesystem {
    artifact: Option<Arc<WslSupervisorArtifact>>,
}

impl ProductionFilesystem {
    pub(super) fn new(artifact: Option<Arc<WslSupervisorArtifact>>) -> Self {
        Self { artifact }
    }

    fn observe_wsl(
        &self,
        mode: &RuntimeMode,
        operation: WslFilesystemOperation,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        let artifact = self
            .artifact
            .clone()
            .ok_or(ValidationIssue::SupervisorUnavailable)?;
        let mode = mode.clone();
        super::filesystem_worker::observe_with_signals(
            deadline,
            cancellation,
            false,
            move |signal, native_signal| {
                wsl_path(
                    artifact,
                    &mode,
                    operation,
                    deadline,
                    &signal,
                    &native_signal,
                    path_bytes,
                )
            },
        )
    }
}

impl ValidationFilesystem for ProductionFilesystem {
    fn executable(
        &self,
        selected: &Path,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        let text = super::checked_text(selected, path_bytes)?;
        let mode = derive_environment(text)?;
        match &mode {
            RuntimeMode::Host => super::filesystem_worker::observe(
                super::filesystem_worker::HostObservation::Executable(selected.into()),
                deadline,
                cancellation,
                path_bytes,
            ),
            RuntimeMode::Wsl(_) => self.observe_wsl(
                &mode,
                WslFilesystemOperation::Executable(wsl_native_path(text)?),
                deadline,
                cancellation,
                path_bytes,
            ),
        }
    }

    fn directory(
        &self,
        selected: &Path,
        mode: &RuntimeMode,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        match mode {
            RuntimeMode::Host => super::filesystem_worker::observe(
                super::filesystem_worker::HostObservation::Directory(selected.into()),
                deadline,
                cancellation,
                path_bytes,
            ),
            RuntimeMode::Wsl(_) => self.observe_wsl(
                mode,
                WslFilesystemOperation::Directory(wsl_native_path(super::checked_text(
                    selected, path_bytes,
                )?)?),
                deadline,
                cancellation,
                path_bytes,
            ),
        }
    }

    fn home(
        &self,
        mode: &RuntimeMode,
        deadline: Instant,
        cancellation: &CommandCancellation,
        path_bytes: usize,
    ) -> Result<ValidationFilesystemPath, ValidationError> {
        match mode {
            RuntimeMode::Host => super::filesystem_worker::observe(
                super::filesystem_worker::HostObservation::Home,
                deadline,
                cancellation,
                path_bytes,
            ),
            RuntimeMode::Wsl(_) => self.observe_wsl(
                mode,
                WslFilesystemOperation::UserHome,
                deadline,
                cancellation,
                path_bytes,
            ),
        }
    }
}

pub(super) fn host_flavor() -> PathFlavor {
    if cfg!(target_os = "windows") {
        PathFlavor::Windows
    } else {
        PathFlavor::Posix
    }
}

pub(super) fn derive_environment(text: &str) -> Result<RuntimeMode, ValidationIssue> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(ValidationIssue::InvalidPath);
    }
    #[cfg(target_os = "windows")]
    {
        let normalized = normalize_windows_path(text)?;
        if let Some(unc) = normalized.strip_prefix(r"\\") {
            let mut parts = unc.split('\\');
            let server = parts.next().ok_or(ValidationIssue::InvalidPath)?;
            let share = parts.next().ok_or(ValidationIssue::InvalidPath)?;
            if server.is_empty() || share.is_empty() || matches!(server, "." | "?") {
                return Err(ValidationIssue::InvalidPath);
            }
            if server.eq_ignore_ascii_case("wsl.localhost") || server.eq_ignore_ascii_case("wsl$") {
                return RuntimeMode::wsl(share).map_err(|_| ValidationIssue::EnvironmentMismatch);
            }
            return Ok(RuntimeMode::Host);
        }
        let bytes = normalized.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return Ok(RuntimeMode::Host);
        }
        Err(ValidationIssue::InvalidPath)
    }
    #[cfg(not(target_os = "windows"))]
    {
        if text.starts_with('/') {
            Ok(RuntimeMode::Host)
        } else {
            Err(ValidationIssue::InvalidPath)
        }
    }
}

pub(super) fn wsl_native_path(text: &str) -> Result<String, ValidationIssue> {
    let normalized = normalize_windows_path(text)?;
    let unc = normalized
        .strip_prefix(r"\\")
        .ok_or(ValidationIssue::EnvironmentMismatch)?;
    let mut parts = unc.split('\\');
    let server = parts.next().ok_or(ValidationIssue::InvalidPath)?;
    if !server.eq_ignore_ascii_case("wsl.localhost") && !server.eq_ignore_ascii_case("wsl$") {
        return Err(ValidationIssue::EnvironmentMismatch);
    }
    let distro = parts.next().ok_or(ValidationIssue::InvalidPath)?;
    RuntimeMode::wsl(distro).map_err(|_| ValidationIssue::EnvironmentMismatch)?;
    let tail = parts.collect::<Vec<_>>().join("/");
    Ok(format!("/{tail}"))
}

fn normalize_windows_path(text: &str) -> Result<String, ValidationIssue> {
    if text.starts_with(r"\\.\") {
        return Err(ValidationIssue::InvalidPath);
    }
    let text = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(drive) = text.strip_prefix(r"\\?\") {
        let bytes = drive.as_bytes();
        if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
            return Err(ValidationIssue::InvalidPath);
        }
        drive.to_owned()
    } else {
        text.to_owned()
    };
    Ok(text.replace('/', "\\"))
}

pub(super) fn host_path(
    selected: &Path,
    executable: bool,
    deadline: Instant,
    cancellation: &CommandCancellation,
    path_bytes: usize,
) -> Result<ValidationFilesystemPath, ValidationError> {
    check_running(deadline, cancellation)?;
    let unavailable = if executable {
        ValidationIssue::ExecutableUnavailable
    } else {
        ValidationIssue::DirectoryUnavailable
    };
    let canonical = fs::canonicalize(selected).map_err(|_| unavailable)?;
    check_running(deadline, cancellation)?;
    let text = super::checked_text(&canonical, path_bytes)?;
    let text = if cfg!(target_os = "windows") {
        normalize_windows_path(text)?
    } else {
        text.to_owned()
    };
    if derive_environment(&text)? != RuntimeMode::Host {
        return Err(ValidationIssue::EnvironmentMismatch.into());
    }
    let metadata = fs::metadata(&canonical).map_err(|_| unavailable)?;
    check_running(deadline, cancellation)?;
    if executable {
        if !metadata.is_file() {
            return Err(unavailable.into());
        }
        fs::File::open(&canonical).map_err(|_| unavailable)?;
        check_running(deadline, cancellation)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(unavailable.into());
            }
        }
    } else {
        if !metadata.is_dir() {
            return Err(unavailable.into());
        }
        let _directory = fs::read_dir(&canonical).map_err(|_| unavailable)?;
    }
    check_running(deadline, cancellation)?;
    Ok(ValidationFilesystemPath {
        mode: RuntimeMode::Host,
        host_path: text.clone(),
        native_path: text,
    })
}

fn wsl_path(
    artifact: Arc<WslSupervisorArtifact>,
    mode: &RuntimeMode,
    operation: WslFilesystemOperation,
    deadline: Instant,
    cancellation: &CommandCancellation,
    native_cancellation: &AtomicBool,
    path_bytes: usize,
) -> Result<ValidationFilesystemPath, ValidationError> {
    let RuntimeMode::Wsl(distribution) = mode else {
        return Err(ValidationIssue::EnvironmentMismatch.into());
    };
    check_running(deadline, cancellation)?;
    let unavailable = match &operation {
        WslFilesystemOperation::Executable(_) => ValidationIssue::ExecutableUnavailable,
        WslFilesystemOperation::Directory(_) => ValidationIssue::DirectoryUnavailable,
        WslFilesystemOperation::UserHome => ValidationIssue::HomeUnavailable,
    };
    let native_path = WslFilesystemObservation::observe(
        artifact,
        distribution,
        operation,
        deadline.saturating_duration_since(Instant::now()),
        native_cancellation,
    )
    .map_err(|failure| {
        let (error, cleanup) = failure.into_parts();
        let issue = match &error {
            ManagedBackendError::WslObservationCancelled => ValidationIssue::Cancelled,
            ManagedBackendError::WslSupervisionTimeout => ValidationIssue::Timeout,
            _ => unavailable,
        };
        let result = match cleanup {
            Some(owner) => ValidationError::with_cleanup(
                issue,
                Box::new(ObservationCleanup {
                    owner,
                    first_failure: None,
                }),
            ),
            None => ValidationError::new(issue),
        };
        result.with_source(Some(Box::new(error)))
    })?;
    super::checked_text(Path::new(&native_path), path_bytes)?;
    if !native_path.starts_with('/') || native_path.contains('\\') {
        return Err(ValidationIssue::InvalidPath.into());
    }
    check_running(deadline, cancellation)?;
    let host_path = format!(
        r"\\wsl.localhost\{}{}",
        distribution.as_str(),
        native_path.replace('/', "\\")
    );
    super::checked_text(Path::new(&host_path), path_bytes)?;
    Ok(ValidationFilesystemPath {
        mode: mode.clone(),
        host_path,
        native_path,
    })
}

struct ObservationCleanup {
    owner: ManagedBackendLaunchCleanup,
    first_failure: Option<ManagedBackendError>,
}

impl ValidationCleanup for ObservationCleanup {
    fn cleanup(&mut self) -> Result<(), ValidationIssue> {
        self.owner.shutdown().map_err(|error| {
            self.first_failure.get_or_insert(error);
            ValidationIssue::Cleanup
        })
    }
    fn failure(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.first_failure
            .as_ref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
    fn take_cleanup_failure(&mut self) -> Option<Box<dyn std::error::Error + Send>> {
        self.first_failure
            .take()
            .map(|error| Box::new(error) as Box<dyn std::error::Error + Send>)
    }
}

#[cfg(target_os = "windows")]
pub(super) fn host_home(path_bytes: usize) -> Result<PathBuf, ValidationIssue> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Profile, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    let profile = unsafe { SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DEFAULT, None) }
        .map_err(|_| ValidationIssue::HomeUnavailable)?;
    let result = (|| {
        let mut length = 0;
        while length <= path_bytes {
            if unsafe { *profile.0.add(length) } == 0 {
                let text =
                    String::from_utf16(unsafe { std::slice::from_raw_parts(profile.0, length) })
                        .map_err(|_| ValidationIssue::HomeUnavailable)?;
                if text.len() > path_bytes {
                    return Err(ValidationIssue::InvalidPath);
                }
                return Ok(PathBuf::from(text));
            }
            length += 1;
        }
        Err(ValidationIssue::InvalidPath)
    })();
    unsafe { CoTaskMemFree(Some(profile.0.cast())) };
    result
}

#[cfg(not(target_os = "windows"))]
pub(super) fn host_home(path_bytes: usize) -> Result<PathBuf, ValidationIssue> {
    let home = std::env::var_os("HOME").ok_or(ValidationIssue::HomeUnavailable)?;
    let home = PathBuf::from(home);
    super::checked_text(&home, path_bytes)?;
    Ok(home)
}
