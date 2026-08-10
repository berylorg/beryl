//! Compile-time build identity and native-window title formatting.

const UNKNOWN_BUILD_ID: &str = "unknown";

pub(crate) enum InjectedCommit<'a> {
    Missing,
    Present(&'a str),
    NonUnicode,
}

pub(crate) const fn normalize_commit(commit: &str) -> Option<[u8; 12]> {
    let bytes = commit.as_bytes();
    if bytes.len() < 12 {
        return None;
    }

    let mut normalized = [0; 12];
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if !byte.is_ascii_hexdigit() {
            return None;
        }

        if index < normalized.len() {
            normalized[index] = byte.to_ascii_lowercase();
        }
        index += 1;
    }

    Some(normalized)
}

pub(crate) fn resolve_build_identity(
    injected_commit: InjectedCommit<'_>,
    discovered_commit: Option<&str>,
    discovered_dirty: bool,
) -> String {
    let (commit, is_local) = match injected_commit {
        InjectedCommit::Present(commit) => (commit, false),
        InjectedCommit::NonUnicode => return UNKNOWN_BUILD_ID.to_string(),
        InjectedCommit::Missing => match discovered_commit {
            Some(commit) => (commit, true),
            None => return UNKNOWN_BUILD_ID.to_string(),
        },
    };

    let Some(normalized) = normalize_commit(commit.trim()) else {
        return UNKNOWN_BUILD_ID.to_string();
    };

    let commit = std::str::from_utf8(&normalized).expect("normalized hexadecimal commit is UTF-8");
    if is_local && discovered_dirty {
        format!("{commit}-dirty")
    } else {
        commit.to_string()
    }
}

#[allow(dead_code)]
pub(crate) fn build_identity() -> &'static str {
    option_env!("BERYL_BUILD_ID").unwrap_or(UNKNOWN_BUILD_ID)
}

#[allow(dead_code)]
pub(crate) fn format_native_window_title(base_title: &str, build_id: &str) -> String {
    format!("{base_title} \u{00b7} {build_id}")
}

#[allow(dead_code)]
pub(crate) fn native_window_title(base_title: &str) -> String {
    format_native_window_title(base_title, build_identity())
}

#[cfg(test)]
mod tests {
    use super::{
        InjectedCommit, format_native_window_title, native_window_title, normalize_commit,
        resolve_build_identity,
    };

    const COMMIT: &str = "A1B2C3D4E5F60718293A4B5C6D7E8F9012345678";

    #[test]
    fn normalizes_full_hex_commits_to_twelve_lowercase_characters() {
        assert_eq!(normalize_commit(COMMIT), Some(*b"a1b2c3d4e5f6"));
    }

    #[test]
    fn rejects_short_or_malformed_commits() {
        assert_eq!(normalize_commit("a1b2c3d4e5f"), None);
        assert_eq!(normalize_commit("a1b2c3d4e5f6-not-a-commit"), None);
    }

    #[test]
    fn injected_commit_takes_precedence_without_a_dirty_suffix() {
        assert_eq!(
            resolve_build_identity(
                InjectedCommit::Present(COMMIT),
                Some("0123456789abcdef"),
                true,
            ),
            "a1b2c3d4e5f6"
        );
    }

    #[test]
    fn locally_discovered_dirty_commit_gets_a_suffix() {
        assert_eq!(
            resolve_build_identity(InjectedCommit::Missing, Some(COMMIT), true),
            "a1b2c3d4e5f6-dirty"
        );
    }

    #[test]
    fn missing_or_malformed_metadata_is_unknown() {
        assert_eq!(
            resolve_build_identity(InjectedCommit::Missing, None, false),
            "unknown"
        );
        assert_eq!(
            resolve_build_identity(InjectedCommit::Present("not-a-commit"), Some(COMMIT), false,),
            "unknown"
        );
        assert_eq!(
            resolve_build_identity(InjectedCommit::Present(""), Some(COMMIT), false),
            "unknown"
        );
        assert_eq!(
            resolve_build_identity(InjectedCommit::NonUnicode, Some(COMMIT), false),
            "unknown"
        );
        assert_eq!(
            resolve_build_identity(InjectedCommit::Missing, Some("bad"), true),
            "unknown"
        );
    }

    #[test]
    fn formats_native_titles_with_an_immutable_suffix() {
        assert_eq!(
            format_native_window_title("Beryl - Workspace", "a1b2c3d4e5f6-dirty"),
            "Beryl - Workspace \u{00b7} a1b2c3d4e5f6-dirty"
        );
    }

    #[test]
    fn native_window_title_uses_the_embedded_build_identity() {
        assert_eq!(
            native_window_title("Beryl Settings"),
            format_native_window_title("Beryl Settings", super::build_identity())
        );
    }
}
