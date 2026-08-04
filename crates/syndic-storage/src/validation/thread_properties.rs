use beryl_home_store::DomainReader;

use crate::{
    BindingState, CanonicalItemKind, ContentLifecycle, ThreadArchiveState,
    ThreadCatalogTitleSource,
    catalog_title::{
        DomainTitleSnapshot, HistoryTitlePath, HistoryTitleReadError, derive_history_title,
    },
    codec::*,
    domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::scan::{point, require, scan};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |key, thread| {
        if *key != thread.id() {
            return invariant("thread key and record disagree");
        }
        let execution =
            require::<ThreadExecutionsFamily>(reader, key, "thread execution record is missing")?;
        let attributes =
            require::<ThreadAttributesFamily>(reader, key, "thread attributes record is missing")?;
        let usage = require::<ThreadUsageFamily>(reader, key, "thread usage record is missing")?;
        let catalog = require::<ThreadCatalogSummariesFamily>(
            reader,
            key,
            "thread catalog summary is missing",
        )?;
        let history =
            require::<HistorySummariesFamily>(reader, key, "thread history summary is missing")?;
        if execution.thread_id() != *key
            || attributes.thread_id() != *key
            || usage.thread_id() != *key
            || catalog.thread_id() != *key
        {
            return invariant("thread property key and record disagree");
        }
        if let Some(parent_id) = thread.parent_thread_id() {
            let parent_execution = require::<ThreadExecutionsFamily>(
                reader,
                &parent_id,
                "parent thread execution record is missing",
            )?;
            if execution.execution() != parent_execution.execution() {
                return invariant("child thread execution does not inherit its parent execution");
            }
        }
        validate_archive_shape(thread, attributes.archive())?;
        validate_attributes_revision(&attributes)?;
        if let Some(title) = attributes.generated_title() {
            validate_generated_title(reader, thread, title)?;
        }
        validate_usage(reader, execution.execution(), &usage)?;
        validate_catalog(reader, thread, &execution, &attributes, &history, &catalog)
    })?;
    validate_orphans(reader)
}

fn validate_attributes_revision(
    attributes: &crate::ThreadAttributesRecord,
) -> Result<(), SyndicValidationError> {
    let expected = 1
        + u64::from(attributes.generated_title().is_some())
        + u64::from(attributes.archive().is_archived());
    if attributes.revision().get() != expected {
        return invariant("thread attributes revision has an impossible lifecycle gap");
    }
    Ok(())
}

fn validate_archive_shape(
    thread: &crate::ThreadRecord,
    archive: ThreadArchiveState,
) -> Result<(), SyndicValidationError> {
    match (thread.parent_thread_id(), archive) {
        (None, ThreadArchiveState::Ordinary)
        | (
            Some(_),
            ThreadArchiveState::BranchDiscussionOpen
            | ThreadArchiveState::BranchDiscussionArchived { .. },
        ) => Ok(()),
        _ => invariant("thread lineage and intrinsic archive state disagree"),
    }
}

fn validate_generated_title(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    title: &crate::GeneratedThreadTitle,
) -> Result<(), SyndicValidationError> {
    if title.source_thread_revision() > thread.revision() {
        return invariant("generated title source revision is in the future");
    }
    let turn = require::<TurnsFamily>(
        reader,
        &title.source_turn_id(),
        "generated title source turn is missing",
    )?;
    if turn.kind() != crate::TurnKind::OrdinaryUser
        || turn.origin_thread_id() != thread.id()
        || title.generated_at() < turn.submitted_at()
    {
        return invariant("generated title source turn is ineligible");
    }
    let index = require::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn.id(),
            ordinal: crate::TurnItemOrdinal::FIRST,
        },
        "generated title source item index is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &index.item_id(),
        "generated title source item is missing",
    )?;
    let content = require::<ContentManifestsFamily>(
        reader,
        &title.source_content().id(),
        "generated title source content is missing",
    )?;
    if item.turn_id() != turn.id()
        || item.kind() != CanonicalItemKind::UserInput
        || item.presentation_content() != Some(title.source_content())
        || content.lifecycle() != ContentLifecycle::Sealed
        || content.sealed_reference() != Some(title.source_content())
    {
        return invariant("generated title source content disagrees");
    }
    Ok(())
}

fn validate_usage(
    reader: &DomainReader<'_, SyndicDomain>,
    execution: &beryl_model::ExecutionBinding,
    usage: &crate::ThreadUsageRecord,
) -> Result<(), SyndicValidationError> {
    let Some(observation) = usage.observation() else {
        if usage.revision() != crate::ThreadUsageRevision::FIRST {
            return invariant("empty thread usage has an impossible revision");
        }
        return Ok(());
    };
    if usage.revision() == crate::ThreadUsageRevision::FIRST {
        return invariant("observed thread usage has an impossible initial revision");
    }
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: usage.thread_id(),
            revision: observation.binding_revision(),
        },
        "usage binding record is missing",
    )?;
    if observation.execution() != execution || binding.thread_id() != usage.thread_id() {
        return invariant("thread usage authority disagrees with current binding");
    }
    let usable = match binding.state() {
        BindingState::Valid(usable) => usable,
        BindingState::Active(active) => {
            let snapshot = require::<ExecutionSnapshotsFamily>(
                reader,
                &active.snapshot_id(),
                "active usage execution snapshot is missing",
            )?;
            if snapshot.execution() != observation.execution()
                || snapshot.cas_thread_id() != observation.cas_thread_id()
                || snapshot.binding_revision() != observation.binding_revision()
                || snapshot.loaded_generation() != observation.loaded_generation()
            {
                return invariant("active usage provenance disagrees with execution snapshot");
            }
            active.usable()
        }
        BindingState::Unbound { .. } | BindingState::Stale(_) => {
            return invariant("thread usage names a non-usable current binding");
        }
    };
    if usable.execution() != observation.execution()
        || usable.cas_thread_id() != observation.cas_thread_id()
    {
        return invariant("thread usage route identity disagrees");
    }
    Ok(())
}

fn validate_catalog(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    execution: &crate::ThreadExecutionRecord,
    attributes: &crate::ThreadAttributesRecord,
    history: &crate::HistorySummaryRecord,
    catalog: &crate::ThreadCatalogSummaryRecord,
) -> Result<(), SyndicValidationError> {
    if catalog.execution() != execution.execution()
        || catalog.parent_thread_id() != thread.parent_thread_id()
        || catalog.lineage_depth() != thread.lineage_depth()
        || catalog.lineage_digest() != thread.lineage_digest()
    {
        return invariant("catalog summary immutable facts disagree with canonical sources");
    }
    let sources = catalog.sources();
    if sources.attributes_revision() > attributes.revision()
        || sources.history_summary_revision() > history.revision()
        || sources.history_thread_revision() > history.thread_revision()
        || sources.thread_revision() > thread.revision()
    {
        return invariant("catalog summary source witness is in the future");
    }
    let history_current = sources.history_summary_revision() == history.revision();
    if history_current
        && (sources.history_thread_revision() != history.thread_revision()
            || sources.history_selected_path_digest() != history.selected_path_digest())
    {
        return invariant("current catalog history provenance disagrees with its source");
    }
    let attributes_current = sources.attributes_revision() == attributes.revision();
    if attributes_current {
        if catalog.archive() != attributes.archive() {
            return invariant("current catalog archive disagrees with canonical attributes");
        }
        match (attributes.generated_title(), catalog.title()) {
            (Some(generated), Some(title))
                if title.source() == ThreadCatalogTitleSource::Generated
                    && title.text() == generated.text() => {}
            (Some(_), _) => {
                return invariant(
                    "current catalog summary does not preserve generated-title precedence",
                );
            }
            (None, Some(title)) if title.source() == ThreadCatalogTitleSource::Generated => {
                return invariant("catalog generated title has no canonical attributes source");
            }
            (None, None | Some(_)) => {}
        }
    }
    if history_current
        && (catalog.last_activity_at() != history.last_activity_at()
            || catalog.complete() != history.complete())
    {
        return invariant("current catalog history payload disagrees with canonical summary");
    }
    let all_sources_current = attributes_current
        && history_current
        && sources.history_thread_revision() == history.thread_revision()
        && sources.history_selected_path_digest() == history.selected_path_digest()
        && sources.thread_revision() == thread.revision();
    if all_sources_current {
        validate_exact_current_title(reader, thread, attributes, catalog)?;
    }
    Ok(())
}

fn validate_exact_current_title(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    attributes: &crate::ThreadAttributesRecord,
    catalog: &crate::ThreadCatalogSummaryRecord,
) -> Result<(), SyndicValidationError> {
    if let Some(generated) = attributes.generated_title() {
        return match catalog.title() {
            Some(title)
                if title.source() == ThreadCatalogTitleSource::Generated
                    && title.text() == generated.text() =>
            {
                Ok(())
            }
            _ => invariant("current catalog title does not equal its generated source"),
        };
    }
    let expected = derive_history_title(
        &DomainTitleSnapshot { reader },
        thread,
        HistoryTitlePath::ThreadRules,
    )
    .map_err(map_title_error)?;
    if catalog.title() != expected.as_ref() {
        return invariant("current catalog title does not equal its history-derived source");
    }
    Ok(())
}

fn map_title_error(error: HistoryTitleReadError) -> SyndicValidationError {
    match error {
        HistoryTitleReadError::Read(source) => SyndicValidationError::Read(source),
        HistoryTitleReadError::Invariant(message) => SyndicValidationError::Invariant(message),
    }
}

fn validate_orphans(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ThreadExecutionsFamily>(reader, |key, _| require_thread(reader, key))?;
    scan::<ThreadAttributesFamily>(reader, |key, _| require_thread(reader, key))?;
    scan::<ThreadUsageFamily>(reader, |key, _| require_thread(reader, key))?;
    scan::<ThreadCatalogSummariesFamily>(reader, |key, _| require_thread(reader, key))
}

fn require_thread(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &beryl_model::SyndicThreadId,
) -> Result<(), SyndicValidationError> {
    if point::<ThreadsFamily>(reader, key)?.is_none() {
        return invariant("orphan thread property record");
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
