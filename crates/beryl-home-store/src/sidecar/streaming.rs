use std::{fs::File, io::Read, io::Write, num::NonZeroUsize};

use sha2::{Digest, Sha256};

use super::*;

impl HomeStore {
    pub fn admit_sidecar_stream<R, F, C>(
        &self,
        namespace: SidecarNamespace,
        declared_length: u64,
        limit: SidecarByteLimit,
        working_page: NonZeroUsize,
        mut source: F,
        cancelled: C,
    ) -> Result<AdmittedSidecar, SidecarError>
    where
        R: Read,
        F: FnMut() -> std::io::Result<R>,
        C: Fn() -> bool,
    {
        ensure_bound(declared_length, limit)?;
        check_cancelled(&cancelled)?;
        let admission = self.health.admit_generation(self.admitted_generation)?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        let generation_state = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        let result = (|| {
            let mut page = vec![0; working_page.get().min(COPY_BUFFER_BYTES)];
            let digest = source_digest(&mut source, declared_length, &mut page, &cancelled)?;
            let address = SidecarAddress::new(namespace, digest, declared_length);
            let directories = retain_sidecar_directories(
                self.canonical_path(),
                &address,
                &self.faults,
                self.durability_tier(),
                true,
                true,
            )?;
            let path = final_path(directories.shard_path(), &address);
            check_cancelled(&cancelled)?;
            match platform::open_final_file(&path).map_err(map_final_open_error) {
                Ok(mut file) => {
                    self.faults
                        .check(FaultPoint::BeforeSidecarVerification)
                        .map_err(|error| storage(SidecarStage::OpenFinal, error))?;
                    compare_final(&mut file, &address, &mut source, &mut page, &cancelled)?;
                    check_cancelled(&cancelled)?;
                    directories.flush_final(&self.faults, self.durability_tier())?;
                }
                Err(SidecarError::Missing) => {
                    let temporary = temporary_path(directories.shard_path())?;
                    self.faults
                        .check(FaultPoint::BeforeSidecarWrite)
                        .map_err(|error| storage(SidecarStage::WriteTemporary, error))?;
                    check_cancelled(&cancelled)?;
                    let mut file = platform::create_temporary(&temporary)
                        .map_err(|error| storage(SidecarStage::CreateTemporary, error))?;
                    let mut reader = open_source(&mut source)?;
                    let mut hasher = Sha256::new();
                    read_source(
                        &mut reader,
                        declared_length,
                        &mut page,
                        &cancelled,
                        |bytes| {
                            hasher.update(bytes);
                            file.write_all(bytes)
                                .map_err(|error| storage(SidecarStage::WriteTemporary, error))
                        },
                    )?;
                    drop(reader);
                    if SidecarDigest(hasher.finalize().into()) != address.digest {
                        return Err(SidecarError::ContentMismatch);
                    }
                    check_cancelled(&cancelled)?;
                    self.faults
                        .check(FaultPoint::BeforeSidecarFileSync)
                        .map_err(|error| storage(SidecarStage::FlushTemporary, error))?;
                    file.sync_all()
                        .map_err(|error| storage(SidecarStage::FlushTemporary, error))?;
                    drop(file);
                    check_cancelled(&cancelled)?;
                    self.publish_or_reuse(&temporary, &path)?;
                    let mut file =
                        platform::open_final_file(&path).map_err(map_final_open_error)?;
                    self.faults
                        .check(FaultPoint::BeforeSidecarVerification)
                        .map_err(|error| storage(SidecarStage::OpenFinal, error))?;
                    compare_final(&mut file, &address, &mut source, &mut page, &|| false)?;
                    directories.flush_final(&self.faults, self.durability_tier())?;
                }
                Err(error) => return Err(error),
            }
            Ok(AdmittedSidecar {
                address,
                path,
                store: generation_state.instance_id,
                generation: admission.generation(),
            })
        })();
        match result {
            Ok(sidecar) => {
                admission.confirm_database(&generation_state.database, |error| {
                    storage(SidecarStage::ConfirmHealth, error)
                })?;
                Ok(sidecar)
            }
            Err(error) => {
                if !matches!(
                    error,
                    SidecarError::Cancelled
                        | SidecarError::Source { .. }
                        | SidecarError::LengthMismatch { .. }
                ) {
                    admission.fail(sidecar_failure_severity(&error));
                }
                Err(error)
            }
        }
    }
}

fn check_cancelled(cancelled: &impl Fn() -> bool) -> Result<(), SidecarError> {
    if cancelled() {
        Err(SidecarError::Cancelled)
    } else {
        Ok(())
    }
}

fn open_source<R: Read>(
    source: &mut impl FnMut() -> std::io::Result<R>,
) -> Result<R, SidecarError> {
    source().map_err(|source| SidecarError::Source { source })
}

fn read_source(
    reader: &mut impl Read,
    declared: u64,
    page: &mut [u8],
    cancelled: &impl Fn() -> bool,
    mut consume: impl FnMut(&[u8]) -> Result<(), SidecarError>,
) -> Result<(), SidecarError> {
    let mut actual = 0;
    while actual < declared {
        check_cancelled(cancelled)?;
        let maximum = usize::try_from((declared - actual).min(page.len() as u64))
            .expect("read size is bounded by a page");
        let count = read_source_page(reader, &mut page[..maximum], cancelled)?;
        if count == 0 {
            return Err(SidecarError::LengthMismatch { declared, actual });
        }
        actual += count as u64;
        consume(&page[..count])?;
    }
    check_cancelled(cancelled)?;
    let count = read_source_page(reader, &mut page[..1], cancelled)?;
    if count != 0 {
        return Err(SidecarError::LengthMismatch {
            declared,
            actual: declared.saturating_add(1),
        });
    }
    check_cancelled(cancelled)
}

fn read_source_page(
    reader: &mut impl Read,
    page: &mut [u8],
    cancelled: &impl Fn() -> bool,
) -> Result<usize, SidecarError> {
    loop {
        check_cancelled(cancelled)?;
        match reader.read(page) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => return result.map_err(|source| SidecarError::Source { source }),
        }
    }
}

fn read_retry(reader: &mut impl Read, page: &mut [u8]) -> std::io::Result<usize> {
    loop {
        match reader.read(page) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

fn source_digest<R: Read>(
    source: &mut impl FnMut() -> std::io::Result<R>,
    declared: u64,
    page: &mut [u8],
    cancelled: &impl Fn() -> bool,
) -> Result<SidecarDigest, SidecarError> {
    check_cancelled(cancelled)?;
    let mut reader = open_source(source)?;
    let mut hasher = Sha256::new();
    read_source(&mut reader, declared, page, cancelled, |bytes| {
        hasher.update(bytes);
        Ok(())
    })?;
    Ok(SidecarDigest(hasher.finalize().into()))
}

fn compare_final<R: Read>(
    file: &mut File,
    address: &SidecarAddress,
    source: &mut impl FnMut() -> std::io::Result<R>,
    page: &mut [u8],
    cancelled: &impl Fn() -> bool,
) -> Result<(), SidecarError> {
    if file
        .metadata()
        .map_err(|error| storage(SidecarStage::ReadFinal, error))?
        .len()
        != address.length
    {
        return Err(SidecarError::ContentMismatch);
    }
    check_cancelled(cancelled)?;
    let mut reader = open_source(source)?;
    let mut hasher = Sha256::new();
    if page.len() == 1 {
        read_source(&mut reader, address.length, page, cancelled, |bytes| {
            let mut found = [0];
            file.read_exact(&mut found)
                .map_err(|error| storage(SidecarStage::ReadFinal, error))?;
            if bytes != found {
                return Err(SidecarError::ContentMismatch);
            }
            hasher.update(found);
            Ok(())
        })?;
    } else {
        let (expected, found) = page.split_at_mut(page.len() / 2);
        read_source(&mut reader, address.length, expected, cancelled, |bytes| {
            let found = &mut found[..bytes.len()];
            file.read_exact(found)
                .map_err(|error| storage(SidecarStage::ReadFinal, error))?;
            if bytes != found {
                return Err(SidecarError::ContentMismatch);
            }
            hasher.update(found);
            Ok(())
        })?;
    }
    check_cancelled(cancelled)?;
    if read_retry(file, &mut page[..1]).map_err(|error| storage(SidecarStage::ReadFinal, error))?
        != 0
        || SidecarDigest(hasher.finalize().into()) != address.digest
    {
        return Err(SidecarError::ContentMismatch);
    }
    Ok(())
}
