use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{HomeDurabilityTier, HomeStore, fault::FaultPoint};

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ThemeOperationEvidence {
    old: ThemeRepositorySnapshot,
    intended_manifest: Option<ThemeFileIdentity>,
    document: Option<DocumentEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentEvidence {
    id: StableThemeFileId,
    old: Option<ThemeFileIdentity>,
    intended: ThemeFileIdentity,
}

impl HomeStore {
    /// Acquires an exact physical repository snapshot for this healthy store instance.
    pub fn theme_repository_snapshot(
        &self,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        let admission = self.health.admit()?;
        let generation = self
            .generation
            .read()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(ThemeRepositoryError::LockPoisoned)?;
        let manifest = observe_optional(
            &manifest_path(self.canonical_path()),
            limits,
            ThemeRepositoryStage::Snapshot,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        admission.confirm_database(&generation.database, |source| ThemeRepositoryError::Io {
            stage: ThemeRepositoryStage::ConfirmHealth,
            source: io::Error::other(source),
        })?;
        Ok(ThemeRepositorySnapshot {
            home_id: self.home_id(),
            store_instance: generation.instance_id.0,
            generation: admission.generation(),
            manifest,
        })
    }

    /// Reads one exact bounded range after verifying the complete natural file identity.
    pub fn read_theme_file_range(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        selector: &ThemeFileSelector,
        expected: ThemeFileIdentity,
        offset: u64,
        max_bytes: NonZeroUsize,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeFileRange, ThemeRepositoryError> {
        let admission = self.health.admit()?;
        let generation = self.validate_snapshot(snapshot, limits)?;
        if max_bytes.get() as u64 > limits.max_source_bytes() {
            return Err(ThemeRepositoryError::LimitExceeded);
        }
        let path = selector_path(self.canonical_path(), selector);
        self.faults
            .check(FaultPoint::BeforeThemeRead)
            .map_err(|source| io_error(ThemeRepositoryStage::ReadRange, source))?;
        let (mut file, actual) = open_and_observe_required(
            &path,
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        if actual != expected {
            return Err(ThemeRepositoryError::IdentityMismatch);
        }
        if offset > actual.length() {
            return Err(ThemeRepositoryError::LimitExceeded);
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|source| io_error(ThemeRepositoryStage::ReadRange, source))?;
        let remaining = actual.length() - offset;
        let count = usize::try_from(remaining.min(max_bytes.get() as u64))
            .map_err(|_| ThemeRepositoryError::LimitExceeded)?;
        let mut bytes = vec![0; count];
        file.read_exact(&mut bytes)
            .map_err(|source| io_error(ThemeRepositoryStage::ReadRange, source))?;
        admission.confirm_database(&generation, |source| ThemeRepositoryError::Io {
            stage: ThemeRepositoryStage::ConfirmHealth,
            source: io::Error::other(source),
        })?;
        Ok(ThemeFileRange {
            identity: actual,
            offset,
            eof: offset + count as u64 == actual.length(),
            bytes,
        })
    }

    /// Observes the exact current identity of one natural repository file.
    pub fn observe_theme_file(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        selector: &ThemeFileSelector,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeFileIdentity, ThemeRepositoryError> {
        let admission = self.health.admit()?;
        let database = self.validate_snapshot(snapshot, limits)?;
        let identity = observe_required(
            &selector_path(self.canonical_path(), selector),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        admission.confirm_database(&database, |source| ThemeRepositoryError::Io {
            stage: ThemeRepositoryStage::ConfirmHealth,
            source: io::Error::other(source),
        })?;
        Ok(identity)
    }

    /// Atomically replaces one stable installed document without changing the manifest.
    pub fn replace_theme_document(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        id: &StableThemeFileId,
        expected_document: Option<ThemeFileIdentity>,
        intended_document: ThemeFileIdentity,
        source: &mut dyn Read,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let _mutation = self
            .theme_mutation
            .lock()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let admission = self.health.admit()?;
        let database = self.validate_snapshot(snapshot, limits)?;
        require_evidence_bounds(2, &[expected_document, Some(intended_document)], limits)?;
        let final_path = document_path(self.canonical_path(), id);
        require_identity(&final_path, expected_document, limits)?;
        let installed = prepare_directories(self.canonical_path())?;
        let staged = match stage_source(
            &installed,
            "document",
            intended_document,
            source,
            limits,
            &self.faults,
            FaultPoint::BeforeThemeDocumentWrite,
            FaultPoint::BeforeThemeDocumentSync,
            ThemeRepositoryStage::DocumentWrite,
            ThemeRepositoryStage::DocumentSync,
        ) {
            Ok(staged) => staged,
            Err(
                error
                @ (ThemeRepositoryError::SourceMismatch | ThemeRepositoryError::LimitExceeded),
            ) => return Err(error),
            Err(_) => {
                require_identity(&final_path, expected_document, limits)?;
                return Ok(ThemeMutationOutcome::NotCommitted);
            }
        };
        require_identity(&final_path, expected_document, limits)?;
        if self
            .faults
            .check(FaultPoint::BeforeThemeDocumentReplace)
            .is_err()
        {
            let _ = fs::remove_file(&staged);
            return Ok(ThemeMutationOutcome::NotCommitted);
        }
        if let Err(source) = super::platform::replace(&staged, &final_path) {
            return self.classify_document_failure(
                snapshot,
                id,
                expected_document,
                intended_document,
                limits,
                ThemeRepositoryStage::DocumentReplace,
                Some(source),
            );
        }
        if self
            .faults
            .check(FaultPoint::AfterThemeDocumentReplace)
            .is_err()
        {
            return Ok(indeterminate_document(
                self.home_id(),
                snapshot,
                id,
                expected_document,
                intended_document,
            ));
        }
        if let Err(source) = sync_with_fault(
            &installed,
            self.durability_tier(),
            &self.faults,
            FaultPoint::BeforeThemeInstalledDirectorySync,
        ) {
            let _ = source;
            return Ok(indeterminate_document(
                self.home_id(),
                snapshot,
                id,
                expected_document,
                intended_document,
            ));
        }
        if observe_optional(
            &final_path,
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )? != Some(intended_document)
        {
            return Ok(indeterminate_document(
                self.home_id(),
                snapshot,
                id,
                expected_document,
                intended_document,
            ));
        }
        let evidence = commit(
            snapshot.clone(),
            Some((id.clone(), intended_document)),
            None,
        );
        match admission.confirm_database(&database, |source| {
            io_error(
                ThemeRepositoryStage::ConfirmHealth,
                io::Error::other(source),
            )
        }) {
            Ok(()) => Ok(ThemeMutationOutcome::Committed(evidence)),
            Err(_) => Ok(ThemeMutationOutcome::Committed(commit(
                snapshot.clone(),
                Some((id.clone(), intended_document)),
                Some(ThemeRepositoryStage::ConfirmHealth),
            ))),
        }
    }

    /// Atomically replaces only the opaque owner manifest.
    pub fn replace_theme_manifest(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        intended_manifest: ThemeFileIdentity,
        source: &mut dyn Read,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        self.publish_manifest(snapshot, None, intended_manifest, source, limits, None)
    }

    /// Publishes a stable document first and the admitting opaque manifest last.
    pub fn install_theme_document(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        id: &StableThemeFileId,
        expected_document: Option<ThemeFileIdentity>,
        intended_document: ThemeFileIdentity,
        document_source: &mut dyn Read,
        intended_manifest: ThemeFileIdentity,
        manifest_source: &mut dyn Read,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        if limits.max_staged_files().get() < 2 {
            return Err(ThemeRepositoryError::LimitExceeded);
        }
        let _mutation = self
            .theme_mutation
            .lock()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let admission = self.health.admit()?;
        let database = self.validate_snapshot(snapshot, limits)?;
        require_evidence_bounds(
            4,
            &[
                snapshot.manifest,
                Some(intended_manifest),
                expected_document,
                Some(intended_document),
            ],
            limits,
        )?;
        let installed = prepare_directories(self.canonical_path())?;
        let final_document = document_path(self.canonical_path(), id);
        require_identity(&final_document, expected_document, limits)?;
        let staged_document = match stage_source(
            &installed,
            "document",
            intended_document,
            document_source,
            limits,
            &self.faults,
            FaultPoint::BeforeThemeDocumentWrite,
            FaultPoint::BeforeThemeDocumentSync,
            ThemeRepositoryStage::DocumentWrite,
            ThemeRepositoryStage::DocumentSync,
        ) {
            Ok(staged) => staged,
            Err(
                error
                @ (ThemeRepositoryError::SourceMismatch | ThemeRepositoryError::LimitExceeded),
            ) => return Err(error),
            Err(_) => return Ok(ThemeMutationOutcome::NotCommitted),
        };
        require_identity(&final_document, expected_document, limits)?;
        if self
            .faults
            .check(FaultPoint::BeforeThemeDocumentReplace)
            .is_err()
        {
            let _ = fs::remove_file(&staged_document);
            return Ok(ThemeMutationOutcome::NotCommitted);
        }
        if super::platform::replace(&staged_document, &final_document).is_err() {
            return self.classify_install(
                snapshot,
                id,
                expected_document,
                intended_document,
                intended_manifest,
                limits,
            );
        }
        if self
            .faults
            .check(FaultPoint::AfterThemeDocumentReplace)
            .is_err()
            || sync_with_fault(
                &installed,
                self.durability_tier(),
                &self.faults,
                FaultPoint::BeforeThemeInstalledDirectorySync,
            )
            .is_err()
        {
            return self.classify_install(
                snapshot,
                id,
                expected_document,
                intended_document,
                intended_manifest,
                limits,
            );
        }
        if observe_optional(
            &final_document,
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )? != Some(intended_document)
        {
            return Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                Some((id, expected_document, intended_document)),
                intended_manifest,
            ));
        }
        let outcome = self.publish_manifest_inner(
            snapshot,
            Some((id, expected_document, intended_document)),
            intended_manifest,
            manifest_source,
            limits,
        )?;
        if matches!(outcome, ThemeMutationOutcome::Committed(_)) {
            let _ = admission.confirm_database(&database, |_| ThemeRepositoryError::LockPoisoned);
        }
        Ok(outcome)
    }

    /// Publishes the removing manifest first, then best-effort removes the now-inert document.
    pub fn delete_theme_document(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        id: &StableThemeFileId,
        expected_document: ThemeFileIdentity,
        intended_manifest: ThemeFileIdentity,
        manifest_source: &mut dyn Read,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let _mutation = self
            .theme_mutation
            .lock()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let admission = self.health.admit()?;
        let database = self.validate_snapshot(snapshot, limits)?;
        let document_path = document_path(self.canonical_path(), id);
        require_identity(&document_path, Some(expected_document), limits)?;
        let outcome = self.publish_manifest_inner(
            snapshot,
            None,
            intended_manifest,
            manifest_source,
            limits,
        )?;
        let ThemeMutationOutcome::Committed(evidence) = outcome else {
            return Ok(outcome);
        };
        let installed = self.canonical_path().join("themes").join("installed");
        let cleanup = (|| {
            require_identity(&document_path, Some(expected_document), limits)?;
            self.faults
                .check(FaultPoint::BeforeThemeDocumentRemove)
                .map_err(|source| io_error(ThemeRepositoryStage::DocumentRemove, source))?;
            fs::remove_file(&document_path)
                .map_err(|source| io_error(ThemeRepositoryStage::DocumentRemove, source))?;
            sync_with_fault(
                &installed,
                self.durability_tier(),
                &self.faults,
                FaultPoint::BeforeThemeInstalledDirectorySync,
            )
            .map_err(|source| io_error(ThemeRepositoryStage::InstalledDirectorySync, source))
        })();
        let later_failure = cleanup.err().map(|error| match error {
            ThemeRepositoryError::Io { stage, .. } => stage,
            _ => ThemeRepositoryStage::DocumentRemove,
        });
        let later_failure =
            match admission.confirm_database(&database, |_| ThemeRepositoryError::LockPoisoned) {
                Ok(()) => later_failure,
                Err(_) => later_failure.or(Some(ThemeRepositoryStage::ConfirmHealth)),
            };
        Ok(ThemeMutationOutcome::Committed(commit(
            evidence.snapshot,
            None,
            later_failure,
        )))
    }

    /// Reconciles only the natural files named by retained bounded evidence.
    pub fn reconcile_theme_mutation(
        &self,
        evidence: &ThemeReconciliationEvidence,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeReconciliationOutcome, ThemeRepositoryError> {
        if evidence.home_id != self.home_id() {
            return Err(ThemeRepositoryError::ForeignEvidence);
        }
        let admission = self.health.admit()?;
        let generation = self.current_database()?;
        let operation = &evidence.operation;
        require_evidence_bounds(
            if operation.document.is_some() { 4 } else { 2 },
            &[
                operation.old.manifest,
                operation.intended_manifest,
                operation.document.as_ref().and_then(|value| value.old),
                operation.document.as_ref().map(|value| value.intended),
            ],
            limits,
        )?;
        let manifest = observe_optional(
            &manifest_path(self.canonical_path()),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        let document = match &operation.document {
            Some(value) => Some(observe_optional(
                &document_path(self.canonical_path(), &value.id),
                limits,
                ThemeRepositoryStage::Verify,
                Some((&self.faults, FaultPoint::BeforeThemeVerification)),
            )?),
            None => None,
        };
        admission.confirm_database(&generation.0, |source| ThemeRepositoryError::Io {
            stage: ThemeRepositoryStage::ConfirmHealth,
            source: io::Error::other(source),
        })?;
        let old_matches = manifest == operation.old.manifest
            && match (&operation.document, document) {
                (Some(expected), Some(actual)) => actual == expected.old,
                (None, None) => true,
                _ => false,
            };
        let new_matches = manifest == operation.intended_manifest
            && match (&operation.document, document) {
                (Some(expected), Some(actual)) => actual == Some(expected.intended),
                (None, None) => true,
                _ => false,
            };
        let snapshot = ThemeRepositorySnapshot {
            home_id: self.home_id(),
            store_instance: generation.1,
            generation: admission.generation(),
            manifest,
        };
        if old_matches {
            return Ok(ThemeReconciliationOutcome::ExactOld(snapshot));
        }
        if new_matches {
            return Ok(ThemeReconciliationOutcome::ExactNew(commit(
                snapshot,
                operation
                    .document
                    .as_ref()
                    .map(|value| (value.id.clone(), value.intended)),
                None,
            )));
        }
        Ok(ThemeReconciliationOutcome::Collision)
    }
}

impl HomeStore {
    fn validate_snapshot(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        limits: ThemeOperationLimits,
    ) -> Result<fjall::Database, ThemeRepositoryError> {
        let generation = self
            .generation
            .read()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(ThemeRepositoryError::LockPoisoned)?;
        let health_generation = self
            .health
            .snapshot()
            .generation()
            .ok_or(ThemeRepositoryError::StaleSnapshot)?;
        if snapshot.home_id != self.home_id()
            || snapshot.store_instance != generation.instance_id.0
            || snapshot.generation != health_generation
        {
            return Err(ThemeRepositoryError::StaleSnapshot);
        }
        let manifest = observe_optional(
            &manifest_path(self.canonical_path()),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        if manifest != snapshot.manifest {
            return Err(ThemeRepositoryError::StaleSnapshot);
        }
        Ok(generation.database.clone())
    }

    fn current_database(&self) -> Result<(fjall::Database, u64), ThemeRepositoryError> {
        let generation = self
            .generation
            .read()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let generation = generation
            .as_ref()
            .ok_or(ThemeRepositoryError::LockPoisoned)?;
        Ok((generation.database.clone(), generation.instance_id.0))
    }

    fn publish_manifest(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        document: Option<(
            &StableThemeFileId,
            Option<ThemeFileIdentity>,
            ThemeFileIdentity,
        )>,
        intended_manifest: ThemeFileIdentity,
        source: &mut dyn Read,
        limits: ThemeOperationLimits,
        _reserved: Option<()>,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let _mutation = self
            .theme_mutation
            .lock()
            .map_err(|_| ThemeRepositoryError::LockPoisoned)?;
        let admission = self.health.admit()?;
        let database = self.validate_snapshot(snapshot, limits)?;
        let outcome =
            self.publish_manifest_inner(snapshot, document, intended_manifest, source, limits)?;
        if let ThemeMutationOutcome::Committed(evidence) = outcome {
            return match admission.confirm_database(&database, |source| {
                io_error(
                    ThemeRepositoryStage::ConfirmHealth,
                    io::Error::other(source),
                )
            }) {
                Ok(()) => Ok(ThemeMutationOutcome::Committed(evidence)),
                Err(_) => Ok(ThemeMutationOutcome::Committed(commit(
                    evidence.snapshot,
                    evidence.document,
                    Some(ThemeRepositoryStage::ConfirmHealth),
                ))),
            };
        }
        Ok(outcome)
    }

    fn publish_manifest_inner(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        document: Option<(
            &StableThemeFileId,
            Option<ThemeFileIdentity>,
            ThemeFileIdentity,
        )>,
        intended_manifest: ThemeFileIdentity,
        source: &mut dyn Read,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        require_evidence_bounds(
            if document.is_some() { 4 } else { 2 },
            &[
                snapshot.manifest,
                Some(intended_manifest),
                document.and_then(|value| value.1),
                document.map(|value| value.2),
            ],
            limits,
        )?;
        let themes = prepare_directories(self.canonical_path())?
            .parent()
            .expect("installed has themes parent")
            .to_path_buf();
        let final_path = manifest_path(self.canonical_path());
        let staged = match stage_source(
            &themes,
            "manifest",
            intended_manifest,
            source,
            limits,
            &self.faults,
            FaultPoint::BeforeThemeManifestWrite,
            FaultPoint::BeforeThemeManifestSync,
            ThemeRepositoryStage::ManifestWrite,
            ThemeRepositoryStage::ManifestSync,
        ) {
            Ok(staged) => staged,
            Err(
                error
                @ (ThemeRepositoryError::SourceMismatch | ThemeRepositoryError::LimitExceeded),
            ) => return Err(error),
            Err(_) => {
                require_identity(&final_path, snapshot.manifest, limits)?;
                return Ok(ThemeMutationOutcome::NotCommitted);
            }
        };
        require_identity(&final_path, snapshot.manifest, limits)?;
        if let Err(_source) = self.faults.check(FaultPoint::BeforeThemeManifestReplace) {
            let _ = fs::remove_file(&staged);
            return Ok(ThemeMutationOutcome::NotCommitted);
        }
        if super::platform::replace(&staged, &final_path).is_err() {
            return self.classify_manifest(snapshot, document, intended_manifest, limits);
        }
        if self
            .faults
            .check(FaultPoint::AfterThemeManifestReplace)
            .is_err()
        {
            return Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                document,
                intended_manifest,
            ));
        }
        if sync_with_fault(
            &themes,
            self.durability_tier(),
            &self.faults,
            FaultPoint::BeforeThemeDirectorySync,
        )
        .is_err()
        {
            return Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                document,
                intended_manifest,
            ));
        }
        let final_manifest = observe_optional(
            &final_path,
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        let final_document = match document {
            Some((id, _, intended)) => {
                observe_optional(
                    &document_path(self.canonical_path(), id),
                    limits,
                    ThemeRepositoryStage::Verify,
                    Some((&self.faults, FaultPoint::BeforeThemeVerification)),
                )? == Some(intended)
            }
            None => true,
        };
        if final_manifest != Some(intended_manifest) || !final_document {
            return Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                document,
                intended_manifest,
            ));
        }
        let new_snapshot = ThemeRepositorySnapshot {
            manifest: Some(intended_manifest),
            ..snapshot.clone()
        };
        Ok(ThemeMutationOutcome::Committed(commit(
            new_snapshot,
            document.map(|value| (value.0.clone(), value.2)),
            None,
        )))
    }

    fn classify_document_failure(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        id: &StableThemeFileId,
        old: Option<ThemeFileIdentity>,
        intended: ThemeFileIdentity,
        limits: ThemeOperationLimits,
        _stage: ThemeRepositoryStage,
        _source: Option<io::Error>,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let actual = observe_optional(
            &document_path(self.canonical_path(), id),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        if actual == old {
            Ok(ThemeMutationOutcome::NotCommitted)
        } else {
            Ok(indeterminate_document(
                self.home_id(),
                snapshot,
                id,
                old,
                intended,
            ))
        }
    }

    fn classify_manifest(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        document: Option<(
            &StableThemeFileId,
            Option<ThemeFileIdentity>,
            ThemeFileIdentity,
        )>,
        intended_manifest: ThemeFileIdentity,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let actual = observe_optional(
            &manifest_path(self.canonical_path()),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        if actual == snapshot.manifest {
            Ok(ThemeMutationOutcome::NotCommitted)
        } else {
            Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                document,
                intended_manifest,
            ))
        }
    }

    fn classify_install(
        &self,
        snapshot: &ThemeRepositorySnapshot,
        id: &StableThemeFileId,
        old: Option<ThemeFileIdentity>,
        intended: ThemeFileIdentity,
        intended_manifest: ThemeFileIdentity,
        limits: ThemeOperationLimits,
    ) -> Result<ThemeMutationOutcome, ThemeRepositoryError> {
        let manifest = observe_optional(
            &manifest_path(self.canonical_path()),
            limits,
            ThemeRepositoryStage::Verify,
            Some((&self.faults, FaultPoint::BeforeThemeVerification)),
        )?;
        if manifest == snapshot.manifest {
            Ok(ThemeMutationOutcome::NotCommitted)
        } else {
            Ok(indeterminate_manifest(
                self.home_id(),
                snapshot,
                Some((id, old, intended)),
                intended_manifest,
            ))
        }
    }
}

fn commit(
    snapshot: ThemeRepositorySnapshot,
    document: Option<(StableThemeFileId, ThemeFileIdentity)>,
    later_failure: Option<ThemeRepositoryStage>,
) -> ThemeCommitEvidence {
    ThemeCommitEvidence {
        snapshot,
        document,
        later_failure,
    }
}

fn indeterminate_document(
    home_id: beryl_model::BerylHomeId,
    snapshot: &ThemeRepositorySnapshot,
    id: &StableThemeFileId,
    old: Option<ThemeFileIdentity>,
    intended: ThemeFileIdentity,
) -> ThemeMutationOutcome {
    ThemeMutationOutcome::Indeterminate(ThemeReconciliationEvidence {
        home_id,
        operation: ThemeOperationEvidence {
            old: snapshot.clone(),
            intended_manifest: snapshot.manifest,
            document: Some(DocumentEvidence {
                id: id.clone(),
                old,
                intended,
            }),
        },
    })
}

fn indeterminate_manifest(
    home_id: beryl_model::BerylHomeId,
    snapshot: &ThemeRepositorySnapshot,
    document: Option<(
        &StableThemeFileId,
        Option<ThemeFileIdentity>,
        ThemeFileIdentity,
    )>,
    intended_manifest: ThemeFileIdentity,
) -> ThemeMutationOutcome {
    ThemeMutationOutcome::Indeterminate(ThemeReconciliationEvidence {
        home_id,
        operation: ThemeOperationEvidence {
            old: snapshot.clone(),
            intended_manifest: Some(intended_manifest),
            document: document.map(|value| DocumentEvidence {
                id: value.0.clone(),
                old: value.1,
                intended: value.2,
            }),
        },
    })
}

fn manifest_path(home: &Path) -> PathBuf {
    home.join("themes").join("manifest.toml")
}

fn document_path(home: &Path, id: &StableThemeFileId) -> PathBuf {
    home.join("themes")
        .join("installed")
        .join(format!("{}.toml", id.as_str()))
}

fn selector_path(home: &Path, selector: &ThemeFileSelector) -> PathBuf {
    match selector {
        ThemeFileSelector::Manifest => manifest_path(home),
        ThemeFileSelector::Document(id) => document_path(home, id),
    }
}

fn prepare_directories(home: &Path) -> Result<PathBuf, ThemeRepositoryError> {
    let themes = home.join("themes");
    let installed = themes.join("installed");
    fs::create_dir_all(&installed)
        .map_err(|source| io_error(ThemeRepositoryStage::DocumentWrite, source))?;
    Ok(installed)
}

#[allow(clippy::too_many_arguments)]
fn stage_source(
    directory: &Path,
    kind: &str,
    intended: ThemeFileIdentity,
    source: &mut dyn Read,
    limits: ThemeOperationLimits,
    faults: &crate::fault::FaultController,
    write_fault: FaultPoint,
    sync_fault: FaultPoint,
    write_stage: ThemeRepositoryStage,
    sync_stage: ThemeRepositoryStage,
) -> Result<PathBuf, ThemeRepositoryError> {
    if intended.length() > limits.max_source_bytes() {
        return Err(ThemeRepositoryError::LimitExceeded);
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|source| io_error(write_stage, io::Error::other(source)))?;
    let path = directory.join(format!(".{kind}-{}.staged", hex::encode(random)));
    faults
        .check(write_fault)
        .map_err(|source| io_error(write_stage, source))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|source| io_error(write_stage, source))?;
    let result = (|| {
        let mut digest = Sha256::new();
        let mut remaining = intended.length();
        let mut buffer = vec![0_u8; limits.io_buffer_bytes().get()];
        while remaining != 0 {
            let count = usize::try_from(remaining.min(buffer.len() as u64))
                .expect("bounded count fits usize");
            let read = source
                .read(&mut buffer[..count])
                .map_err(|source| io_error(write_stage, source))?;
            if read == 0 {
                return Err(ThemeRepositoryError::SourceMismatch);
            }
            file.write_all(&buffer[..read])
                .map_err(|source| io_error(write_stage, source))?;
            digest.update(&buffer[..read]);
            remaining -= read as u64;
        }
        let mut extra = [0_u8; 1];
        if source
            .read(&mut extra)
            .map_err(|source| io_error(write_stage, source))?
            != 0
        {
            return Err(ThemeRepositoryError::SourceMismatch);
        }
        let actual = ThemeFileIdentity::new(intended.length(), digest.finalize().into());
        if actual != intended {
            return Err(ThemeRepositoryError::SourceMismatch);
        }
        faults
            .check(sync_fault)
            .map_err(|source| io_error(sync_stage, source))?;
        file.sync_all()
            .map_err(|source| io_error(sync_stage, source))?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

fn require_identity(
    path: &Path,
    expected: Option<ThemeFileIdentity>,
    limits: ThemeOperationLimits,
) -> Result<(), ThemeRepositoryError> {
    let actual = observe_optional(path, limits, ThemeRepositoryStage::Verify, None)?;
    if actual == expected {
        Ok(())
    } else {
        Err(ThemeRepositoryError::IdentityMismatch)
    }
}

fn observe_required(
    path: &Path,
    limits: ThemeOperationLimits,
    stage: ThemeRepositoryStage,
    fault: Option<(&crate::fault::FaultController, FaultPoint)>,
) -> Result<ThemeFileIdentity, ThemeRepositoryError> {
    observe_optional(path, limits, stage, fault)?.ok_or(ThemeRepositoryError::FileAbsent)
}

fn open_and_observe_required(
    path: &Path,
    limits: ThemeOperationLimits,
    stage: ThemeRepositoryStage,
    fault: Option<(&crate::fault::FaultController, FaultPoint)>,
) -> Result<(File, ThemeFileIdentity), ThemeRepositoryError> {
    if let Some((faults, point)) = fault {
        faults
            .check(point)
            .map_err(|source| io_error(stage, source))?;
    }
    let mut file = open_regular(path, stage)?;
    let length = file
        .metadata()
        .map_err(|source| io_error(stage, source))?
        .len();
    if length > limits.max_source_bytes() {
        return Err(ThemeRepositoryError::LimitExceeded);
    }
    let identity = observe_open_file(&mut file, length, limits, stage)?;
    Ok((file, identity))
}

fn observe_optional(
    path: &Path,
    limits: ThemeOperationLimits,
    stage: ThemeRepositoryStage,
    fault: Option<(&crate::fault::FaultController, FaultPoint)>,
) -> Result<Option<ThemeFileIdentity>, ThemeRepositoryError> {
    if let Some((faults, point)) = fault {
        faults
            .check(point)
            .map_err(|source| io_error(stage, source))?;
    }
    let mut file = match open_regular(path, stage) {
        Ok(file) => file,
        Err(ThemeRepositoryError::Io { source, .. })
            if source.kind() == io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let length = file
        .metadata()
        .map_err(|source| io_error(stage, source))?
        .len();
    if length > limits.max_source_bytes() {
        return Err(ThemeRepositoryError::LimitExceeded);
    }
    observe_open_file(&mut file, length, limits, stage).map(Some)
}

fn observe_open_file(
    file: &mut File,
    length: u64,
    limits: ThemeOperationLimits,
    stage: ThemeRepositoryStage,
) -> Result<ThemeFileIdentity, ThemeRepositoryError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|source| io_error(stage, source))?;
    let mut digest = Sha256::new();
    let mut actual = 0_u64;
    let mut buffer = vec![0_u8; limits.io_buffer_bytes().get()];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| io_error(stage, source))?;
        if read == 0 {
            break;
        }
        actual = actual
            .checked_add(read as u64)
            .ok_or(ThemeRepositoryError::LimitExceeded)?;
        if actual > limits.max_source_bytes() {
            return Err(ThemeRepositoryError::LimitExceeded);
        }
        digest.update(&buffer[..read]);
    }
    if actual != length {
        return Err(ThemeRepositoryError::IdentityMismatch);
    }
    Ok(ThemeFileIdentity::new(actual, digest.finalize().into()))
}

fn open_regular(path: &Path, stage: ThemeRepositoryStage) -> Result<File, ThemeRepositoryError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| io_error(stage, source))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io_error(
            stage,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "theme repository object is not a regular file",
            ),
        ));
    }
    File::open(path).map_err(|source| io_error(stage, source))
}

fn sync_with_fault(
    path: &Path,
    tier: HomeDurabilityTier,
    faults: &crate::fault::FaultController,
    point: FaultPoint,
) -> io::Result<()> {
    faults.check(point)?;
    match super::platform::sync_directory(path) {
        Ok(()) => Ok(()),
        Err(source)
            if tier == HomeDurabilityTier::BestEffort
                && source.kind() == io::ErrorKind::Unsupported =>
        {
            Ok(())
        }
        Err(source) => Err(source),
    }
}

fn require_evidence_bounds(
    count: usize,
    identities: &[Option<ThemeFileIdentity>],
    limits: ThemeOperationLimits,
) -> Result<(), ThemeRepositoryError> {
    if count > limits.max_evidence_files().get() {
        return Err(ThemeRepositoryError::LimitExceeded);
    }
    let encoded = identities
        .iter()
        .flatten()
        .count()
        .checked_mul(40)
        .ok_or(ThemeRepositoryError::LimitExceeded)?;
    if encoded > limits.max_evidence_bytes().get() {
        return Err(ThemeRepositoryError::LimitExceeded);
    }
    Ok(())
}

fn io_error(stage: ThemeRepositoryStage, source: io::Error) -> ThemeRepositoryError {
    ThemeRepositoryError::Io { stage, source }
}
