use std::{io::Write, path::Path};

use sha2::{Digest, Sha256};

use super::*;

impl HomeStore {
    /// Writes, flushes, atomically publishes, directory-flushes, and retains bytes.
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
        ensure_bound(actual, limit)?;
        let admission = self.health.admit()?;
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
        let digest = SidecarDigest(Sha256::digest(bytes).into());
        let address = SidecarAddress::new(namespace, digest, actual);
        let result = self.admit_sidecar_inner(
            &address,
            bytes,
            generation_state.instance_id,
            admission.generation(),
        );
        match result {
            Ok(sidecar) => {
                admission.confirm()?;
                Ok(sidecar)
            }
            Err(error) => {
                admission.fail(sidecar_failure_severity(&error));
                Err(error)
            }
        }
    }

    /// Verifies one referenced sidecar and retains it against replacement.
    pub fn verify_sidecar(
        &self,
        address: &SidecarAddress,
        limit: SidecarByteLimit,
    ) -> Result<VerifiedSidecar, SidecarError> {
        ensure_bound(address.length, limit)?;
        let admission = self.health.admit()?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(SidecarError::GenerationPoisoned);
            }
        };
        if generation.is_none() {
            admission.fail(FailureSeverity::Structural);
            return Err(SidecarError::GenerationPoisoned);
        }
        drop(generation);
        let result = self.verify_sidecar_inner(address, admission.generation());
        match result {
            Ok(sidecar) => {
                admission.confirm()?;
                Ok(sidecar)
            }
            Err(error) => {
                admission.fail(sidecar_failure_severity(&error));
                Err(error)
            }
        }
    }

    fn admit_sidecar_inner(
        &self,
        address: &SidecarAddress,
        bytes: &[u8],
        store: StoreInstanceId,
        generation: HomeGeneration,
    ) -> Result<AdmittedSidecar, SidecarError> {
        let directories =
            retain_sidecar_directories(self.canonical_path(), address, &self.faults, true, true)?;
        let final_path = final_path(directories.shard_path(), address);
        match open_and_verify_final(&self.faults, &directories, address, Some(bytes), None, true) {
            Ok(file) => {
                return Ok(AdmittedSidecar {
                    address: address.clone(),
                    path: final_path,
                    _file: file,
                    store,
                    generation,
                });
            }
            Err(SidecarError::Missing) => {}
            Err(source) => return Err(source),
        }

        let temporary = temporary_path(directories.shard_path())?;
        self.faults
            .check(FaultPoint::BeforeSidecarWrite)
            .map_err(|source| storage(SidecarStage::WriteTemporary, source))?;
        let mut file = platform::create_temporary(&temporary)
            .map_err(|source| storage(SidecarStage::CreateTemporary, source))?;
        file.write_all(bytes)
            .map_err(|source| storage(SidecarStage::WriteTemporary, source))?;
        self.faults
            .check(FaultPoint::BeforeSidecarFileSync)
            .map_err(|source| storage(SidecarStage::FlushTemporary, source))?;
        file.sync_all()
            .map_err(|source| storage(SidecarStage::FlushTemporary, source))?;
        let published_identity = platform::file_identity(&file)
            .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
        drop(file);

        let expected_identity =
            self.publish_or_reuse(&temporary, &final_path, published_identity)?;
        let file = open_and_verify_final(
            &self.faults,
            &directories,
            address,
            Some(bytes),
            expected_identity,
            true,
        )?;
        Ok(AdmittedSidecar {
            address: address.clone(),
            path: final_path,
            _file: file,
            store,
            generation,
        })
    }

    fn verify_sidecar_inner(
        &self,
        address: &SidecarAddress,
        generation: HomeGeneration,
    ) -> Result<VerifiedSidecar, SidecarError> {
        let directories =
            retain_sidecar_directories(self.canonical_path(), address, &self.faults, false, true)?;
        let path = final_path(directories.shard_path(), address);
        let file = open_and_verify_final(&self.faults, &directories, address, None, None, true)?;
        Ok(VerifiedSidecar {
            address: address.clone(),
            path,
            _file: file,
            generation,
        })
    }

    fn publish_or_reuse(
        &self,
        temporary: &Path,
        final_path: &Path,
        published_identity: platform::FileIdentity,
    ) -> Result<Option<platform::FileIdentity>, SidecarError> {
        self.faults
            .check(FaultPoint::BeforeSidecarRename)
            .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
        match platform::rename_durable(temporary, final_path) {
            Ok(platform::RenameOutcome::Published) => {
                self.faults
                    .check(FaultPoint::AfterSidecarRename)
                    .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
                Ok(Some(published_identity))
            }
            Ok(platform::RenameOutcome::Collision) => Ok(None),
            Err(source) => Err(storage(SidecarStage::RenameFinal, source)),
        }
    }
}
