use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use gpui::ImageFormat;
use serde::{Deserialize, Serialize};

const IMAGE_ASSETS_DIR_NAME: &str = "image-assets";
static NEXT_IMAGE_ASSET_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceImageAssetFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Svg,
    Bmp,
    Tiff,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceImageAssetMetadata {
    id: String,
    relative_path: String,
    format: WorkspaceImageAssetFormat,
    byte_len: u64,
    created_at_millis: u64,
    retained_at_millis: Option<u64>,
    unreferenced_at_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_backend_path: Option<String>,
    width_px: Option<u32>,
    height_px: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceImageAssetStatus {
    Available,
    MissingFile,
    CorruptFile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceImageAsset {
    metadata: WorkspaceImageAssetMetadata,
    file_path: PathBuf,
    status: WorkspaceImageAssetStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspaceImageAssetMetadataRecord {
    assets: Vec<WorkspaceImageAssetMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingWorkspaceImageAsset {
    pub(crate) metadata: WorkspaceImageAssetMetadata,
    file_path: PathBuf,
}

impl WorkspaceImageAssetFormat {
    pub fn from_gpui(format: ImageFormat) -> Self {
        match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Webp => Self::Webp,
            ImageFormat::Gif => Self::Gif,
            ImageFormat::Svg => Self::Svg,
            ImageFormat::Bmp => Self::Bmp,
            ImageFormat::Tiff => Self::Tiff,
        }
    }

    pub fn to_gpui(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Webp => ImageFormat::Webp,
            Self::Gif => ImageFormat::Gif,
            Self::Svg => ImageFormat::Svg,
            Self::Bmp => ImageFormat::Bmp,
            Self::Tiff => ImageFormat::Tiff,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Svg => "svg",
            Self::Bmp => "bmp",
            Self::Tiff => "tif",
        }
    }
}

impl WorkspaceImageAssetMetadata {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn format(&self) -> WorkspaceImageAssetFormat {
        self.format
    }

    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub fn retained_at_millis(&self) -> Option<u64> {
        self.retained_at_millis
    }

    pub fn unreferenced_at_millis(&self) -> Option<u64> {
        self.unreferenced_at_millis
    }

    pub fn dimensions(&self) -> Option<(u32, u32)> {
        self.width_px.zip(self.height_px)
    }

    pub fn source_backend_path(&self) -> Option<&str> {
        self.source_backend_path.as_deref()
    }
}

impl WorkspaceImageAsset {
    pub fn metadata(&self) -> &WorkspaceImageAssetMetadata {
        &self.metadata
    }

    pub fn id(&self) -> &str {
        self.metadata.id()
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn status(&self) -> WorkspaceImageAssetStatus {
        self.status
    }

    pub fn format(&self) -> ImageFormat {
        self.metadata.format().to_gpui()
    }
}

impl Default for WorkspaceImageAssetMetadataRecord {
    fn default() -> Self {
        Self { assets: Vec::new() }
    }
}

impl WorkspaceImageAssetMetadataRecord {
    pub(crate) fn assets(&self) -> &[WorkspaceImageAssetMetadata] {
        &self.assets
    }

    pub(crate) fn push(&mut self, metadata: WorkspaceImageAssetMetadata) -> bool {
        if self.assets.iter().any(|asset| asset.id == metadata.id) {
            return false;
        }

        self.assets.push(metadata);
        true
    }

    pub(crate) fn mark_referenced(&mut self, asset_id: &str) -> bool {
        let Some(asset) = self.asset_mut(asset_id) else {
            return false;
        };
        if asset.unreferenced_at_millis.is_none() {
            return false;
        }

        asset.unreferenced_at_millis = None;
        true
    }

    pub(crate) fn mark_retained(&mut self, asset_id: &str, millis: u64) -> bool {
        let Some(asset) = self.asset_mut(asset_id) else {
            return false;
        };
        let mut changed = false;
        if asset.retained_at_millis.is_none() {
            asset.retained_at_millis = Some(millis);
            changed = true;
        }
        if asset.unreferenced_at_millis.take().is_some() {
            changed = true;
        }
        changed
    }

    pub(crate) fn mark_unreferenced(&mut self, asset_id: &str, millis: u64) -> bool {
        let Some(asset) = self.asset_mut(asset_id) else {
            return false;
        };
        if asset.retained_at_millis.is_some() || asset.unreferenced_at_millis.is_some() {
            return false;
        }

        asset.unreferenced_at_millis = Some(millis);
        true
    }

    fn asset_mut(&mut self, asset_id: &str) -> Option<&mut WorkspaceImageAssetMetadata> {
        self.assets.iter_mut().find(|asset| asset.id == asset_id)
    }
}

pub(crate) fn workspace_image_assets_dir(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join(IMAGE_ASSETS_DIR_NAME)
}

pub(crate) fn workspace_image_asset_path(
    workspace_dir: &Path,
    metadata: &WorkspaceImageAssetMetadata,
) -> Option<PathBuf> {
    let relative = Path::new(metadata.relative_path());
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return None;
    }

    Some(workspace_dir.join(relative))
}

pub(crate) fn begin_workspace_image_asset_write(
    workspace_dir: &Path,
    format: ImageFormat,
    byte_len: u64,
    created_at_millis: u64,
    source_backend_path: Option<String>,
) -> PendingWorkspaceImageAsset {
    let format = WorkspaceImageAssetFormat::from_gpui(format);
    let sequence = NEXT_IMAGE_ASSET_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let asset_id = format!(
        "img-{created_at_millis:016x}-{process_id:08x}-{sequence:016x}",
        process_id = std::process::id()
    );
    let file_name = format!("{asset_id}.{}", format.extension());
    let relative_path = format!("{IMAGE_ASSETS_DIR_NAME}/{file_name}");
    let file_path = workspace_dir.join(&relative_path);
    let metadata = WorkspaceImageAssetMetadata {
        id: asset_id,
        relative_path,
        format,
        byte_len,
        created_at_millis,
        retained_at_millis: None,
        unreferenced_at_millis: None,
        source_backend_path,
        width_px: None,
        height_px: None,
    };

    PendingWorkspaceImageAsset {
        metadata,
        file_path,
    }
}

pub(crate) fn write_pending_workspace_image_asset_file(
    pending: &PendingWorkspaceImageAsset,
    bytes: &[u8],
) -> Result<(), WorkspaceImageAssetFileWriteError> {
    let Some(parent) = pending.file_path.parent() else {
        return Err(WorkspaceImageAssetFileWriteError::CreateDirectory {
            path: pending.file_path.display().to_string(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "image path has no parent"),
        });
    };
    fs::create_dir_all(parent).map_err(|source| {
        WorkspaceImageAssetFileWriteError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        }
    })?;

    let mut temp_file = tempfile::NamedTempFile::new_in(parent).map_err(|source| {
        WorkspaceImageAssetFileWriteError::Write {
            path: pending.file_path.display().to_string(),
            source,
        }
    })?;
    temp_file
        .write_all(bytes)
        .and_then(|()| temp_file.as_file().sync_all())
        .map_err(|source| WorkspaceImageAssetFileWriteError::Write {
            path: temp_file.path().display().to_string(),
            source,
        })?;

    let temp_path = temp_file.path().display().to_string();
    temp_file.persist(&pending.file_path).map_err(|error| {
        let tempfile::PersistError { error: source, .. } = error;
        WorkspaceImageAssetFileWriteError::Rename {
            from: temp_path,
            to: pending.file_path.display().to_string(),
            source,
        }
    })?;
    Ok(())
}

pub(crate) fn materialize_workspace_image_asset(
    workspace_dir: &Path,
    metadata: WorkspaceImageAssetMetadata,
) -> WorkspaceImageAsset {
    let file_path = workspace_image_asset_path(workspace_dir, &metadata).unwrap_or_default();
    let status = workspace_image_asset_status(&file_path, metadata.byte_len());
    WorkspaceImageAsset {
        metadata,
        file_path,
        status,
    }
}

pub(crate) fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn workspace_image_asset_status(path: &Path, expected_byte_len: u64) -> WorkspaceImageAssetStatus {
    let Ok(metadata) = fs::metadata(path) else {
        return WorkspaceImageAssetStatus::MissingFile;
    };

    if metadata.len() == expected_byte_len {
        WorkspaceImageAssetStatus::Available
    } else {
        WorkspaceImageAssetStatus::CorruptFile
    }
}

#[derive(Debug)]
pub(crate) enum WorkspaceImageAssetFileWriteError {
    CreateDirectory {
        path: String,
        source: io::Error,
    },
    Write {
        path: String,
        source: io::Error,
    },
    Rename {
        from: String,
        to: String,
        source: io::Error,
    },
}
