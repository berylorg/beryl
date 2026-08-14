use std::path::Path;

use beryl_model::workspace::{RuntimeMode, WorkspaceId};

pub(super) enum RuntimePathResolution {
    Allowed { backend_path: String },
    PathNotAllowed,
    RenderNotSupported,
}

pub(super) fn resolve_markdown_runtime_path(
    destination: &str,
    execution_target: &WorkspaceId,
) -> RuntimePathResolution {
    let destination = destination.trim();
    if destination.is_empty() || is_remote_or_data_url(destination) {
        return RuntimePathResolution::RenderNotSupported;
    }

    match execution_target.runtime_mode() {
        RuntimeMode::HostWindows => {
            resolve_host_windows_path(destination, execution_target.canonical_path())
        }
        RuntimeMode::WslLinux { .. } => {
            resolve_wsl_linux_path(destination, execution_target.canonical_path())
        }
    }
}

fn resolve_host_windows_path(destination: &str, root: &Path) -> RuntimePathResolution {
    if is_uri_scheme_without_host_drive(destination) || is_windows_drive_relative(destination) {
        return RuntimePathResolution::RenderNotSupported;
    }
    if is_host_root_relative_path(destination) {
        return RuntimePathResolution::PathNotAllowed;
    }

    let Some(root) = normalize_host_absolute_path(&root.display().to_string()) else {
        return RuntimePathResolution::PathNotAllowed;
    };
    let resolved = if is_host_absolute_path(destination) {
        normalize_host_absolute_path(destination)
    } else {
        normalize_host_relative_path(root.as_str(), destination)
    };
    let Some(resolved) = resolved else {
        return RuntimePathResolution::PathNotAllowed;
    };

    if host_path_is_under_root(resolved.as_str(), root.as_str()) {
        RuntimePathResolution::Allowed {
            backend_path: resolved,
        }
    } else {
        RuntimePathResolution::PathNotAllowed
    }
}

fn resolve_wsl_linux_path(destination: &str, root: &Path) -> RuntimePathResolution {
    if is_remote_or_data_url(destination)
        || is_windows_drive_path(destination)
        || is_uri_scheme_without_host_drive(destination)
    {
        return RuntimePathResolution::RenderNotSupported;
    }

    let Some(root) = normalize_posix_absolute_path(&root.display().to_string().replace('\\', "/"))
    else {
        return RuntimePathResolution::PathNotAllowed;
    };
    let destination = destination.replace('\\', "/");
    let resolved = if destination.starts_with('/') {
        normalize_posix_absolute_path(destination.as_str())
    } else {
        normalize_posix_absolute_path(format!("{root}/{destination}").as_str())
    };
    let Some(resolved) = resolved else {
        return RuntimePathResolution::PathNotAllowed;
    };

    if posix_path_is_under_root(resolved.as_str(), root.as_str()) {
        RuntimePathResolution::Allowed {
            backend_path: resolved,
        }
    } else {
        RuntimePathResolution::PathNotAllowed
    }
}

fn normalize_host_relative_path(root: &str, relative: &str) -> Option<String> {
    if relative.contains(':')
        || is_host_absolute_path(relative)
        || is_host_root_relative_path(relative)
    {
        return None;
    }
    normalize_host_absolute_path(format!("{root}\\{relative}").as_str())
}

fn normalize_host_absolute_path(path: &str) -> Option<String> {
    let normalized = path.replace('/', "\\");
    let (prefix, tail) = host_absolute_prefix_and_tail(normalized.as_str())?;
    let segments = normalize_segments(tail.split('\\'))?;
    if segments.is_empty() {
        Some(prefix)
    } else {
        Some(format!("{prefix}\\{}", segments.join("\\")))
    }
}

fn host_absolute_prefix_and_tail(path: &str) -> Option<(String, &str)> {
    let bytes = path.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return Some((path[..2].to_ascii_lowercase(), &path[3..]));
    }

    if let Some(rest) = path.strip_prefix("\\\\") {
        let mut parts = rest.splitn(3, '\\');
        let server = parts.next()?.trim();
        let share = parts.next()?.trim();
        if server.is_empty() || share.is_empty() {
            return None;
        }
        let tail = parts.next().unwrap_or("");
        return Some((format!("\\\\{server}\\{share}").to_ascii_lowercase(), tail));
    }

    None
}

fn normalize_posix_absolute_path(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let segments = normalize_segments(path.split('/'))?;
    if segments.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", segments.join("/")))
    }
}

fn normalize_segments<'a>(segments: impl Iterator<Item = &'a str>) -> Option<Vec<String>> {
    let mut normalized = Vec::new();
    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => {
                normalized.pop()?;
            }
            segment => normalized.push(segment.to_string()),
        }
    }
    Some(normalized)
}

fn host_path_is_under_root(path: &str, root: &str) -> bool {
    let path = trim_trailing_host_separator(path).to_ascii_lowercase();
    let root = trim_trailing_host_separator(root).to_ascii_lowercase();
    path == root || path.starts_with(&format!("{root}\\"))
}

fn posix_path_is_under_root(path: &str, root: &str) -> bool {
    let path = trim_trailing_posix_separator(path);
    let root = trim_trailing_posix_separator(root);
    path == root || path.starts_with(&format!("{root}/"))
}

fn trim_trailing_host_separator(path: &str) -> &str {
    path.trim_end_matches('\\')
}

fn trim_trailing_posix_separator(path: &str) -> &str {
    if path == "/" {
        path
    } else {
        path.trim_end_matches('/')
    }
}

fn is_remote_or_data_url(destination: &str) -> bool {
    let lower = destination.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("data:")
        || lower.starts_with("file://")
}

fn is_uri_scheme_without_host_drive(destination: &str) -> bool {
    let Some(colon) = destination.find(':') else {
        return false;
    };
    if colon == 1 && destination.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let scheme = &destination[..colon];
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || index > 0 && (byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
        })
}

fn is_host_absolute_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    let bytes = normalized.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
        || normalized.starts_with("\\\\")
}

fn is_host_root_relative_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    normalized.starts_with('\\') && !normalized.starts_with("\\\\")
}

fn is_windows_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_windows_drive_relative(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    let bytes = normalized.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && !matches!(bytes.get(2), Some(b'\\'))
}
