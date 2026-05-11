use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use beryl_backend::{ManagedBackendSession, ThreadItem, TurnInfo, UserInput};
use beryl_model::workspace::{BerylWorkspaceId, RuntimeMode};
use gpui::ImageFormat;
use tracing::warn;

use super::{
    composer_image_delivery::runtime_readable_image_path,
    execution_detail::{TranscriptImagePathResolver, TranscriptImageSourceResolution},
};
use crate::{
    BerylWorkspacePersistence, WorkspaceImageAsset, WorkspaceImageAssetStatus,
    WorkspacePersistenceError,
};

const MAX_EXTERNAL_IMPORTS_PER_HISTORY_PAGE: usize = 16;

pub(crate) trait TranscriptImageExternalReader {
    type Error: std::fmt::Display;

    fn read_file_bytes(&mut self, path: &str, timeout: Duration) -> Result<Vec<u8>, Self::Error>;
}

impl TranscriptImageExternalReader for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn read_file_bytes(&mut self, path: &str, timeout: Duration) -> Result<Vec<u8>, Self::Error> {
        ManagedBackendSession::read_file_bytes(self, path, timeout)
    }
}

pub(crate) fn transcript_image_path_resolver_for_turns<R>(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    runtime_mode: &RuntimeMode,
    turns: &[TurnInfo],
    backend: &mut R,
    timeout: Duration,
) -> Result<TranscriptImagePathResolver, WorkspacePersistenceError>
where
    R: TranscriptImageExternalReader,
{
    let assets = persistence.load_workspace_image_assets(workspace_id)?;
    let mut resolver = transcript_image_path_resolver_for_assets(runtime_mode, &assets);
    let external_paths = unresolved_historical_local_image_paths(turns, &resolver);

    for path in external_paths
        .into_iter()
        .take(MAX_EXTERNAL_IMPORTS_PER_HISTORY_PAGE)
    {
        let Some(asset) = import_historical_image_path(
            persistence,
            workspace_id,
            runtime_mode,
            backend,
            timeout,
            &path,
        ) else {
            continue;
        };
        insert_asset_paths(runtime_mode, &asset, &mut resolver);
    }

    Ok(resolver)
}

pub(crate) fn transcript_image_path_resolver_for_assets(
    runtime_mode: &RuntimeMode,
    assets: &[WorkspaceImageAsset],
) -> TranscriptImagePathResolver {
    let mut resolver = TranscriptImagePathResolver::default();
    for asset in assets {
        insert_asset_paths(runtime_mode, asset, &mut resolver);
    }
    resolver
}

fn insert_asset_paths(
    runtime_mode: &RuntimeMode,
    asset: &WorkspaceImageAsset,
    resolver: &mut TranscriptImagePathResolver,
) {
    let runtime_path = runtime_readable_image_path(runtime_mode, asset.file_path(), |_| Ok(()));
    let resolution = resolution_for_asset(asset, runtime_path.is_ok());
    resolver
        .insert_local_path_resolution(asset.file_path().display().to_string(), resolution.clone());
    if let Some(source_path) = asset.metadata().source_backend_path() {
        resolver.insert_local_path_resolution(source_path.to_string(), resolution.clone());
    }
    if let Ok(runtime_path) = runtime_path {
        resolver.insert_local_path_resolution(runtime_path.backend_path().to_string(), resolution);
    }
}

fn resolution_for_asset(
    asset: &WorkspaceImageAsset,
    runtime_readable: bool,
) -> TranscriptImageSourceResolution {
    match asset.status() {
        WorkspaceImageAssetStatus::Available => {
            TranscriptImageSourceResolution::available_asset_with_format(
                asset.id(),
                asset.format(),
                runtime_readable,
            )
        }
        WorkspaceImageAssetStatus::MissingFile | WorkspaceImageAssetStatus::CorruptFile => {
            TranscriptImageSourceResolution::unavailable_asset(asset.id())
        }
    }
}

fn unresolved_historical_local_image_paths(
    turns: &[TurnInfo],
    resolver: &TranscriptImagePathResolver,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for turn in turns {
        for item in &turn.items {
            let ThreadItem::UserMessage(message) = item else {
                continue;
            };
            for input in &message.content {
                let UserInput::LocalImage { path } = input else {
                    continue;
                };
                if resolver.resolve_local_path(path).is_none() && seen.insert(path.clone()) {
                    paths.push(path.clone());
                }
            }
        }
    }
    paths
}

fn import_historical_image_path<R>(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    runtime_mode: &RuntimeMode,
    backend: &mut R,
    timeout: Duration,
    backend_path: &str,
) -> Option<WorkspaceImageAsset>
where
    R: TranscriptImageExternalReader,
{
    let format = image_format_from_path(backend_path)?;
    if let Some(host_path) = trusted_host_path_for_backend_path(runtime_mode, backend_path) {
        match fs::read(&host_path) {
            Ok(bytes) => {
                return persist_imported_asset(
                    persistence,
                    workspace_id,
                    format,
                    bytes,
                    backend_path,
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!(
                    path = %host_path.display(),
                    error = %error,
                    "failed to read host-visible historical transcript image"
                );
            }
        }
    }

    match backend.read_file_bytes(backend_path, timeout) {
        Ok(bytes) => persist_imported_asset(persistence, workspace_id, format, bytes, backend_path),
        Err(error) => {
            warn!(
                path = %backend_path,
                error = %error,
                "failed to import historical transcript image through backend fs/readFile"
            );
            None
        }
    }
}

fn persist_imported_asset(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    format: ImageFormat,
    bytes: Vec<u8>,
    source_backend_path: &str,
) -> Option<WorkspaceImageAsset> {
    match persistence.import_workspace_image_asset(
        workspace_id,
        format,
        &bytes,
        source_backend_path,
    ) {
        Ok(asset) => {
            if let Err(error) =
                persistence.mark_workspace_image_asset_retained(workspace_id, asset.id())
            {
                warn!(
                    asset_id = %asset.id(),
                    error = %error,
                    "failed to mark imported transcript image asset as retained"
                );
            }
            Some(asset)
        }
        Err(error) => {
            warn!(
                source_path = %source_backend_path,
                error = %error,
                "failed to persist imported transcript image asset"
            );
            None
        }
    }
}

fn trusted_host_path_for_backend_path(
    runtime_mode: &RuntimeMode,
    backend_path: &str,
) -> Option<PathBuf> {
    match runtime_mode {
        RuntimeMode::HostWindows => {
            let path = PathBuf::from(backend_path);
            path.is_absolute().then_some(path)
        }
        RuntimeMode::WslLinux { .. } => host_path_from_wsl_mount_path(backend_path),
    }
}

fn host_path_from_wsl_mount_path(path: &str) -> Option<PathBuf> {
    let normalized = path.replace('\\', "/");
    let rest = normalized.strip_prefix("/mnt/")?;
    let mut parts = rest.splitn(2, '/');
    let drive = parts.next()?;
    let tail = parts.next()?;
    if drive.len() != 1 || tail.is_empty() {
        return None;
    }
    let drive = drive.as_bytes()[0];
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}:\\{}",
        (drive as char).to_ascii_uppercase(),
        tail.replace('/', "\\")
    )))
}

fn image_format_from_path(path: &str) -> Option<ImageFormat> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::Webp),
        "gif" => Some(ImageFormat::Gif),
        "svg" => Some(ImageFormat::Svg),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}
