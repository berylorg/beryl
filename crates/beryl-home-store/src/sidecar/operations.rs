use std::{io, num::NonZeroUsize, path::Path};

use super::*;

impl HomeStore {
    /// Makes the next directory synchronization on this test thread return a
    /// synthetic physical-operation error.
    #[cfg(feature = "test-faults")]
    pub fn fail_next_sidecar_directory_sync_for_tests(&self, kind: io::ErrorKind) {
        let _ = self;
        platform::fail_next_directory_sync_for_tests(kind);
    }

    /// Writes, flushes, atomically publishes, directory-flushes, and authorizes bytes.
    ///
    /// The returned token must be held through the first metadata-reference
    /// command. Failure may leave inert temporary or final bytes; this package
    /// deliberately exposes no deletion operation.
    pub fn admit_sidecar(
        &self,
        namespace: SidecarNamespace,
        bytes: &[u8],
        limit: SidecarByteLimit,
    ) -> Result<AdmittedSidecar, SidecarError> {
        let actual = u64::try_from(bytes.len()).map_err(|_| SidecarError::BoundExceeded {
            maximum: limit.get(),
            actual: u64::MAX,
        })?;
        self.admit_sidecar_stream(
            namespace,
            actual,
            limit,
            NonZeroUsize::new(COPY_BUFFER_BYTES).expect("copy page is positive"),
            || Ok(io::Cursor::new(bytes)),
            || false,
        )
    }

    /// Verifies one referenced sidecar at the current path.
    pub fn verify_sidecar(
        &self,
        address: &SidecarAddress,
        limit: SidecarByteLimit,
    ) -> Result<VerifiedSidecar, SidecarError> {
        ensure_bound(address.length, limit)?;
        let admission = self.health.admit_generation(self.admitted_generation)?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        let database = match generation.as_ref() {
            Some(generation) => generation.database.clone(),
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        drop(generation);
        let result = self.verify_sidecar_inner(address, admission.generation());
        match result {
            Ok(sidecar) => {
                admission.confirm_database(&database, |source| {
                    storage(SidecarStage::ConfirmHealth, source)
                })?;
                Ok(sidecar)
            }
            Err(error) => {
                admission.fail(sidecar_failure_severity(&error));
                Err(error)
            }
        }
    }

    fn verify_sidecar_inner(
        &self,
        address: &SidecarAddress,
        generation: HomeGeneration,
    ) -> Result<VerifiedSidecar, SidecarError> {
        let directories = retain_sidecar_directories(
            self.canonical_path(),
            address,
            &self.faults,
            self.durability_tier(),
            false,
            true,
        )?;
        let path = final_path(directories.shard_path(), address);
        open_and_verify_final(
            &self.faults,
            &directories,
            address,
            None,
            self.durability_tier(),
            true,
        )?;
        Ok(VerifiedSidecar {
            address: address.clone(),
            path,
            generation,
        })
    }

    pub(super) fn publish_or_reuse(
        &self,
        temporary: &Path,
        final_path: &Path,
    ) -> Result<(), SidecarError> {
        self.faults
            .check(FaultPoint::BeforeSidecarRename)
            .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
        match platform::rename_durable(temporary, final_path) {
            Ok(platform::RenameOutcome::Published) => {
                self.faults
                    .check(FaultPoint::AfterSidecarRename)
                    .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
                Ok(())
            }
            Ok(platform::RenameOutcome::Collision) => Ok(()),
            Err(source) => Err(storage(SidecarStage::RenameFinal, source)),
        }
    }
}
