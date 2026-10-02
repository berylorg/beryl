use std::{fmt, fs, io::Write, path::PathBuf};

use beryl_model::{AdmittedHostPath, RuntimeMode, RuntimeNativePath};
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::ManagedBackendError;

const TOKEN_BYTES: usize = 32;
const NONCE_BYTES: usize = 16;
#[cfg(target_os = "windows")]
mod private_file;
pub(crate) struct ManagedBackendAuthMaterial {
    token: String,
    token_sha256: String,
    host_token_file_path: PathBuf,
    backend_token_file_path: String,
    cleaned_up: bool,
}

impl ManagedBackendAuthMaterial {
    pub(crate) fn generate(
        host_token_directory: &AdmittedHostPath,
        runtime_token_directory: &RuntimeNativePath,
    ) -> Result<Self, ManagedBackendError> {
        let token = random_hex(TOKEN_BYTES)?;
        let file_name = format!("token-{}.txt", random_hex(NONCE_BYTES)?);
        let host_token_file_path = PathBuf::from(host_token_directory.as_str()).join(&file_name);
        let backend_token_file_path = match runtime_token_directory.mode() {
            RuntimeMode::Host => PathBuf::from(runtime_token_directory.as_str())
                .join(&file_name)
                .display()
                .to_string(),
            RuntimeMode::Wsl(_) => posix_join(runtime_token_directory.as_str(), &file_name),
        };

        if let Some(parent) = host_token_file_path.parent() {
            fs::create_dir_all(parent).map_err(|source| {
                ManagedBackendError::CreateWebSocketTokenFile {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }

        #[cfg(target_os = "windows")]
        let file = private_file::create(&host_token_file_path);
        #[cfg(not(target_os = "windows"))]
        let file = {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(&host_token_file_path)
        };
        let mut file = file.map_err(|source| ManagedBackendError::CreateWebSocketTokenFile {
            path: host_token_file_path.clone(),
            source,
        })?;
        let material = Self {
            token_sha256: hex::encode(Sha256::digest(token.as_bytes())),
            token,
            host_token_file_path,
            backend_token_file_path,
            cleaned_up: false,
        };
        let written = file
            .write_all(material.token.as_bytes())
            .map_err(|source| ManagedBackendError::WriteWebSocketTokenFile {
                path: material.host_token_file_path.clone(),
                source,
            })
            .and_then(|()| {
                file.flush()
                    .map_err(|source| ManagedBackendError::WriteWebSocketTokenFile {
                        path: material.host_token_file_path.clone(),
                        source,
                    })
            });
        drop(file);
        written?;
        Ok(material)
    }

    pub(crate) fn backend_token_file_path(&self) -> &str {
        &self.backend_token_file_path
    }

    pub(crate) fn token_sha256(&self) -> &str {
        &self.token_sha256
    }

    pub(crate) fn authorization_header_value(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub(crate) fn cleanup(&mut self) -> Result<(), ManagedBackendError> {
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

    pub(crate) fn preserve_file_on_drop(&mut self) {
        self.cleaned_up = true;
    }
}

impl fmt::Debug for ManagedBackendAuthMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedBackendAuthMaterial")
            .field("token", &"<redacted>")
            .field("token_sha256", &self.token_sha256)
            .field("host_token_file_path", &self.host_token_file_path)
            .field("backend_token_file_path", &self.backend_token_file_path)
            .field("cleaned_up", &self.cleaned_up)
            .finish()
    }
}

fn posix_join(directory: &str, file_name: &str) -> String {
    if directory == "/" {
        format!("/{file_name}")
    } else {
        format!("{}/{file_name}", directory.trim_end_matches('/'))
    }
}

impl Drop for ManagedBackendAuthMaterial {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            warn!(%error, "failed to clean up managed backend auth material");
        }
    }
}

fn random_hex(byte_count: usize) -> Result<String, ManagedBackendError> {
    let mut bytes = vec![0_u8; byte_count];
    getrandom::fill(&mut bytes)
        .map_err(|source| ManagedBackendError::GenerateWebSocketToken { source })?;

    Ok(hex::encode(bytes))
}
