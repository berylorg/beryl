use beryl_home_store::{
    CommandBuildError, CommandError, CommitReceiptError, HomeCommand, HomeHealthState, HomeStore,
    ReadError,
};
use beryl_model::DomainRevision;
use syndic_storage::{
    ContentAppend, ContentBuild, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SyndicCurrentDraft, SyndicMutationError, SyndicPointReadLimit, SyndicReadError,
    SyndicRecordError, SyndicStorage,
};
use thiserror::Error;

use super::{
    DraftKnownUnchanged, DraftSaveOutcome, DraftSaveRequest, DraftSaveToken, DraftSuspensionCause,
};

/// Opaque executor-issued completion and retained diagnostic failure for one exact request.
///
/// The service consumes this value as durable-publication proof. Its diagnostic
/// status alone is never accepted as proof.
#[derive(Debug)]
pub struct DraftSaveExecution {
    token: DraftSaveToken,
    outcome: DraftSaveOutcome,
    failure: Option<DraftSaveExecutionFailure>,
}

impl DraftSaveExecution {
    #[must_use]
    /// Returns the closed diagnostic classification without granting publication authority.
    pub const fn outcome(&self) -> DraftSaveOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&DraftSaveExecutionFailure> {
        self.failure.as_ref()
    }

    pub(crate) fn into_completion(self) -> (DraftSaveToken, DraftSaveOutcome) {
        (self.token, self.outcome)
    }

    #[cfg(test)]
    pub(super) fn test_completion(token: DraftSaveToken, outcome: DraftSaveOutcome) -> Self {
        Self {
            token,
            outcome,
            failure: None,
        }
    }
}

/// Typed diagnostic cause retained alongside the closed state-machine outcome.
#[derive(Debug, Error)]
pub enum DraftSaveExecutionFailure {
    #[error("draft save belongs to another home or healthy generation")]
    StaleBinding,
    #[error("the exact current draft no longer exists")]
    MissingCurrentDraft,
    #[error("the exact thread, draft, or revision binding changed")]
    ChangedCurrentDraft,
    #[error("current draft read failed: {0}")]
    CurrentDraftRead(#[source] SyndicReadError),
    #[error("home revision read failed: {0}")]
    HomeRevisionRead(#[source] ReadError),
    #[error("Syndic revision read failed: {0}")]
    DomainRevisionRead(#[source] ReadError),
    #[error("draft update preparation failed: {0}")]
    Preparation(#[source] SyndicMutationError),
    #[error("draft content preparation failed: {0}")]
    ContentPreparation(#[source] SyndicRecordError),
    #[error("home command construction failed: {0}")]
    CommandBuild(#[source] CommandBuildError),
    #[error("home command failed: {0}")]
    Command(#[source] CommandError),
    #[error("successful receipt was not current: {0}")]
    Receipt(#[source] CommitReceiptError),
    #[error("successful receipt omitted or misreported the Syndic domain revision")]
    ReceiptInvariant,
}

/// Executes one request synchronously; callers run this away from the GPUI thread.
pub fn execute_draft_save(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DraftSaveRequest,
    limit: SyndicPointReadLimit,
) -> DraftSaveExecution {
    let binding = request.binding();
    let health = store.health();
    if store.home_id() != binding.home_id()
        || health.state() != HomeHealthState::Healthy
        || health.generation() != Some(binding.home_generation())
    {
        return suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::StaleBinding,
        );
    }
    let current = match storage.current_draft(store, request.thread_id(), limit) {
        Ok(Some(current)) => current,
        Ok(None) => {
            return suspended(
                request,
                DraftSuspensionCause::RevisionConflict,
                DraftSaveExecutionFailure::MissingCurrentDraft,
            );
        }
        Err(error) => {
            return suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::CurrentDraftRead(error),
            );
        }
    };
    if !matches_request(&current, request) {
        return suspended(
            request,
            DraftSuspensionCause::RevisionConflict,
            DraftSaveExecutionFailure::ChangedCurrentDraft,
        );
    }
    let content = match PreparedContent::composer(request.payload()) {
        Ok(content) => content,
        Err(error) => {
            return known_unchanged(
                request,
                DraftKnownUnchanged::ValidationRejected,
                DraftSaveExecutionFailure::ContentPreparation(error),
            );
        }
    };
    let update = match DraftPayloadUpdate::prepare(&current, &content, request.updated_at()) {
        Ok(DraftPayloadUpdateDecision::Update(update)) => update,
        Ok(DraftPayloadUpdateDecision::NoChange) => {
            return known_unchanged(
                request,
                DraftKnownUnchanged::ValidationRejected,
                DraftSaveExecutionFailure::ChangedCurrentDraft,
            );
        }
        Err(error) => {
            return known_unchanged(
                request,
                DraftKnownUnchanged::ValidationRejected,
                DraftSaveExecutionFailure::Preparation(error),
            );
        }
    };
    if let Err(execution) = ensure_content(store, storage, request, limit, &content) {
        return *execution;
    }
    execute_update(store, storage, request, update)
}

fn ensure_content(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DraftSaveRequest,
    limit: SyndicPointReadLimit,
    content: &PreparedContent,
) -> Result<(), Box<DraftSaveExecution>> {
    loop {
        let manifest = match storage.content_manifest(store, content.id(), limit) {
            Ok(manifest) => manifest.map(|record| record.record().clone()),
            Err(error) => {
                return Err(Box::new(suspended(
                    request,
                    DraftSuspensionCause::AmbiguousStorageFailure,
                    DraftSaveExecutionFailure::CurrentDraftRead(error),
                )));
            }
        };
        let Some(manifest) = manifest else {
            execute_auxiliary_command(store, storage, request, |revision| {
                storage.begin_content(revision, ContentBuild::from_prepared(content))
            })?;
            continue;
        };
        let append = match ContentAppend::prepare(&manifest, content) {
            Ok(append) => append,
            Err(error) => {
                return Err(Box::new(known_unchanged(
                    request,
                    DraftKnownUnchanged::ValidationRejected,
                    DraftSaveExecutionFailure::Preparation(error),
                )));
            }
        };
        let Some(append) = append else {
            return Ok(());
        };
        execute_auxiliary_command(store, storage, request, |revision| {
            storage.append_content(revision, append)
        })?;
    }
}

fn execute_auxiliary_command(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DraftSaveRequest,
    contribution: impl FnOnce(DomainRevision) -> beryl_home_store::MutationContribution,
) -> Result<(), Box<DraftSaveExecution>> {
    let home_revision = store.home_revision().map_err(|error| {
        Box::new(suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::HomeRevisionRead(error),
        ))
    })?;
    let domain_revision = storage.revision(store).map_err(|error| {
        Box::new(suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::DomainRevisionRead(error),
        ))
    })?;
    let mut command = HomeCommand::new(home_revision).with_cancellation(request.cancellation());
    command
        .add(contribution(domain_revision))
        .map_err(|error| {
            Box::new(known_unchanged(
                request,
                DraftKnownUnchanged::ValidationRejected,
                DraftSaveExecutionFailure::CommandBuild(error),
            ))
        })?;
    let receipt = store
        .execute(command)
        .map_err(|error| Box::new(classify_command_error(request, error)))?;
    if receipt.generation() != request.binding().home_generation() {
        return Err(Box::new(suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::ReceiptInvariant,
        )));
    }
    let committed = storage
        .committed_revision(store, &receipt)
        .map_err(|error| {
            Box::new(suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::Receipt(error),
            ))
        })?;
    if committed != Some(next_domain_revision(domain_revision)) {
        return Err(Box::new(suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::ReceiptInvariant,
        )));
    }
    Ok(())
}

fn execute_update(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DraftSaveRequest,
    update: DraftPayloadUpdate,
) -> DraftSaveExecution {
    let home_revision = match store.home_revision() {
        Ok(revision) => revision,
        Err(error) => {
            return suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::HomeRevisionRead(error),
            );
        }
    };
    let domain_revision = match storage.revision(store) {
        Ok(revision) => revision,
        Err(error) => {
            return suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::DomainRevisionRead(error),
            );
        }
    };
    let mut command = HomeCommand::new(home_revision).with_cancellation(request.cancellation());
    if let Err(error) = command.add(storage.update_draft_payload(domain_revision, update)) {
        return known_unchanged(
            request,
            DraftKnownUnchanged::ValidationRejected,
            DraftSaveExecutionFailure::CommandBuild(error),
        );
    }
    let receipt = match store.execute(command) {
        Ok(receipt) => receipt,
        Err(error) => return classify_command_error(request, error),
    };
    if receipt.generation() != request.binding().home_generation() {
        return suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::ReceiptInvariant,
        );
    }
    let committed = match storage.committed_revision(store, &receipt) {
        Ok(Some(revision)) => revision,
        Ok(None) => {
            return suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::ReceiptInvariant,
            );
        }
        Err(error) => {
            return suspended(
                request,
                DraftSuspensionCause::AmbiguousStorageFailure,
                DraftSaveExecutionFailure::Receipt(error),
            );
        }
    };
    if committed != next_domain_revision(domain_revision) {
        return suspended(
            request,
            DraftSuspensionCause::AmbiguousStorageFailure,
            DraftSaveExecutionFailure::ReceiptInvariant,
        );
    }
    match request.expected_revision().checked_next() {
        Ok(revision) => DraftSaveExecution {
            token: request.token(),
            outcome: DraftSaveOutcome::Committed { revision },
            failure: None,
        },
        Err(_) => suspended(
            request,
            DraftSuspensionCause::InvalidCommitRevision,
            DraftSaveExecutionFailure::ReceiptInvariant,
        ),
    }
}

fn matches_request(current: &SyndicCurrentDraft, request: &DraftSaveRequest) -> bool {
    current.thread().id() == request.thread_id()
        && current.thread().revision() == request.binding().thread_revision()
        && current.draft().id() == request.draft_id()
        && current.draft().revision() == request.expected_revision()
}

fn classify_command_error(request: &DraftSaveRequest, error: CommandError) -> DraftSaveExecution {
    let outcome = command_error_outcome(&error);
    DraftSaveExecution {
        token: request.token(),
        outcome,
        failure: Some(DraftSaveExecutionFailure::Command(error)),
    }
}

fn command_error_outcome(error: &CommandError) -> DraftSaveOutcome {
    match error {
        CommandError::CancelledBeforeAdmission => {
            DraftSaveOutcome::KnownUnchanged(DraftKnownUnchanged::CancelledBeforeAdmission)
        }
        CommandError::Conflict { .. } => {
            DraftSaveOutcome::RequiresReconciliation(DraftSuspensionCause::RevisionConflict)
        }
        CommandError::ContributorValidation { source, .. }
            if source
                .downcast_ref::<SyndicMutationError>()
                .is_some_and(|source| {
                    matches!(
                        source,
                        SyndicMutationError::DraftRevisionConflict { .. }
                            | SyndicMutationError::ThreadRevisionConflict { .. }
                            | SyndicMutationError::CurrentDraftConflict
                    )
                }) =>
        {
            DraftSaveOutcome::RequiresReconciliation(DraftSuspensionCause::RevisionConflict)
        }
        CommandError::ContributorValidation { .. }
        | CommandError::ContributorAssembly { .. }
        | CommandError::EmptyContribution { .. }
        | CommandError::EmptyCommand => {
            DraftSaveOutcome::KnownUnchanged(DraftKnownUnchanged::ValidationRejected)
        }
        _ => {
            DraftSaveOutcome::RequiresReconciliation(DraftSuspensionCause::AmbiguousStorageFailure)
        }
    }
}

fn next_domain_revision(revision: DomainRevision) -> DomainRevision {
    revision
        .checked_next()
        .expect("a successful command already advanced this domain revision")
}

fn suspended(
    request: &DraftSaveRequest,
    cause: DraftSuspensionCause,
    failure: DraftSaveExecutionFailure,
) -> DraftSaveExecution {
    DraftSaveExecution {
        token: request.token(),
        outcome: DraftSaveOutcome::RequiresReconciliation(cause),
        failure: Some(failure),
    }
}

fn known_unchanged(
    request: &DraftSaveRequest,
    reason: DraftKnownUnchanged,
    failure: DraftSaveExecutionFailure,
) -> DraftSaveExecution {
    DraftSaveExecution {
        token: request.token(),
        outcome: DraftSaveOutcome::KnownUnchanged(reason),
        failure: Some(failure),
    }
}

#[cfg(test)]
mod tests {
    use beryl_home_store::CommandError;
    use beryl_model::ThreadRevision;
    use syndic_storage::SyndicMutationError;

    use super::command_error_outcome;
    use crate::draft_persistence::{DraftSaveOutcome, DraftSuspensionCause};

    #[test]
    fn thread_revision_contributor_conflict_requires_reconciliation() {
        let error = CommandError::ContributorValidation {
            domain: "syndic",
            source: Box::new(SyndicMutationError::ThreadRevisionConflict {
                expected: ThreadRevision::new(1).expect("revision"),
                current: ThreadRevision::new(2).expect("revision"),
            }),
        };

        assert_eq!(
            command_error_outcome(&error),
            DraftSaveOutcome::RequiresReconciliation(DraftSuspensionCause::RevisionConflict)
        );
    }
}
