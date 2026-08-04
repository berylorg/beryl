use beryl_home_store::DomainReader;

use crate::mutation::{point, required};
use crate::{
    CasItemSource, ProviderItemLifecycle, ProviderObservationIssue,
    ProviderObservationIssueEvidenceError, ProviderObservationIssueReason, SourceEventRecord,
    SyndicMutationError, classify_provider_observation_issue, codec::*, domain::SyndicDomain,
    validate_provider_observation_issue_evidence,
};

pub(super) fn publish_provider_observation_issue(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    issue: &ProviderObservationIssue,
) -> Result<(), SyndicMutationError> {
    if event.source() != Some(issue.source()) {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    validate_provider_observation_issue_evidence(reader, issue).map_err(issue_error)?;
    let actual =
        classify_provider_observation_issue(reader, event.turn_id(), event.sequence(), issue)
            .map_err(issue_error)?;
    if actual != Some(issue.reason()) {
        return Err(SyndicMutationError::ProviderObservationIssueConflict);
    }
    validate_current_item_frontier(reader, event, issue)
}

fn validate_current_item_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    issue: &ProviderObservationIssue,
) -> Result<(), SyndicMutationError> {
    let key = CasItemKey::Record(
        issue.source().thread_id().clone(),
        issue.source().turn_id().clone(),
        issue.item_id().clone(),
    );
    if matches!(
        issue.reason(),
        ProviderObservationIssueReason::CompletionOnlyItemStarted
            | ProviderObservationIssueReason::MissingItemStart
    ) {
        return if point::<CasItemIndexFamily>(reader, &key)?.is_none() {
            Ok(())
        } else {
            Err(SyndicMutationError::ProviderObservationIssueConflict)
        };
    }

    let index = required::<CasItemIndexFamily>(reader, &key)?;
    let item = required::<CanonicalItemsFamily>(reader, &index.item_id())?;
    let source = CasItemSource::new(issue.source().clone(), issue.item_id().clone());
    if item.id() != index.item_id()
        || item.turn_id() != event.turn_id()
        || item.revision() != index.item_revision()
        || item.cas_source() != Some(&source)
        || item.provider().is_none()
    {
        return Err(SyndicMutationError::ProviderObservationIssueConflict);
    }
    let expected_lifecycle = match issue.reason() {
        ProviderObservationIssueReason::EventAfterCompletion => ProviderItemLifecycle::Completed,
        ProviderObservationIssueReason::DuplicateItemStart
        | ProviderObservationIssueReason::ItemKindMismatch
        | ProviderObservationIssueReason::CompletionBeforeStart => ProviderItemLifecycle::Started,
        ProviderObservationIssueReason::CompletionOnlyItemStarted
        | ProviderObservationIssueReason::MissingItemStart => unreachable!("handled above"),
    };
    let kind_matches = item.provider_kind() == issue.item_kind();
    let kind_frontier_is_valid = match issue.reason() {
        // Normal frame preparation rejects a completed frontier before comparing item kind.
        ProviderObservationIssueReason::EventAfterCompletion => true,
        ProviderObservationIssueReason::ItemKindMismatch => !kind_matches,
        _ => kind_matches,
    };
    if item.provider_lifecycle() != expected_lifecycle || !kind_frontier_is_valid {
        return Err(SyndicMutationError::ProviderObservationIssueConflict);
    }
    Ok(())
}

fn issue_error(error: ProviderObservationIssueEvidenceError) -> SyndicMutationError {
    match error {
        ProviderObservationIssueEvidenceError::Read(error) => SyndicMutationError::Read(error),
        _ => SyndicMutationError::ProviderObservationIssueConflict,
    }
}
