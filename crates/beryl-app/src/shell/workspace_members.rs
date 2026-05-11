use std::{env, error::Error, fmt, path::PathBuf};

use beryl_backend::{
    WorkspacePathError, canonicalize_host_path, canonicalize_wsl_home_path, canonicalize_wsl_path,
    strip_windows_extended_prefix,
};
use beryl_model::{
    conversation::{
        PrimaryWorkspaceMember, WorkspaceConversationState, WorkspaceConversationStateError,
    },
    workspace::{RuntimeMode, WorkspaceId, WorkspaceMemberId},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct WorkspaceMembersState {
    path_prompt_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MemberPickerValidationError {
    HostRejectedWslUnc { path: String },
    WslRequiresUnc { path: String },
    WslDistroMismatch { expected: String, actual: String },
    EmptyWslUncPath { distro_name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NewThreadExecutionTargetError {
    MissingRuntimeSelection,
    BackendRuntimeMismatch { primary: String, active: String },
}

#[derive(Debug)]
pub(super) enum WorkspaceTargetResolutionError {
    MissingHomeDirectory,
    MissingRuntimeSelection,
    InvalidPath(WorkspacePathError),
}

impl WorkspaceMembersState {
    pub(crate) fn path_prompt_active(&self) -> bool {
        self.path_prompt_active
    }

    pub(crate) fn set_path_prompt_active(&mut self, active: bool) {
        self.path_prompt_active = active;
    }
}

impl WorkspaceTargetResolutionError {
    pub(super) fn open_failure_summary(&self) -> &'static str {
        match self {
            Self::MissingHomeDirectory => {
                "Beryl could not resolve the implicit home member for the selected runtime environment."
            }
            Self::MissingRuntimeSelection => {
                "Beryl could not resolve the primary workspace member because the workspace runtime environment is missing."
            }
            Self::InvalidPath(_) => {
                "Beryl could not resolve the primary workspace member into a canonical execution target."
            }
        }
    }
}

impl fmt::Display for WorkspaceTargetResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHomeDirectory => {
                write!(f, "could not determine the current user's home directory")
            }
            Self::MissingRuntimeSelection => {
                write!(f, "the workspace runtime environment is not selected")
            }
            Self::InvalidPath(error) => write!(f, "{error}"),
        }
    }
}

impl Error for WorkspaceTargetResolutionError {}

impl fmt::Display for NewThreadExecutionTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRuntimeSelection => {
                write!(
                    f,
                    "select a runtime environment before starting a new thread"
                )
            }
            Self::BackendRuntimeMismatch { primary, active } => write!(
                f,
                "the current backend is running in {active}, but the primary workspace member is in {primary}; reopen the primary member before starting a new thread"
            ),
        }
    }
}

impl Error for NewThreadExecutionTargetError {}

pub(super) fn validate_primary_execution_target_selection(
    workspace_state: &WorkspaceConversationState,
    execution_target: &WorkspaceId,
) -> Result<(), WorkspaceConversationStateError> {
    let mut next_state = workspace_state.clone();
    next_state.designate_primary_execution_target(execution_target)?;
    Ok(())
}

pub(super) fn apply_primary_execution_target_selection(
    workspace_state: &mut WorkspaceConversationState,
    execution_target: &WorkspaceId,
) -> Result<bool, WorkspaceConversationStateError> {
    workspace_state.designate_primary_execution_target(execution_target)
}

pub(super) fn apply_workspace_member_attachment(
    workspace_state: &mut WorkspaceConversationState,
    execution_target: &WorkspaceId,
) -> Result<bool, WorkspaceConversationStateError> {
    workspace_state.attach_execution_target(execution_target)
}

pub(super) fn apply_workspace_member_primary_selection(
    workspace_state: &mut WorkspaceConversationState,
    member_id: &WorkspaceMemberId,
) -> Result<bool, WorkspaceConversationStateError> {
    workspace_state.set_primary_explicit_member(member_id)
}

pub(super) fn apply_workspace_member_detach(
    workspace_state: &mut WorkspaceConversationState,
    member_id: &WorkspaceMemberId,
) -> Result<bool, WorkspaceConversationStateError> {
    workspace_state.detach_explicit_member(member_id)
}

pub(super) fn resolve_new_thread_execution_target(
    workspace_state: &WorkspaceConversationState,
    active_execution_target: &WorkspaceId,
) -> Result<WorkspaceId, NewThreadExecutionTargetError> {
    let Some(primary_member) = workspace_state.primary_member() else {
        return Err(NewThreadExecutionTargetError::MissingRuntimeSelection);
    };

    let primary_runtime = match primary_member {
        PrimaryWorkspaceMember::Explicit(_) => workspace_state
            .selected_runtime()
            .ok_or(NewThreadExecutionTargetError::MissingRuntimeSelection)?,
        PrimaryWorkspaceMember::ImplicitHome(runtime) => runtime,
    };
    if primary_runtime != active_execution_target.runtime_mode() {
        return Err(NewThreadExecutionTargetError::BackendRuntimeMismatch {
            primary: primary_runtime.display_name().to_string(),
            active: active_execution_target
                .runtime_mode()
                .display_name()
                .to_string(),
        });
    }

    match primary_member {
        PrimaryWorkspaceMember::Explicit(member) => Ok(WorkspaceId::from_parts(
            primary_runtime.clone(),
            member.canonical_path().to_path_buf(),
        )),
        PrimaryWorkspaceMember::ImplicitHome(_) => Ok(active_execution_target.clone()),
    }
}

pub(super) fn validate_host_member_picker_path(
    picked_path: PathBuf,
) -> Result<PathBuf, MemberPickerValidationError> {
    let normalized = strip_windows_extended_prefix(picked_path);
    if parse_wsl_unc_path(&normalized).is_some() {
        return Err(MemberPickerValidationError::HostRejectedWslUnc {
            path: normalized.display().to_string(),
        });
    }

    Ok(normalized)
}

pub(super) fn validate_wsl_member_picker_path(
    distro_name: &str,
    picked_path: PathBuf,
) -> Result<PathBuf, MemberPickerValidationError> {
    let normalized = strip_windows_extended_prefix(picked_path);
    let Some((actual_distro, linux_path)) = parse_wsl_unc_path(&normalized) else {
        return Err(MemberPickerValidationError::WslRequiresUnc {
            path: normalized.display().to_string(),
        });
    };

    if !actual_distro.eq_ignore_ascii_case(distro_name) {
        return Err(MemberPickerValidationError::WslDistroMismatch {
            expected: distro_name.to_string(),
            actual: actual_distro,
        });
    }

    if linux_path.as_os_str().is_empty() {
        return Err(MemberPickerValidationError::EmptyWslUncPath {
            distro_name: distro_name.to_string(),
        });
    }

    Ok(linux_path)
}

pub(super) enum WorkspaceMemberAttachRequest {
    HostPath {
        path: PathBuf,
    },
    WslPath {
        distro_name: String,
        path: PathBuf,
    },
    PickerPath {
        runtime: RuntimeMode,
        picked_path: PathBuf,
    },
}

pub(super) fn resolve_workspace_member_attach_request(
    request: WorkspaceMemberAttachRequest,
) -> Result<WorkspaceId, WorkspaceMemberAttachPathError> {
    match request {
        WorkspaceMemberAttachRequest::HostPath { path } => {
            let host_path = validate_host_member_picker_path(path)?;
            let canonical_path = canonicalize_host_path(host_path.as_path())?;
            Ok(WorkspaceId::host_windows(canonical_path))
        }
        WorkspaceMemberAttachRequest::WslPath { distro_name, path } => {
            let canonical_path = canonicalize_wsl_path(&distro_name, path.as_path())?;
            Ok(WorkspaceId::wsl_linux(distro_name, canonical_path))
        }
        WorkspaceMemberAttachRequest::PickerPath {
            runtime,
            picked_path,
        } => canonicalize_member_picker_path(&runtime, picked_path),
    }
}

pub(super) fn canonicalize_member_picker_path(
    runtime: &RuntimeMode,
    picked_path: PathBuf,
) -> Result<WorkspaceId, WorkspaceMemberAttachPathError> {
    match runtime {
        RuntimeMode::HostWindows => {
            let host_path = validate_host_member_picker_path(picked_path)?;
            let canonical_path = canonicalize_host_path(host_path.as_path())?;
            Ok(WorkspaceId::host_windows(canonical_path))
        }
        RuntimeMode::WslLinux { distro_name } => {
            let wsl_path = validate_wsl_member_picker_path(distro_name, picked_path)?;
            let canonical_path = canonicalize_wsl_path(distro_name, wsl_path.as_path())?;
            Ok(WorkspaceId::wsl_linux(distro_name.clone(), canonical_path))
        }
    }
}

pub(super) fn resolve_primary_execution_target(
    workspace_state: &WorkspaceConversationState,
) -> Result<Option<WorkspaceId>, WorkspaceTargetResolutionError> {
    let Some(primary_member) = workspace_state.primary_member() else {
        return Ok(None);
    };

    match primary_member {
        PrimaryWorkspaceMember::Explicit(member) => {
            let runtime = workspace_state
                .selected_runtime()
                .ok_or(WorkspaceTargetResolutionError::MissingRuntimeSelection)?
                .clone();
            Ok(Some(WorkspaceId::from_parts(
                runtime,
                member.canonical_path().to_path_buf(),
            )))
        }
        PrimaryWorkspaceMember::ImplicitHome(runtime) => {
            let canonical_path = resolve_runtime_home_directory(runtime)?;
            Ok(Some(WorkspaceId::from_parts(
                runtime.clone(),
                canonical_path,
            )))
        }
    }
}

pub(super) fn resolve_runtime_home_directory(
    runtime: &RuntimeMode,
) -> Result<PathBuf, WorkspaceTargetResolutionError> {
    match runtime {
        RuntimeMode::HostWindows => {
            let home = env::var_os("USERPROFILE")
                .or_else(|| env::var_os("HOME"))
                .map(PathBuf::from)
                .ok_or(WorkspaceTargetResolutionError::MissingHomeDirectory)?;
            canonicalize_host_path(home.as_path())
                .map_err(WorkspaceTargetResolutionError::InvalidPath)
        }
        RuntimeMode::WslLinux { distro_name } => canonicalize_wsl_home_path(distro_name)
            .map_err(WorkspaceTargetResolutionError::InvalidPath),
    }
}

fn parse_wsl_unc_path(path: &std::path::Path) -> Option<(String, PathBuf)> {
    let raw = path.to_string_lossy().replace('/', "\\");
    let trimmed = raw
        .strip_prefix(r"\\wsl.localhost\")
        .or_else(|| raw.strip_prefix(r"\\wsl$\"))
        .or_else(|| raw.strip_prefix(r"\\?\UNC\wsl.localhost\"))
        .or_else(|| raw.strip_prefix(r"\\?\UNC\wsl$\"));
    let trimmed = trimmed?;
    let mut parts = trimmed.split('\\').filter(|part| !part.is_empty());
    let distro_name = parts.next()?.to_string();
    let mut linux_path = String::from("/");
    linux_path.push_str(&parts.collect::<Vec<_>>().join("/"));
    Some((distro_name, PathBuf::from(linux_path)))
}

#[derive(Debug)]
pub(super) enum WorkspaceMemberAttachPathError {
    Picker(MemberPickerValidationError),
    Canonicalize(WorkspacePathError),
}

impl fmt::Display for MemberPickerValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostRejectedWslUnc { path } => write!(
                f,
                "host-Windows members must be local host directories, but the selected path is WSL path {path}"
            ),
            Self::WslRequiresUnc { path } => write!(
                f,
                "WSL members must be selected through \\\\wsl.localhost\\<distro>\\..., but the selected path is {path}"
            ),
            Self::WslDistroMismatch { expected, actual } => write!(
                f,
                "selected WSL member is in distro {actual}, but this workspace uses distro {expected}"
            ),
            Self::EmptyWslUncPath { distro_name } => write!(
                f,
                "select a directory inside \\\\wsl.localhost\\{distro_name}; the distro root itself is not a member directory"
            ),
        }
    }
}

impl Error for MemberPickerValidationError {}

impl fmt::Display for WorkspaceMemberAttachPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Picker(error) => write!(f, "{error}"),
            Self::Canonicalize(error) => write!(f, "{error}"),
        }
    }
}

impl Error for WorkspaceMemberAttachPathError {}

impl From<MemberPickerValidationError> for WorkspaceMemberAttachPathError {
    fn from(error: MemberPickerValidationError) -> Self {
        Self::Picker(error)
    }
}

impl From<WorkspacePathError> for WorkspaceMemberAttachPathError {
    fn from(error: WorkspacePathError) -> Self {
        Self::Canonicalize(error)
    }
}
