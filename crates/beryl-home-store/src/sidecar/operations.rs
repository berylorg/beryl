use std::{
    fs::File,
    io,
    io::Write,
    path::{Path, PathBuf},
};

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
        let directory = self.ensure_sidecar_directories(address)?;
        let final_path = final_path(&directory, address);
        if final_path.exists() {
            let file = self.open_and_verify(&final_path, address, Some(bytes))?;
            return Ok(AdmittedSidecar {
                address: address.clone(),
                path: final_path,
                _file: file,
                store,
                generation,
            });
        }

        let temporary = temporary_path(&directory)?;
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
        drop(file);

        self.publish_or_reuse(&temporary, &final_path, address, bytes)?;
        let file = self.open_and_verify(&final_path, address, Some(bytes))?;
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
        let path = final_path(&sidecar_shard(self.canonical_path(), address), address);
        let file = self.open_and_verify(&path, address, None)?;
        Ok(VerifiedSidecar {
            address: address.clone(),
            path,
            _file: file,
            generation,
        })
    }

    fn ensure_sidecar_directories(
        &self,
        address: &SidecarAddress,
    ) -> Result<PathBuf, SidecarError> {
        let home = self.canonical_path();
        let root = home.join(SIDECAR_DIRECTORY);
        ensure_directory(self, home, &root)?;
        let namespace = root.join(address.namespace.as_str());
        ensure_directory(self, &root, &namespace)?;
        let shard = namespace.join(digest_hex(address.digest).get(..2).expect("digest hex"));
        ensure_directory(self, &namespace, &shard)?;
        Ok(shard)
    }

    fn publish_or_reuse(
        &self,
        temporary: &Path,
        final_path: &Path,
        address: &SidecarAddress,
        bytes: &[u8],
    ) -> Result<(), SidecarError> {
        self.faults
            .check(FaultPoint::BeforeSidecarRename)
            .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
        match platform::rename_durable(temporary, final_path) {
            Ok(()) => {
                self.faults
                    .check(FaultPoint::AfterSidecarRename)
                    .map_err(|source| storage(SidecarStage::RenameFinal, source))?;
                flush_directory(self, final_path.parent().expect("sidecar has parent"))
            }
            Err(_rename_error) if final_path.exists() => {
                self.open_and_verify(final_path, address, Some(bytes))?;
                Ok(())
            }
            Err(source) => Err(storage(SidecarStage::RenameFinal, source)),
        }
    }

    fn open_and_verify(
        &self,
        path: &Path,
        address: &SidecarAddress,
        expected_bytes: Option<&[u8]>,
    ) -> Result<File, SidecarError> {
        self.faults
            .check(FaultPoint::BeforeSidecarVerification)
            .map_err(|source| storage(SidecarStage::OpenFinal, source))?;
        let mut file = platform::open_retained(path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                SidecarError::Missing
            } else {
                storage(SidecarStage::OpenFinal, source)
            }
        })?;
        verify_file(&mut file, address, expected_bytes)?;
        Ok(file)
    }
}
