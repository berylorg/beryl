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

pub(super) struct SidecarDirectoryChain {
    _home: platform::RetainedDirectory,
    _root: platform::RetainedDirectory,
    _namespace: platform::RetainedDirectory,
    shard: platform::RetainedDirectory,
    shard_path: PathBuf,
}

impl SidecarDirectoryChain {
    pub(super) fn shard_path(&self) -> &Path {
        &self.shard_path
    }
}

pub(super) fn retain_sidecar_directories(
    home_path: &Path,
    address: &SidecarAddress,
    faults: &FaultController,
    durability_tier: HomeDurabilityTier,
    create_missing: bool,
    repair_barriers: bool,
) -> Result<SidecarDirectoryChain, SidecarError> {
    let home = retain_directory(home_path, false)?;
    let root_path = home_path.join(SIDECAR_DIRECTORY);
    let root = retain_directory(&root_path, create_missing)?;
    if repair_barriers {
        flush_directory(
            faults,
            FaultPoint::BeforeSidecarRootDirectorySync,
            &home,
            durability_tier,
        )?;
    }

    let namespace_path = root_path.join(address.namespace.as_str());
    let namespace = retain_directory(&namespace_path, create_missing)?;
    if repair_barriers {
        flush_directory(
            faults,
            FaultPoint::BeforeSidecarNamespaceDirectorySync,
            &root,
            durability_tier,
        )?;
    }

    let shard_path = namespace_path.join(
        digest_hex(address.digest)
            .get(..2)
            .expect("SHA-256 hex always has a shard"),
    );
    let shard = retain_directory(&shard_path, create_missing)?;
    if repair_barriers {
        flush_directory(
            faults,
            FaultPoint::BeforeSidecarShardDirectorySync,
            &namespace,
            durability_tier,
        )?;
    }

    Ok(SidecarDirectoryChain {
        _home: home,
        _root: root,
        _namespace: namespace,
        shard,
        shard_path,
    })
}

pub(super) fn open_and_verify_final(
    faults: &FaultController,
    directories: &SidecarDirectoryChain,
    address: &SidecarAddress,
    expected_bytes: Option<&[u8]>,
    durability_tier: HomeDurabilityTier,
    repair_final_barrier: bool,
) -> Result<(), SidecarError> {
    let path = final_path(directories.shard_path(), address);
    let mut file = platform::open_final_file(&path).map_err(map_final_open_error)?;
    faults
        .check(FaultPoint::BeforeSidecarVerification)
        .map_err(|source| storage(SidecarStage::OpenFinal, source))?;
    verify_file(&mut file, address, expected_bytes)?;
    if repair_final_barrier {
        flush_directory(
            faults,
            FaultPoint::BeforeSidecarFinalDirectorySync,
            &directories.shard,
            durability_tier,
        )?;
    }
    Ok(())
}

fn retain_directory(
    path: &Path,
    create_missing: bool,
) -> Result<platform::RetainedDirectory, SidecarError> {
    match platform::open_directory(path) {
        Ok(directory) => Ok(directory),
        Err(platform::OpenObjectError::Io(source))
            if create_missing && source.kind() == io::ErrorKind::NotFound =>
        {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
                Err(source) => return Err(storage(SidecarStage::CreateDirectory, source)),
            }
            platform::open_directory(path).map_err(map_directory_open_error)
        }
        Err(source) => Err(map_directory_open_error(source)),
    }
}

fn flush_directory(
    faults: &FaultController,
    point: FaultPoint,
    directory: &platform::RetainedDirectory,
    durability_tier: HomeDurabilityTier,
) -> Result<(), SidecarError> {
    faults
        .check(point)
        .map_err(|source| storage(SidecarStage::FlushDirectory, source))?;
    match platform::flush_directory(directory) {
        Ok(()) => Ok(()),
        Err(source)
            if durability_tier == HomeDurabilityTier::BestEffort
                && platform::directory_sync_unsupported(&source) =>
        {
            Ok(())
        }
        Err(source) => Err(storage(SidecarStage::FlushDirectory, source)),
    }
}

fn map_directory_open_error(source: platform::OpenObjectError) -> SidecarError {
    match source {
        platform::OpenObjectError::InvalidLayout => SidecarError::InvalidLayout,
        platform::OpenObjectError::Io(source) if source.kind() == io::ErrorKind::NotFound => {
            SidecarError::Missing
        }
        platform::OpenObjectError::Io(source) => storage(SidecarStage::CreateDirectory, source),
    }
}

fn map_final_open_error(source: platform::OpenObjectError) -> SidecarError {
    match source {
        platform::OpenObjectError::InvalidLayout => SidecarError::InvalidLayout,
        platform::OpenObjectError::Io(source) if source.kind() == io::ErrorKind::NotFound => {
            SidecarError::Missing
        }
        platform::OpenObjectError::Io(source) => storage(SidecarStage::OpenFinal, source),
    }
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
        | SidecarError::Storage { .. } => FailureSeverity::Structural,
        SidecarError::GenerationPoisoned
        | SidecarError::Missing
        | SidecarError::ContentMismatch
        | SidecarError::InvalidLayout => FailureSeverity::Structural,
    }
}
