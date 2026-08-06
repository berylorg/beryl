use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use super::{AcceptanceSessionError, MAX_ACCEPTANCE_RUN_ID_BYTES};

pub(super) fn validate_duration(
    label: &str,
    value: Duration,
    maximum: Duration,
) -> Result<(), AcceptanceSessionError> {
    if value.is_zero() || value > maximum {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "{label} must be nonzero and at most {maximum:?}"
        )));
    }
    if label == "cleanup timeout" && value < Duration::from_millis(3) {
        return Err(AcceptanceSessionError::InvalidConfiguration(
            "cleanup timeout must be at least 3ms so graceful and termination phases are nonzero"
                .to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_count(
    label: &str,
    value: usize,
    maximum: usize,
) -> Result<(), AcceptanceSessionError> {
    if value == 0 || value > maximum {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "{label} limit must be nonzero and at most {maximum}"
        )));
    }
    Ok(())
}

pub(super) fn validate_run_identity(value: &str) -> Result<(), AcceptanceSessionError> {
    if value.is_empty()
        || value.len() > MAX_ACCEPTANCE_RUN_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "run identity must contain 1..={MAX_ACCEPTANCE_RUN_ID_BYTES} ASCII letters, digits, '.', '-', or '_'"
        )));
    }
    Ok(())
}

pub(super) fn validate_absolute_path(
    label: &str,
    path: &Path,
    maximum_bytes: usize,
) -> Result<(), AcceptanceSessionError> {
    let display = path.display().to_string();
    if display.trim().is_empty() || !path.is_absolute() || display.len() > maximum_bytes {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "{label} path must be nonempty, absolute, and at most {maximum_bytes} bytes"
        )));
    }
    Ok(())
}

pub(super) fn require_file(
    path: &Path,
    action: &'static str,
) -> Result<(), AcceptanceSessionError> {
    let metadata = fs::metadata(path).map_err(|source| AcceptanceSessionError::PathIo {
        action,
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "{} must be an existing regular file",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn require_directory(
    path: &Path,
    action: &'static str,
) -> Result<(), AcceptanceSessionError> {
    let metadata = fs::metadata(path).map_err(|source| AcceptanceSessionError::PathIo {
        action,
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(AcceptanceSessionError::InvalidConfiguration(format!(
            "{} must be an existing directory",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn paths_overlap(left: &Path, right: &Path) -> bool {
    let left = comparable_path(left);
    let right = comparable_path(right);
    left.starts_with(&right) || right.starts_with(&left)
}

fn comparable_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let resolved = path
        .ancestors()
        .find_map(|ancestor| {
            fs::canonicalize(ancestor).ok().map(|canonical_ancestor| {
                path.strip_prefix(ancestor)
                    .map(|suffix| canonical_ancestor.join(suffix))
                    .unwrap_or(canonical_ancestor)
            })
        })
        .unwrap_or_else(|| path.to_path_buf());
    let mut normalized = PathBuf::new();
    for component in resolved.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    comparable_case(normalized)
}

#[cfg(target_os = "windows")]
fn comparable_case(path: PathBuf) -> PathBuf {
    PathBuf::from(path.display().to_string().to_ascii_lowercase())
}

#[cfg(not(target_os = "windows"))]
fn comparable_case(path: PathBuf) -> PathBuf {
    path
}
