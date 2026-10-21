use std::{
    fs::File,
    io::{self, BufRead, Read, Seek, SeekFrom},
    num::NonZeroUsize,
};

use crate::CommandCancellation;

use super::*;

pub struct VerifiedSidecarReader {
    verified: VerifiedSidecar,
    file: File,
    page: Vec<u8>,
    start: usize,
    end: usize,
    position: u64,
    failed: bool,
    cancellation: CommandCancellation,
}

impl VerifiedSidecarReader {
    pub fn verified_sidecar(&self) -> &VerifiedSidecar {
        &self.verified
    }

    fn check_active(&self) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::other(SidecarError::Cancelled));
        }
        if self.failed {
            return Err(io::Error::other(storage(
                SidecarStage::ReadFinal,
                io::Error::other("verified sidecar reader previously failed"),
            )));
        }
        Ok(())
    }

    fn fail(&mut self, stage: SidecarStage, source: io::Error) -> io::Error {
        self.failed = true;
        self.start = 0;
        self.end = 0;
        io::Error::other(storage(stage, source))
    }
}

impl Read for VerifiedSidecarReader {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        self.check_active()?;
        if destination.is_empty() {
            return Ok(0);
        }
        let available = self.fill_buf()?;
        let count = destination.len().min(available.len());
        destination[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}

impl BufRead for VerifiedSidecarReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.check_active()?;
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
            loop {
                self.check_active()?;
                match self.file.read(&mut self.page) {
                    Ok(count) => {
                        self.end = count;
                        break;
                    }
                    Err(source) if source.kind() == io::ErrorKind::Interrupted => continue,
                    Err(source) => return Err(self.fail(SidecarStage::ReadFinal, source)),
                }
            }
            self.check_active()?;
        }
        Ok(&self.page[self.start..self.end])
    }

    fn consume(&mut self, amount: usize) {
        let consumed = amount.min(self.end - self.start);
        self.start += consumed;
        self.position += consumed as u64;
    }
}

impl Seek for VerifiedSidecarReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.check_active()?;
        let target = match position {
            SeekFrom::Current(offset) => {
                SeekFrom::Start(self.position.checked_add_signed(offset).ok_or_else(|| {
                    io::Error::other(storage(
                        SidecarStage::SeekFinal,
                        io::Error::new(io::ErrorKind::InvalidInput, "invalid sidecar seek"),
                    ))
                })?)
            }
            other => other,
        };
        match self.file.seek(target) {
            Ok(position) => {
                self.position = position;
                self.start = 0;
                self.end = 0;
                Ok(position)
            }
            Err(source) => Err(self.fail(SidecarStage::SeekFinal, source)),
        }
    }
}

impl HomeStore {
    pub fn open_verified_sidecar_reader(
        &self,
        address: &SidecarAddress,
        limit: SidecarByteLimit,
        working_page: NonZeroUsize,
        cancellation: CommandCancellation,
    ) -> Result<VerifiedSidecarReader, SidecarError> {
        ensure_bound(address.length, limit)?;
        if cancellation.is_cancelled() {
            return Err(SidecarError::Cancelled);
        }
        let admission = self.health.admit_generation(self.admitted_generation)?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        let (database, store) = match generation.as_ref() {
            Some(generation) => (generation.database.clone(), generation.instance_id),
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        drop(generation);
        let result = (|| {
            let directories = retain_sidecar_directories(
                self.canonical_path(),
                address,
                &self.faults,
                self.durability_tier(),
                false,
                true,
            )?;
            let mut file = open_final_for_verification(&self.faults, &directories, address)?;
            let mut page = vec![0; working_page.get().min(COPY_BUFFER_BYTES)];
            verify_file_with_page(&mut file, address, None, &mut page, || {
                cancellation.is_cancelled()
            })?;
            file.seek(SeekFrom::Start(0))
                .map_err(|source| storage(SidecarStage::SeekFinal, source))?;
            directories.flush_final(&self.faults, self.durability_tier())?;
            if cancellation.is_cancelled() {
                return Err(SidecarError::Cancelled);
            }
            Ok(VerifiedSidecarReader {
                verified: VerifiedSidecar {
                    address: address.clone(),
                    path: final_path(directories.shard_path(), address),
                    store,
                    generation: admission.generation(),
                },
                file,
                page,
                start: 0,
                end: 0,
                position: 0,
                failed: false,
                cancellation,
            })
        })();
        match result {
            Ok(reader) => {
                admission.confirm_database(&database, |source| {
                    storage(SidecarStage::ConfirmHealth, source)
                })?;
                Ok(reader)
            }
            Err(error) => {
                if !matches!(error, SidecarError::Cancelled) {
                    admission.fail(sidecar_failure_severity(&error));
                }
                Err(error)
            }
        }
    }
}
