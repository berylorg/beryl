use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};
use thiserror::Error;

use super::composer_draft::{AcceptedComposerDraft, AcceptedComposerImage};
use crate::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PreparedComposerImage {
    label: String,
    asset_id: String,
    host_path: PathBuf,
    backend_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PreparedComposerDraft {
    draft: AcceptedComposerDraft,
    images: Vec<PreparedComposerImage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RuntimeReadableImagePath {
    host_path: PathBuf,
    backend_path: String,
    validation_path: PathBuf,
}

pub(super) enum ComposerImageDeliveryUpdate {
    Finished(Result<PreparedComposerDraft, String>),
}

#[derive(Debug, Error)]
pub(super) enum ComposerImageDeliveryError {
    #[error("failed to load durable Beryl image assets")]
    LoadAssets {
        #[source]
        source: WorkspacePersistenceError,
    },
    #[error("image {label} does not have a durable Beryl asset id")]
    MissingAssetId { label: String },
    #[error("image {label} refers to missing durable Beryl asset {asset_id}")]
    MissingAsset { label: String, asset_id: String },
    #[error("image {label} asset file is missing at {path}")]
    MissingAssetFile { label: String, path: String },
    #[error("image {label} asset file is corrupt at {path}")]
    CorruptAssetFile { label: String, path: String },
    #[error("image {label} asset path {path} cannot be mapped into WSL distro {distro_name}")]
    WslPathUnmappable {
        label: String,
        path: String,
        distro_name: String,
    },
    #[error("image {label} is not readable by the backend runtime at {backend_path}")]
    RuntimePathUnreadable {
        label: String,
        backend_path: String,
        validation_path: String,
        #[source]
        source: io::Error,
    },
}

pub(super) fn prepare_accepted_composer_images(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    draft: &AcceptedComposerDraft,
    runtime_mode: &RuntimeMode,
) -> Result<Vec<PreparedComposerImage>, ComposerImageDeliveryError> {
    let assets = persistence
        .load_workspace_image_assets(workspace_id)
        .map_err(|source| ComposerImageDeliveryError::LoadAssets { source })?;

    draft
        .images()
        .map(|image| prepare_composer_image(image, &assets, runtime_mode, readable_file))
        .collect()
}

pub(super) fn spawn_composer_image_delivery_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    draft: AcceptedComposerDraft,
    runtime_mode: RuntimeMode,
) -> Receiver<ComposerImageDeliveryUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result =
            prepare_accepted_composer_images(&persistence, &workspace_id, &draft, &runtime_mode)
                .map_err(|error| error.to_string())
                .map(|images| PreparedComposerDraft::new(draft, images))
                .map_err(|error| error.to_string());
        let _ = sender.send(ComposerImageDeliveryUpdate::Finished(result));
    });
    receiver
}

fn prepare_composer_image(
    image: &AcceptedComposerImage,
    assets: &[WorkspaceImageAsset],
    runtime_mode: &RuntimeMode,
    read_probe: impl FnMut(&Path) -> io::Result<()>,
) -> Result<PreparedComposerImage, ComposerImageDeliveryError> {
    let asset_id =
        image
            .data()
            .asset_id()
            .ok_or_else(|| ComposerImageDeliveryError::MissingAssetId {
                label: image.label().to_string(),
            })?;
    let asset = assets
        .iter()
        .find(|asset| asset.id() == asset_id)
        .ok_or_else(|| ComposerImageDeliveryError::MissingAsset {
            label: image.label().to_string(),
            asset_id: asset_id.to_string(),
        })?;
    validate_asset_status(image.label(), asset)?;
    let path = runtime_readable_image_path(runtime_mode, asset.file_path(), read_probe)
        .map_err(|error| error.into_delivery_error(image.label()))?;

    Ok(PreparedComposerImage {
        label: image.label().to_string(),
        asset_id: asset_id.to_string(),
        host_path: path.host_path,
        backend_path: path.backend_path,
    })
}

fn validate_asset_status(
    label: &str,
    asset: &WorkspaceImageAsset,
) -> Result<(), ComposerImageDeliveryError> {
    match asset.status() {
        WorkspaceImageAssetStatus::Available => Ok(()),
        WorkspaceImageAssetStatus::MissingFile => {
            Err(ComposerImageDeliveryError::MissingAssetFile {
                label: label.to_string(),
                path: asset.file_path().display().to_string(),
            })
        }
        WorkspaceImageAssetStatus::CorruptFile => {
            Err(ComposerImageDeliveryError::CorruptAssetFile {
                label: label.to_string(),
                path: asset.file_path().display().to_string(),
            })
        }
    }
}

pub(super) fn runtime_readable_image_path(
    runtime_mode: &RuntimeMode,
    host_path: &Path,
    read_probe: impl FnMut(&Path) -> io::Result<()>,
) -> Result<RuntimeReadableImagePath, RuntimeReadableImagePathError> {
    match runtime_mode {
        RuntimeMode::HostWindows => runtime_readable_host_windows_path(host_path, read_probe),
        RuntimeMode::WslLinux { distro_name } => {
            runtime_readable_wsl_path(distro_name, host_path, read_probe)
        }
    }
}

fn runtime_readable_host_windows_path(
    host_path: &Path,
    mut read_probe: impl FnMut(&Path) -> io::Result<()>,
) -> Result<RuntimeReadableImagePath, RuntimeReadableImagePathError> {
    read_probe(host_path).map_err(|source| RuntimeReadableImagePathError::Unreadable {
        backend_path: host_path.display().to_string(),
        validation_path: host_path.to_path_buf(),
        source,
    })?;
    Ok(RuntimeReadableImagePath {
        host_path: host_path.to_path_buf(),
        backend_path: host_path.display().to_string(),
        validation_path: host_path.to_path_buf(),
    })
}

fn runtime_readable_wsl_path(
    distro_name: &str,
    host_path: &Path,
    mut read_probe: impl FnMut(&Path) -> io::Result<()>,
) -> Result<RuntimeReadableImagePath, RuntimeReadableImagePathError> {
    let Some(mapped) = map_host_drive_path_to_wsl(host_path, distro_name) else {
        return Err(RuntimeReadableImagePathError::WslPathUnmappable {
            path: host_path.display().to_string(),
            distro_name: distro_name.to_string(),
        });
    };
    read_probe(&mapped.validation_path).map_err(|source| {
        RuntimeReadableImagePathError::Unreadable {
            backend_path: mapped.backend_path.clone(),
            validation_path: mapped.validation_path.clone(),
            source,
        }
    })?;
    Ok(RuntimeReadableImagePath {
        host_path: host_path.to_path_buf(),
        backend_path: mapped.backend_path,
        validation_path: mapped.validation_path,
    })
}

fn readable_file(path: &Path) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "runtime path is not a file",
        ))
    }
}

impl PreparedComposerImage {
    #[allow(dead_code)]
    pub(super) fn label(&self) -> &str {
        &self.label
    }

    #[allow(dead_code)]
    pub(super) fn asset_id(&self) -> &str {
        &self.asset_id
    }

    #[allow(dead_code)]
    pub(super) fn host_path(&self) -> &PathBuf {
        &self.host_path
    }

    #[allow(dead_code)]
    pub(super) fn backend_path(&self) -> &str {
        &self.backend_path
    }
}

impl PreparedComposerDraft {
    pub(super) fn new(draft: AcceptedComposerDraft, images: Vec<PreparedComposerImage>) -> Self {
        Self { draft, images }
    }

    pub(super) fn draft(&self) -> &AcceptedComposerDraft {
        &self.draft
    }

    pub(super) fn backend_path_for_label(&self, label: &str) -> Option<&str> {
        self.image_for_label(label)
            .map(|image| image.backend_path())
    }

    pub(super) fn image_for_label(&self, label: &str) -> Option<&PreparedComposerImage> {
        self.images.iter().find(|image| image.label == label)
    }
}

impl RuntimeReadableImagePath {
    #[allow(dead_code)]
    pub(super) fn host_path(&self) -> &PathBuf {
        &self.host_path
    }

    #[allow(dead_code)]
    pub(super) fn backend_path(&self) -> &str {
        &self.backend_path
    }

    #[allow(dead_code)]
    pub(super) fn validation_path(&self) -> &PathBuf {
        &self.validation_path
    }
}

#[derive(Debug)]
pub(super) enum RuntimeReadableImagePathError {
    WslPathUnmappable {
        path: String,
        distro_name: String,
    },
    Unreadable {
        backend_path: String,
        validation_path: PathBuf,
        source: io::Error,
    },
}

impl RuntimeReadableImagePathError {
    fn into_delivery_error(self, label: &str) -> ComposerImageDeliveryError {
        match self {
            Self::WslPathUnmappable { path, distro_name } => {
                ComposerImageDeliveryError::WslPathUnmappable {
                    label: label.to_string(),
                    path,
                    distro_name,
                }
            }
            Self::Unreadable {
                backend_path,
                validation_path,
                source,
            } => ComposerImageDeliveryError::RuntimePathUnreadable {
                label: label.to_string(),
                backend_path,
                validation_path: validation_path.display().to_string(),
                source,
            },
        }
    }
}

struct WslMappedPath {
    backend_path: String,
    validation_path: PathBuf,
}

fn map_host_drive_path_to_wsl(host_path: &Path, distro_name: &str) -> Option<WslMappedPath> {
    let raw = host_path.to_string_lossy().replace('\\', "/");
    let normalized = raw
        .strip_prefix("//?/")
        .or_else(|| raw.strip_prefix("//./"))
        .unwrap_or(raw.as_str());
    let mut chars = normalized.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() || chars.next()? != ':' || chars.next()? != '/' {
        return None;
    }
    let rest = chars.as_str().trim_start_matches('/');
    if rest.is_empty() {
        return None;
    }

    let drive = drive.to_ascii_lowercase();
    let backend_path = format!("/mnt/{drive}/{rest}");
    let validation_path = PathBuf::from(format!(
        r"\\wsl.localhost\{distro_name}\mnt\{drive}\{}",
        rest.replace('/', "\\")
    ));
    Some(WslMappedPath {
        backend_path,
        validation_path,
    })
}
