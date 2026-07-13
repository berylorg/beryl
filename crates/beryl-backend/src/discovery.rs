use std::path::PathBuf;

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
