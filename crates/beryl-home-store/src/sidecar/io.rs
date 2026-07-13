use std::{
    error::Error,
    fs,
    fs::File,
    io,
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::*;

pub(super) fn ensure_directory(
    store: &HomeStore,
    parent: &Path,
    path: &Path,
) -> Result<(), SidecarError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => ensure_ordinary_directory(&metadata),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(storage(SidecarStage::CreateDirectory, source)),
            }
            let metadata = fs::symlink_metadata(path)
                .map_err(|source| storage(SidecarStage::CreateDirectory, source))?;
            ensure_ordinary_directory(&metadata)?;
            flush_directory(store, parent)
        }
        Err(source) => Err(storage(SidecarStage::CreateDirectory, source)),
    }
}

pub(super) fn flush_directory(store: &HomeStore, path: &Path) -> Result<(), SidecarError> {
    store
        .faults
        .check(FaultPoint::BeforeSidecarDirectorySync)
        .map_err(|source| storage(SidecarStage::FlushDirectory, source))?;
    platform::flush_directory(path).map_err(|source| storage(SidecarStage::FlushDirectory, source))
}

pub(super) fn verify_file(
    file: &mut File,
    address: &SidecarAddress,
    expected_bytes: Option<&[u8]>,
) -> Result<(), SidecarError> {
    let metadata = file
        .metadata()
        .map_err(|source| storage(SidecarStage::ReadFinal, source))?;
    if metadata.len() != address.length {
        return Err(SidecarError::ContentMismatch);
    }

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut offset = 0usize;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| storage(SidecarStage::ReadFinal, source))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        if let Some(expected) = expected_bytes {
            let end = offset
                .checked_add(count)
                .ok_or(SidecarError::ContentMismatch)?;
            if expected.get(offset..end) != Some(&buffer[..count]) {
                return Err(SidecarError::ContentMismatch);
            }
            offset = end;
        }
    }
    if expected_bytes.is_some_and(|expected| offset != expected.len()) {
        return Err(SidecarError::ContentMismatch);
    }
    let found: [u8; HASH_BYTES] = hasher.finalize().into();
    if found != address.digest.0 {
        return Err(SidecarError::ContentMismatch);
    }
    Ok(())
}

pub(super) fn sidecar_shard(home: &Path, address: &SidecarAddress) -> PathBuf {
    let digest = digest_hex(address.digest);
    home.join(SIDECAR_DIRECTORY)
        .join(address.namespace.as_str())
        .join(digest.get(..2).expect("SHA-256 hex always has a shard"))
}

pub(super) fn final_path(directory: &Path, address: &SidecarAddress) -> PathBuf {
    directory.join(digest_hex(address.digest))
}

pub(super) fn temporary_path(directory: &Path) -> Result<PathBuf, SidecarError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|source| storage(SidecarStage::CreateTemporary, source))?;
    Ok(directory.join(format!(".tmp-{}", hex::encode(random))))
}

pub(super) fn digest_hex(digest: SidecarDigest) -> String {
    hex::encode(digest.0)
}

pub(super) fn ensure_bound(actual: u64, limit: SidecarByteLimit) -> Result<(), SidecarError> {
    if actual > limit.get() {
        Err(SidecarError::BoundExceeded {
            maximum: limit.get(),
            actual,
        })
    } else {
        Ok(())
    }
}

fn ensure_ordinary_directory(metadata: &fs::Metadata) -> Result<(), SidecarError> {
    if metadata.is_dir() && !is_reparse_point(metadata) {
        Ok(())
    } else {
        Err(SidecarError::InvalidLayout)
    }
}

#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub(super) fn storage(
    stage: SidecarStage,
    source: impl Error + Send + Sync + 'static,
) -> SidecarError {
    SidecarError::Storage {
        stage,
        source: Box::new(source),
    }
}

pub(super) fn sidecar_failure_severity(error: &SidecarError) -> FailureSeverity {
    match error {
        SidecarError::HealthGate(_)
        | SidecarError::BoundExceeded { .. }
        | SidecarError::Storage { .. } => FailureSeverity::Verify,
        SidecarError::GenerationPoisoned
        | SidecarError::Missing
        | SidecarError::ContentMismatch
        | SidecarError::InvalidLayout => FailureSeverity::Structural,
    }
}
