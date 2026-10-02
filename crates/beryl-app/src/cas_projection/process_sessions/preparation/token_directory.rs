use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeMode, RuntimeNativePath};

#[derive(Clone)]
pub struct RuntimeTokenDirectory {
    host: AdmittedHostPath,
}

impl RuntimeTokenDirectory {
    pub fn from_admitted(host: AdmittedHostPath) -> Self {
        Self { host }
    }

    pub fn host(&self) -> &AdmittedHostPath {
        &self.host
    }

    pub fn runtime_path(&self, mode: &RuntimeMode) -> Option<RuntimeNativePath> {
        let (flavor, path) = match mode {
            RuntimeMode::Host => (self.host.flavor(), self.host.as_str().to_owned()),
            RuntimeMode::Wsl(_) => {
                if self.host.flavor() != PathFlavor::Windows {
                    return None;
                }
                let path = self
                    .host
                    .as_str()
                    .strip_prefix(r"\\?\")
                    .unwrap_or(self.host.as_str());
                let bytes = path.as_bytes();
                if bytes.len() < 3
                    || !bytes[0].is_ascii_alphabetic()
                    || bytes[1] != b':'
                    || !matches!(bytes[2], b'\\' | b'/')
                {
                    return None;
                }
                let mut projected = format!("/mnt/{}", char::from(bytes[0].to_ascii_lowercase()));
                for component in path[3..].split(['\\', '/']).filter(|part| !part.is_empty()) {
                    if matches!(component, "." | "..") {
                        return None;
                    }
                    projected.push('/');
                    projected.push_str(component);
                }
                (PathFlavor::Posix, projected)
            }
        };
        RuntimeNativePath::from_admitted(mode.clone(), flavor, path).ok()
    }
}
