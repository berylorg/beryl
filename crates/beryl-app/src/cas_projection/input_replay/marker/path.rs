//! Runtime-path projection for one verified input image.

use std::path::{Component, Path, Prefix};

use beryl_model::RuntimeMode;

use super::error::MarkerReplayError;

pub(super) fn project_runtime_path(
    path: &Path,
    runtime_mode: &RuntimeMode,
) -> Result<Box<str>, MarkerReplayError> {
    match runtime_mode {
        RuntimeMode::Host => path
            .to_str()
            .map(Into::into)
            .ok_or(MarkerReplayError::RuntimePathNotUnicode),
        RuntimeMode::Wsl(_) => project_wsl_path(path),
    }
}

fn project_wsl_path(path: &Path) -> Result<Box<str>, MarkerReplayError> {
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            Prefix::Verbatim(_)
            | Prefix::UNC(_, _)
            | Prefix::VerbatimUNC(_, _)
            | Prefix::DeviceNS(_) => return Err(MarkerReplayError::RuntimePathUnmappable),
        },
        Some(Component::Normal(component)) if component.to_str().is_none() => {
            return Err(MarkerReplayError::RuntimePathNotUnicode);
        }
        _ => return Err(MarkerReplayError::RuntimePathUnmappable),
    };
    if !drive.is_ascii_alphabetic() || components.next() != Some(Component::RootDir) {
        return Err(MarkerReplayError::RuntimePathUnmappable);
    }

    let mut projected = String::from("/mnt/");
    projected.push(char::from(drive.to_ascii_lowercase()));
    for component in components {
        let Component::Normal(component) = component else {
            return Err(MarkerReplayError::RuntimePathUnmappable);
        };
        let component = component
            .to_str()
            .ok_or(MarkerReplayError::RuntimePathNotUnicode)?;
        projected.push('/');
        projected.push_str(component);
    }
    Ok(projected.into_boxed_str())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/ordinary_input_marker_path.rs"
    ));
}
