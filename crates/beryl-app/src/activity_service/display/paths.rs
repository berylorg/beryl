use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum WindowsRoot {
    Relative,
    Drive,
    Unc,
    Unsupported,
}

fn windows_root(path: &str) -> WindowsRoot {
    if path.starts_with("//?/") || path.starts_with("//./") || path.starts_with("///") {
        WindowsRoot::Unsupported
    } else if let Some(tail) = path.strip_prefix("//") {
        let mut parts = tail.split('/');
        if parts
            .next()
            .is_some_and(|part| !part.is_empty() && !part.contains(':'))
            && parts
                .next()
                .is_some_and(|part| !part.is_empty() && !part.contains(':'))
        {
            WindowsRoot::Unc
        } else {
            WindowsRoot::Unsupported
        }
    } else if path.starts_with('/') {
        WindowsRoot::Unsupported
    } else if path.as_bytes().get(1) == Some(&b':') {
        if path.as_bytes()[0].is_ascii_alphabetic() && path.as_bytes().get(2) == Some(&b'/') {
            WindowsRoot::Drive
        } else {
            WindowsRoot::Unsupported
        }
    } else if path.contains(':') {
        WindowsRoot::Unsupported
    } else {
        WindowsRoot::Relative
    }
}

pub(super) fn relative_path(path: &str, execution: Option<&ExecutionBinding>) -> Option<String> {
    if path.is_empty() || path.chars().any(char::is_control) || path.trim() != path {
        return None;
    }
    let windows =
        execution.is_some_and(|binding| binding.root_path().flavor() == PathFlavor::Windows);
    let normalize = |value: &str| {
        if windows {
            value.replace('\\', "/")
        } else {
            value.to_owned()
        }
    };
    let path = normalize(path);
    let path_root = windows.then(|| windows_root(&path));
    if path_root == Some(WindowsRoot::Unsupported) {
        return None;
    }
    let components = |value: &str| -> Option<Vec<String>> {
        let mut parts = Vec::new();
        for part in value.split('/') {
            if part == ".." || (windows && (part.ends_with(' ') || part.ends_with('.'))) {
                return None;
            }
            if !part.is_empty() && part != "." {
                parts.push(part.to_owned());
            }
        }
        Some(parts)
    };
    let absolute =
        path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') || path.starts_with('\\');
    if !absolute {
        if path.contains(':') || path.contains('\\') {
            return None;
        }
        let parts = components(&path)?;
        return (!parts.is_empty()).then(|| parts.join("/"));
    }
    let binding = execution?;
    let root = normalize(binding.root_path().as_str());
    if windows {
        let root_class = windows_root(&root);
        if !matches!(root_class, WindowsRoot::Drive | WindowsRoot::Unc)
            || path_root != Some(root_class)
        {
            return None;
        }
    }
    if path.starts_with('/') != root.starts_with('/') {
        return None;
    }
    let path_parts = components(&path)?;
    let root_parts = components(&root)?;
    if path_parts.len() <= root_parts.len()
        || !path_parts.iter().zip(&root_parts).all(|(path, root)| {
            if windows {
                path.eq_ignore_ascii_case(root)
            } else {
                path == root
            }
        })
    {
        return None;
    }
    let relative = &path_parts[root_parts.len()..];
    if relative
        .iter()
        .any(|part| part.contains(':') || part.contains('\\'))
    {
        return None;
    }
    Some(relative.join("/"))
}
