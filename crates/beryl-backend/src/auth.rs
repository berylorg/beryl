use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use beryl_model::workspace::RuntimeMode;
use tracing::warn;

use crate::ManagedBackendError;

const TOKEN_BYTES: usize = 32;
const NONCE_BYTES: usize = 16;
const TOKEN_DIR_NAME: &str = "beryl-codex-app-server";
const WSL_TOKEN_DIR: &str = "/tmp/beryl-codex-app-server";

pub struct ManagedBackendAuthMaterial {
    token: String,
    host_token_file_path: PathBuf,
    backend_token_file_path: PathBuf,
    cleaned_up: bool,
}

impl ManagedBackendAuthMaterial {
    pub fn generate(runtime_mode: &RuntimeMode) -> Result<Self, ManagedBackendError> {
        let token = random_hex(TOKEN_BYTES)?;
        let file_name = format!("token-{}.txt", random_hex(NONCE_BYTES)?);
        let (host_token_file_path, backend_token_file_path) = token_paths(runtime_mode, &file_name);

        if let Some(parent) = host_token_file_path.parent() {
            fs::create_dir_all(parent).map_err(|source| {
                ManagedBackendError::CreateWebSocketTokenFile {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&host_token_file_path)
            .map_err(|source| ManagedBackendError::CreateWebSocketTokenFile {
                path: host_token_file_path.clone(),
                source,
            })?;
        file.write_all(token.as_bytes()).map_err(|source| {
            ManagedBackendError::WriteWebSocketTokenFile {
                path: host_token_file_path.clone(),
                source,
            }
        })?;
        file.flush()
            .map_err(|source| ManagedBackendError::WriteWebSocketTokenFile {
                path: host_token_file_path.clone(),
                source,
            })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&host_token_file_path, fs::Permissions::from_mode(0o600));
        }

        Ok(Self {
            token,
            host_token_file_path,
            backend_token_file_path,
            cleaned_up: false,
        })
    }

    pub fn host_token_file_path(&self) -> &Path {
        &self.host_token_file_path
    }

    pub fn backend_token_file_path(&self) -> &Path {
        &self.backend_token_file_path
    }

    pub fn authorization_header_value(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn cleanup(&mut self) -> Result<(), ManagedBackendError> {
        if self.cleaned_up {
            return Ok(());
        }

        match fs::remove_file(&self.host_token_file_path) {
            Ok(()) => {
                self.cleaned_up = true;
                Ok(())
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                self.cleaned_up = true;
                Ok(())
            }
            Err(source) => Err(ManagedBackendError::CleanUpWebSocketTokenFile {
                path: self.host_token_file_path.clone(),
                source,
            }),
        }
    }
}

impl fmt::Debug for ManagedBackendAuthMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedBackendAuthMaterial")
            .field("token", &"<redacted>")
            .field("host_token_file_path", &self.host_token_file_path)
            .field("backend_token_file_path", &self.backend_token_file_path)
            .field("cleaned_up", &self.cleaned_up)
            .finish()
    }
}

impl Drop for ManagedBackendAuthMaterial {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            warn!(%error, "failed to clean up managed backend auth material");
        }
    }
}

fn token_paths(runtime_mode: &RuntimeMode, file_name: &str) -> (PathBuf, PathBuf) {
    match runtime_mode {
        RuntimeMode::HostWindows => {
            let path = std::env::temp_dir().join(TOKEN_DIR_NAME).join(file_name);
            (path.clone(), path)
        }
        RuntimeMode::WslLinux { distro_name } => {
            let backend_path = format!("{WSL_TOKEN_DIR}/{file_name}");
            let host_path = PathBuf::from(format!(
                r"\\wsl.localhost\{distro_name}\tmp\beryl-codex-app-server\{file_name}"
            ));
            (host_path, PathBuf::from(backend_path))
        }
    }
}

fn random_hex(byte_count: usize) -> Result<String, ManagedBackendError> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes)
        .map_err(|source| ManagedBackendError::GenerateWebSocketToken { source })?;

    Ok(hex::encode(bytes))
}
