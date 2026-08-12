use std::{
    error::Error,
    fmt,
    io::Cursor,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    DeleteTheme, InstallTheme, InstalledThemeId, SaveTheme, SaveThemeAs, THEME_DOCUMENT_MAX_BYTES,
    ThemeCommandError, ThemeDocument, ThemeDocumentDigest, ThemeDocumentError,
    ThemeDocumentIdentity, ThemeIdentityError, ThemeManifestContentIdentity,
    ThemeManifestDecodeError, ThemeManifestEncodeError, ThemeManifestGeneration,
    ThemeManifestIdentity, ThemeNaturalRepositoryIdentity, ThemeReconciliation,
    ThemeReconciliationDescriptor, ThemeReferenceSnapshot, ThemeRepositoryCommand,
    ThemeRepositoryCommit, ThemeService, ThemeServiceError, UpdateTheme,
    physical::{
        PhysicalThemeLimits, physical_document_identity, physical_file_identity, stable_file_id,
    },
};
use beryl_home_store::{
    HomeStore, StableThemeFileId, ThemeCommitEvidence, ThemeFileIdentity, ThemeFileSelector,
    ThemeMutationOutcome, ThemeReconciliationEvidence, ThemeReconciliationOutcome,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStage,
};

mod transform;

use super::runtime::{
    RetainedOperation, ThemeMutationGuard, ThemeOperationScope, ThemeReconciliationMetric,
    ThemeScopeGateError,
};
use transform::{
    ManifestChange, hash_manifest_transform, manifest_transform_reader, require_member,
};

static NEXT_THEME_RECONCILIATION_OPERATION: AtomicU64 = AtomicU64::new(1);

/// A terminal typed repository-command result.
#[derive(Debug)]
pub enum ThemeRepositoryOperationOutcome {
    NotCommitted {
        failure: ThemeRepositoryOperationFailure,
    },
    Committed {
        publication: ThemeRepositoryCommit,
        later_failure: Option<ThemeRepositoryOperationStage>,
    },
    Indeterminate(Box<ThemeIndeterminateOperation>),
}

/// Bounded state-owned custody for one ambiguous physical publication.
pub struct ThemeIndeterminateOperation {
    operation: NonZeroU64,
    evidence: Option<ThemeReconciliationEvidence>,
    expected: Option<ExpectedPublication>,
}

impl ThemeIndeterminateOperation {
    #[must_use]
    pub const fn operation(&self) -> NonZeroU64 {
        self.operation
    }
}

impl fmt::Debug for ThemeIndeterminateOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ThemeIndeterminateOperation")
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRepositoryOperationFailure {
    PublicationDidNotComplete,
}

/// Supplies the feature-owned, authoritative delete-reference view at admission.
pub trait ThemeReferenceSnapshotProvider {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeReferenceSnapshotUnavailable;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeRepositoryOperationStage {
    Snapshot,
    Verify,
    ReadRange,
    DocumentWrite,
    DocumentSync,
    DocumentReplace,
    DocumentRemove,
    InstalledDirectorySync,
    ManifestWrite,
    ManifestSync,
    ManifestReplace,
    ThemesDirectorySync,
    ConfirmHealth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeCommandFactError {
    ForeignOrStaleHome,
    ManifestMismatch,
    PhysicalManifestMismatch,
    ExpectedRowMismatch,
    ThemeAlreadyInstalled,
    ThemeNotInstalled,
    NewOrderOutOfRange,
    DocumentMismatch,
    DeleteReferenceChanged,
    ScopeGated,
    RetainedOperationCapacityExhausted,
    UnknownReconciliationOperation,
    ReconciliationAlreadyInFlight,
    CollisionScopeClosed,
}

#[derive(Debug)]
pub enum ThemeRepositoryExecutionError {
    InvalidLimits,
    CommandFact(ThemeCommandFactError),
    Command(ThemeCommandError),
    Document(ThemeDocumentError),
    ManifestDecode(ThemeManifestDecodeError),
    ManifestEncode(ThemeManifestEncodeError),
    Repository(ThemeRepositoryError),
    Service(ThemeServiceError),
    Identity(ThemeIdentityError),
    ReferenceSnapshotUnavailable,
}

impl fmt::Display for ThemeRepositoryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeRepositoryExecutionError {}

#[derive(Clone, Debug)]
pub(super) struct ExpectedPublication {
    old_manifest: Option<ThemeFileIdentity>,
    intended_manifest: Option<ThemeFileIdentity>,
    intended_document: Option<(StableThemeFileId, ThemeFileIdentity)>,
    publication: ThemeRepositoryCommit,
    old_natural: ThemeNaturalRepositoryIdentity,
    intended_natural: ThemeNaturalRepositoryIdentity,
}

/// Executes one command against an exact repository observation.
///
/// This is crate-private because `ThemeService` must supply the opaque snapshot
/// and observed physical manifest from the same validated observation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_theme_command(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &ThemeRepositoryCommand,
    max_manifest_source: NonZeroU64,
    references: &dyn ThemeReferenceSnapshotProvider,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    let scope = command_scope(command);
    let guard = service
        .runtime
        .begin_mutation(scope)
        .map_err(map_scope_gate)?;
    let operation = next_operation_id()?;
    validate_context(service, snapshot, manifest, physical_manifest)?;
    let outcome = match command {
        ThemeRepositoryCommand::Install(command) => execute_install(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command,
            max_manifest_source,
            operation,
        ),
        ThemeRepositoryCommand::Rename(command) => execute_manifest_only(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command.expected_manifest(),
            ManifestChange::Rename {
                expected: command.expected().clone(),
                name: command.name().clone(),
            },
            max_manifest_source,
            operation,
        ),
        ThemeRepositoryCommand::Delete(command) => execute_delete(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command,
            max_manifest_source,
            references,
            operation,
        ),
        ThemeRepositoryCommand::Reorder(command) => execute_manifest_only(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command.expected_manifest(),
            ManifestChange::Reorder {
                expected: command.expected().clone(),
                new_order: command.new_order(),
            },
            max_manifest_source,
            operation,
        ),
        ThemeRepositoryCommand::Update(command) => execute_update(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command,
            max_manifest_source,
            operation,
        ),
        ThemeRepositoryCommand::Save(command) => execute_save(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command,
            max_manifest_source,
            operation,
        ),
        ThemeRepositoryCommand::SaveAs(command) => execute_save_as(
            service,
            store,
            snapshot,
            manifest,
            physical_manifest,
            command,
            max_manifest_source,
            operation,
        ),
    }?;
    register_outcome(service, &guard, outcome)
}

/// Reconciles exactly one service-retained ambiguous operation.
pub(crate) fn reconcile_theme_operation(
    service: &ThemeService,
    store: &HomeStore,
    operation: NonZeroU64,
    max_manifest_source: NonZeroU64,
) -> Result<ThemeReconciliation, ThemeRepositoryExecutionError> {
    let limits = combined_limits(max_manifest_source)?;
    let retained = service
        .runtime
        .begin_reconciliation(operation)
        .map_err(map_scope_gate)?;
    let physical = store.reconcile_theme_mutation(&retained.evidence, limits.operations());
    let physical = match physical {
        Ok(value) => value,
        Err(source) => {
            service.runtime.restore_reconciliation(operation, retained);
            return Err(ThemeRepositoryExecutionError::Repository(source));
        }
    };
    let result = match physical {
        ThemeReconciliationOutcome::ExactOld(snapshot) => {
            if snapshot.manifest_identity() != retained.expected.old_manifest {
                Err(ThemeRepositoryExecutionError::CommandFact(
                    ThemeCommandFactError::PhysicalManifestMismatch,
                ))
            } else {
                Ok(ThemeReconciliation::ExactOld)
            }
        }
        ThemeReconciliationOutcome::ExactNew(evidence) => {
            validate_commit_evidence(&evidence, &retained.expected)
                .map(|()| ThemeReconciliation::ExactNew(retained.expected.publication.clone()))
        }
        ThemeReconciliationOutcome::Collision => Ok(ThemeReconciliation::Collision),
    };
    match &result {
        Ok(ThemeReconciliation::ExactOld) => service.runtime.finish_reconciliation(
            operation,
            retained,
            ThemeReconciliationMetric::ExactOld,
        ),
        Ok(ThemeReconciliation::ExactNew(_)) => service.runtime.finish_reconciliation(
            operation,
            retained,
            ThemeReconciliationMetric::ExactNew,
        ),
        Ok(ThemeReconciliation::Collision) => service.runtime.finish_reconciliation(
            operation,
            retained,
            ThemeReconciliationMetric::Collision,
        ),
        Err(_) => service.runtime.restore_reconciliation(operation, retained),
    }
    result
}

fn validate_context(
    service: &ThemeService,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
) -> Result<(), ThemeRepositoryExecutionError> {
    if manifest.home() != service.home()
        || snapshot.home_id() != service.home().home_id()
        || snapshot.generation() != service.home().home_generation()
    {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::ForeignOrStaleHome,
        ));
    }
    if snapshot.manifest_identity() != physical_manifest {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::PhysicalManifestMismatch,
        ));
    }
    let logical_physical = match manifest.content() {
        ThemeManifestContentIdentity::Absent => None,
        ThemeManifestContentIdentity::Present {
            byte_length,
            digest,
        } => Some(physical_file_identity(byte_length, digest)),
    };
    if logical_physical != physical_manifest {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::PhysicalManifestMismatch,
        ));
    }
    Ok(())
}

fn command_scope(command: &ThemeRepositoryCommand) -> ThemeOperationScope {
    match command {
        ThemeRepositoryCommand::Update(command) => {
            ThemeOperationScope::Document(command.target().theme_id().clone())
        }
        ThemeRepositoryCommand::Save(command) => {
            ThemeOperationScope::Document(command.draft().binding().theme_id().clone())
        }
        ThemeRepositoryCommand::Install(_)
        | ThemeRepositoryCommand::Rename(_)
        | ThemeRepositoryCommand::Delete(_)
        | ThemeRepositoryCommand::Reorder(_)
        | ThemeRepositoryCommand::SaveAs(_) => ThemeOperationScope::Repository,
    }
}

fn register_outcome(
    service: &ThemeService,
    guard: &ThemeMutationGuard,
    outcome: ThemeRepositoryOperationOutcome,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    match outcome {
        ThemeRepositoryOperationOutcome::NotCommitted { failure } => {
            service.runtime.note_mutation_not_committed();
            Ok(ThemeRepositoryOperationOutcome::NotCommitted { failure })
        }
        ThemeRepositoryOperationOutcome::Committed {
            publication,
            later_failure,
        } => {
            service.runtime.note_mutation_committed();
            Ok(ThemeRepositoryOperationOutcome::Committed {
                publication,
                later_failure,
            })
        }
        ThemeRepositoryOperationOutcome::Indeterminate(mut operation) => {
            service.runtime.retain(
                operation.operation,
                RetainedOperation {
                    scope: guard.scope().clone(),
                    evidence: operation.evidence.take().ok_or_else(|| {
                        fact(ThemeCommandFactError::UnknownReconciliationOperation)
                    })?,
                    expected: operation.expected.take().ok_or_else(|| {
                        fact(ThemeCommandFactError::UnknownReconciliationOperation)
                    })?,
                },
            );
            Ok(ThemeRepositoryOperationOutcome::Indeterminate(Box::new(
                ThemeIndeterminateOperation {
                    operation: operation.operation,
                    evidence: None,
                    expected: None,
                },
            )))
        }
    }
}

fn map_scope_gate(source: ThemeScopeGateError) -> ThemeRepositoryExecutionError {
    let command_fact = match source {
        ThemeScopeGateError::Gated => ThemeCommandFactError::ScopeGated,
        ThemeScopeGateError::CapacityExhausted => {
            ThemeCommandFactError::RetainedOperationCapacityExhausted
        }
        ThemeScopeGateError::UnknownOperation => {
            ThemeCommandFactError::UnknownReconciliationOperation
        }
        ThemeScopeGateError::ReconciliationBusy => {
            ThemeCommandFactError::ReconciliationAlreadyInFlight
        }
        ThemeScopeGateError::CollisionClosed => ThemeCommandFactError::CollisionScopeClosed,
    };
    fact(command_fact)
}

#[allow(clippy::too_many_arguments)]
fn execute_update(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &UpdateTheme,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    execute_document_only(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        command.target(),
        command.document(),
        max_manifest_source,
        operation,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_save(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &SaveTheme,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    execute_document_only(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        command.draft().binding(),
        command.draft().document(),
        max_manifest_source,
        operation,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_document_only(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    target: &ThemeDocumentIdentity,
    document: &ThemeDocument,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    if target.manifest() != manifest || document.id().is_some_and(|id| id != target.theme_id()) {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::DocumentMismatch,
        ));
    }
    require_member(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        target.theme_id(),
        max_manifest_source,
    )?;
    let bytes = canonical_document(document)?;
    let digest = ThemeDocumentDigest::of_bytes(&bytes);
    let next = service
        .observe_document(
            manifest,
            target.theme_id().clone(),
            Some(target),
            bytes.len() as u64,
            digest,
        )
        .map_err(ThemeRepositoryExecutionError::Service)?;
    let stable = stable_file_id(target.theme_id()).map_err(|_| {
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::DocumentMismatch)
    })?;
    let physical_next = physical_document_identity(&next);
    let publication = ThemeRepositoryCommit::checked(None, vec![next.clone()])
        .map_err(ThemeRepositoryExecutionError::Command)?;
    let expected = expected_publication(
        manifest,
        physical_manifest,
        physical_manifest,
        Some((stable.clone(), physical_next)),
        publication,
        vec![target.clone()],
        vec![next],
    )?;
    let limits = PhysicalThemeLimits::document()
        .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)?;
    let outcome = store
        .replace_theme_document(
            snapshot,
            &stable,
            Some(physical_document_identity(target)),
            physical_next,
            &mut Cursor::new(bytes),
            limits.operations(),
        )
        .map_err(ThemeRepositoryExecutionError::Repository)?;
    map_mutation_outcome(outcome, expected, operation)
}

#[allow(clippy::too_many_arguments)]
fn execute_install(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &InstallTheme,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    require_manifest(command.expected_manifest(), manifest)?;
    let change = ManifestChange::Append {
        id: command.new_id().clone(),
        name: command.name().clone(),
        required_member: None,
    };
    execute_install_like(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        command.new_id(),
        command.document(),
        change,
        max_manifest_source,
        operation,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_save_as(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &SaveThemeAs,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    require_manifest(command.expected_manifest(), manifest)?;
    if command.draft().binding().manifest() != manifest {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::DocumentMismatch,
        ));
    }
    let limits = combined_limits(max_manifest_source)?;
    let binding_file = stable_file_id(command.draft().binding().theme_id())
        .map_err(|_| fact(ThemeCommandFactError::DocumentMismatch))?;
    let observed_binding = store
        .observe_theme_file(
            snapshot,
            &ThemeFileSelector::Document(binding_file),
            limits.operations(),
        )
        .map_err(ThemeRepositoryExecutionError::Repository)?;
    if observed_binding != physical_document_identity(command.draft().binding()) {
        return Err(fact(ThemeCommandFactError::DocumentMismatch));
    }
    let document = ThemeDocument::new(
        Some(command.new_id().clone()),
        command.draft().document().name(),
        command.draft().document().definition().clone(),
    )
    .map_err(ThemeRepositoryExecutionError::Document)?;
    let change = ManifestChange::Append {
        id: command.new_id().clone(),
        name: command.name().clone(),
        required_member: Some(command.draft().binding().theme_id().clone()),
    };
    execute_install_like(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        command.new_id(),
        &document,
        change,
        max_manifest_source,
        operation,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_install_like(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    id: &InstalledThemeId,
    document: &ThemeDocument,
    change: ManifestChange,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    let next_generation = successor_generation(manifest)?;
    let physical_next_manifest = hash_manifest_transform(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_generation,
        change.clone(),
        max_manifest_source,
    )?;
    let next_manifest = logical_manifest(service, next_generation, physical_next_manifest);
    let bytes = canonical_document(document)?;
    let digest = ThemeDocumentDigest::of_bytes(&bytes);
    let next_document = service
        .observe_document(next_manifest, id.clone(), None, bytes.len() as u64, digest)
        .map_err(ThemeRepositoryExecutionError::Service)?;
    let stable = stable_file_id(id).map_err(|_| {
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::DocumentMismatch)
    })?;
    let physical_next_document = physical_document_identity(&next_document);
    let publication =
        ThemeRepositoryCommit::checked(Some(next_manifest), vec![next_document.clone()])
            .map_err(ThemeRepositoryExecutionError::Command)?;
    let expected = expected_publication(
        manifest,
        physical_manifest,
        Some(physical_next_manifest),
        Some((stable.clone(), physical_next_document)),
        publication,
        Vec::new(),
        vec![next_document],
    )?;
    let limits = combined_limits(max_manifest_source)?;
    let mut manifest_reader = manifest_transform_reader(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_manifest.generation(),
        change,
        max_manifest_source,
        limits,
    )?;
    let outcome = store
        .install_theme_document(
            snapshot,
            &stable,
            None,
            physical_next_document,
            &mut Cursor::new(bytes),
            physical_next_manifest,
            &mut manifest_reader,
            limits.operations(),
        )
        .map_err(ThemeRepositoryExecutionError::Repository)?;
    map_mutation_outcome(outcome, expected, operation)
}

#[allow(clippy::too_many_arguments)]
fn execute_manifest_only(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    expected_manifest: ThemeManifestIdentity,
    change: ManifestChange,
    max_manifest_source: NonZeroU64,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    require_manifest(expected_manifest, manifest)?;
    let next_generation = successor_generation(manifest)?;
    let intended = hash_manifest_transform(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_generation,
        change.clone(),
        max_manifest_source,
    )?;
    let next_manifest = logical_manifest(service, next_generation, intended);
    let publication = ThemeRepositoryCommit::checked(Some(next_manifest), Vec::new())
        .map_err(ThemeRepositoryExecutionError::Command)?;
    let expected = expected_publication(
        manifest,
        physical_manifest,
        Some(intended),
        None,
        publication,
        Vec::new(),
        Vec::new(),
    )?;
    let limits = combined_limits(max_manifest_source)?;
    let mut reader = manifest_transform_reader(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_manifest.generation(),
        change,
        max_manifest_source,
        limits,
    )?;
    let outcome = store
        .replace_theme_manifest(snapshot, intended, &mut reader, limits.operations())
        .map_err(ThemeRepositoryExecutionError::Repository)?;
    map_mutation_outcome(outcome, expected, operation)
}

#[allow(clippy::too_many_arguments)]
fn execute_delete(
    service: &ThemeService,
    store: &HomeStore,
    snapshot: &ThemeRepositorySnapshot,
    manifest: ThemeManifestIdentity,
    physical_manifest: Option<ThemeFileIdentity>,
    command: &DeleteTheme,
    max_manifest_source: NonZeroU64,
    references: &dyn ThemeReferenceSnapshotProvider,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    require_manifest(command.expected_manifest(), manifest)?;
    command
        .references()
        .delete_guard(command.target())
        .map_err(|_| {
            ThemeRepositoryExecutionError::CommandFact(
                ThemeCommandFactError::DeleteReferenceChanged,
            )
        })?;
    if command.expected_document().manifest() != manifest
        || command.expected_document().theme_id() != command.target()
    {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::DocumentMismatch,
        ));
    }
    let change = ManifestChange::Delete {
        expected: command.expected().clone(),
    };
    let next_generation = successor_generation(manifest)?;
    let intended = hash_manifest_transform(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_generation,
        change.clone(),
        max_manifest_source,
    )?;
    let next_manifest = logical_manifest(service, next_generation, intended);
    let stable = stable_file_id(command.target()).map_err(|_| {
        ThemeRepositoryExecutionError::CommandFact(ThemeCommandFactError::DocumentMismatch)
    })?;
    let publication = ThemeRepositoryCommit::checked(Some(next_manifest), Vec::new())
        .map_err(ThemeRepositoryExecutionError::Command)?;
    let expected = expected_publication(
        manifest,
        physical_manifest,
        Some(intended),
        None,
        publication,
        Vec::new(),
        Vec::new(),
    )?;
    let limits = combined_limits(max_manifest_source)?;
    let mut reader = manifest_transform_reader(
        service,
        store,
        snapshot,
        manifest,
        physical_manifest,
        next_manifest.generation(),
        change,
        max_manifest_source,
        limits,
    )?;
    let current_references = references
        .current_theme_references()
        .map_err(|_| ThemeRepositoryExecutionError::ReferenceSnapshotUnavailable)?;
    if &current_references != command.references() {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::DeleteReferenceChanged,
        ));
    }
    current_references
        .delete_guard(command.target())
        .map_err(|_| {
            ThemeRepositoryExecutionError::CommandFact(
                ThemeCommandFactError::DeleteReferenceChanged,
            )
        })?;
    let outcome = store
        .delete_theme_document(
            snapshot,
            &stable,
            physical_document_identity(command.expected_document()),
            intended,
            &mut reader,
            limits.operations(),
        )
        .map_err(ThemeRepositoryExecutionError::Repository)?;
    map_mutation_outcome(outcome, expected, operation)
}

fn expected_publication(
    old_manifest: ThemeManifestIdentity,
    old_physical_manifest: Option<ThemeFileIdentity>,
    intended_manifest: Option<ThemeFileIdentity>,
    intended_document: Option<(StableThemeFileId, ThemeFileIdentity)>,
    publication: ThemeRepositoryCommit,
    old_documents: Vec<ThemeDocumentIdentity>,
    intended_documents: Vec<ThemeDocumentIdentity>,
) -> Result<ExpectedPublication, ThemeRepositoryExecutionError> {
    let intended_logical_manifest = publication.manifest().unwrap_or(old_manifest);
    Ok(ExpectedPublication {
        old_manifest: old_physical_manifest,
        intended_manifest,
        intended_document,
        publication,
        old_natural: ThemeNaturalRepositoryIdentity::checked(old_manifest, old_documents)
            .map_err(ThemeRepositoryExecutionError::Command)?,
        intended_natural: ThemeNaturalRepositoryIdentity::checked(
            intended_logical_manifest,
            intended_documents,
        )
        .map_err(ThemeRepositoryExecutionError::Command)?,
    })
}

fn map_mutation_outcome(
    outcome: ThemeMutationOutcome,
    expected: ExpectedPublication,
    operation: NonZeroU64,
) -> Result<ThemeRepositoryOperationOutcome, ThemeRepositoryExecutionError> {
    match outcome {
        ThemeMutationOutcome::NotCommitted => Ok(ThemeRepositoryOperationOutcome::NotCommitted {
            failure: ThemeRepositoryOperationFailure::PublicationDidNotComplete,
        }),
        ThemeMutationOutcome::Committed(evidence) => {
            validate_commit_evidence(&evidence, &expected)?;
            Ok(ThemeRepositoryOperationOutcome::Committed {
                publication: expected.publication,
                later_failure: evidence.later_failure().map(map_stage),
            })
        }
        ThemeMutationOutcome::Indeterminate(evidence) => {
            let descriptor = ThemeReconciliationDescriptor::new(
                operation,
                expected.old_natural.clone(),
                expected.intended_natural.clone(),
            )
            .map_err(ThemeRepositoryExecutionError::Command)?;
            Ok(ThemeRepositoryOperationOutcome::Indeterminate(Box::new(
                ThemeIndeterminateOperation {
                    operation: descriptor.operation(),
                    evidence: Some(evidence),
                    expected: Some(expected),
                },
            )))
        }
    }
}

fn validate_commit_evidence(
    evidence: &ThemeCommitEvidence,
    expected: &ExpectedPublication,
) -> Result<(), ThemeRepositoryExecutionError> {
    let logical_manifest = expected
        .publication
        .manifest()
        .unwrap_or(expected.old_natural.manifest());
    if evidence.snapshot().home_id() != logical_manifest.home().home_id()
        || evidence.snapshot().generation() != logical_manifest.home().home_generation()
    {
        return Err(fact(ThemeCommandFactError::ForeignOrStaleHome));
    }
    if evidence.snapshot().manifest_identity() != expected.intended_manifest {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::PhysicalManifestMismatch,
        ));
    }
    let actual_document = evidence.document();
    let expected_document = expected
        .intended_document
        .as_ref()
        .map(|(id, identity)| (id, *identity));
    if actual_document != expected_document {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::DocumentMismatch,
        ));
    }
    Ok(())
}

fn next_operation_id() -> Result<NonZeroU64, ThemeRepositoryExecutionError> {
    let raw = NEXT_THEME_RECONCILIATION_OPERATION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)?;
    NonZeroU64::new(raw).ok_or(ThemeRepositoryExecutionError::InvalidLimits)
}

fn map_stage(stage: ThemeRepositoryStage) -> ThemeRepositoryOperationStage {
    match stage {
        ThemeRepositoryStage::Snapshot => ThemeRepositoryOperationStage::Snapshot,
        ThemeRepositoryStage::Verify => ThemeRepositoryOperationStage::Verify,
        ThemeRepositoryStage::ReadRange => ThemeRepositoryOperationStage::ReadRange,
        ThemeRepositoryStage::DocumentWrite => ThemeRepositoryOperationStage::DocumentWrite,
        ThemeRepositoryStage::DocumentSync => ThemeRepositoryOperationStage::DocumentSync,
        ThemeRepositoryStage::DocumentReplace => ThemeRepositoryOperationStage::DocumentReplace,
        ThemeRepositoryStage::DocumentRemove => ThemeRepositoryOperationStage::DocumentRemove,
        ThemeRepositoryStage::InstalledDirectorySync => {
            ThemeRepositoryOperationStage::InstalledDirectorySync
        }
        ThemeRepositoryStage::ManifestWrite => ThemeRepositoryOperationStage::ManifestWrite,
        ThemeRepositoryStage::ManifestSync => ThemeRepositoryOperationStage::ManifestSync,
        ThemeRepositoryStage::ManifestReplace => ThemeRepositoryOperationStage::ManifestReplace,
        ThemeRepositoryStage::ThemesDirectorySync => {
            ThemeRepositoryOperationStage::ThemesDirectorySync
        }
        ThemeRepositoryStage::ConfirmHealth => ThemeRepositoryOperationStage::ConfirmHealth,
    }
}

fn require_manifest(
    expected: ThemeManifestIdentity,
    actual: ThemeManifestIdentity,
) -> Result<(), ThemeRepositoryExecutionError> {
    if expected != actual {
        return Err(ThemeRepositoryExecutionError::CommandFact(
            ThemeCommandFactError::ManifestMismatch,
        ));
    }
    Ok(())
}

fn successor_generation(
    manifest: ThemeManifestIdentity,
) -> Result<ThemeManifestGeneration, ThemeRepositoryExecutionError> {
    manifest
        .generation()
        .checked_next()
        .map_err(ThemeRepositoryExecutionError::Identity)
}

fn logical_manifest(
    service: &ThemeService,
    generation: ThemeManifestGeneration,
    physical: ThemeFileIdentity,
) -> ThemeManifestIdentity {
    ThemeManifestIdentity::observed(
        service.home(),
        generation,
        physical.length(),
        ThemeDocumentDigest::from_bytes(physical.sha256()),
    )
}

fn canonical_document(document: &ThemeDocument) -> Result<Vec<u8>, ThemeRepositoryExecutionError> {
    let encoded = document
        .to_canonical_toml()
        .map_err(ThemeRepositoryExecutionError::Document)?
        .into_bytes();
    if encoded.len() > THEME_DOCUMENT_MAX_BYTES {
        return Err(ThemeRepositoryExecutionError::Document(
            ThemeDocumentError::DocumentTooLarge,
        ));
    }
    Ok(encoded)
}

fn combined_limits(
    max_manifest_source: NonZeroU64,
) -> Result<PhysicalThemeLimits, ThemeRepositoryExecutionError> {
    let maximum = max_manifest_source
        .get()
        .max(THEME_DOCUMENT_MAX_BYTES as u64);
    PhysicalThemeLimits::manifest(
        NonZeroU64::new(maximum).ok_or(ThemeRepositoryExecutionError::InvalidLimits)?,
    )
    .map_err(|_| ThemeRepositoryExecutionError::InvalidLimits)
}

fn fact(value: ThemeCommandFactError) -> ThemeRepositoryExecutionError {
    ThemeRepositoryExecutionError::CommandFact(value)
}
