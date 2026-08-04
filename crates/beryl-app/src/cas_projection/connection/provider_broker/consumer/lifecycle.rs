use beryl_home_store::HomeStore;
use beryl_model::{CasItemId, SyndicItemId};
use syndic_storage::{
    CasItemSource, ProviderFrameObservationSummaryV1, ProviderItemKind, ProviderItemLifecycle,
    ProviderObservationFramePreparationPlan, ProviderObservationIssueReason,
    SealedProviderFrameReference, SourceEventSequence, SyndicPointReadLimit, SyndicStorage,
};
use thiserror::Error;

use crate::cas_projection::{
    live_source::LiveSourceTarget,
    provider_identity::{provider_content_id, syndic_item_id},
};

#[allow(
    clippy::large_enum_variant,
    reason = "the fixed-size frame branch carries non-cloneable inspected evidence without a per-observation allocation"
)]
pub(super) enum ResolvedObservation {
    Frame(ResolvedFrame),
    Issue(ProviderObservationIssueReason),
}

pub(super) struct ResolvedFrame {
    item_id: SyndicItemId,
    source: CasItemSource,
    prior: Option<SealedProviderFrameReference>,
}

impl ResolvedFrame {
    pub(super) const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }

    pub(super) fn into_plan(
        self,
        target: &LiveSourceTarget,
        sequence: SourceEventSequence,
    ) -> ProviderObservationFramePreparationPlan {
        match self.prior {
            Some(prior) => ProviderObservationFramePreparationPlan::subsequent(
                self.item_id,
                target.turn_id(),
                self.source,
                sequence,
                prior,
            ),
            None => ProviderObservationFramePreparationPlan::first(
                self.item_id,
                target.turn_id(),
                self.source,
                sequence,
                provider_content_id(
                    target.thread_id(),
                    target.turn_id(),
                    target.source(),
                    self.item_id,
                ),
            ),
        }
    }
}

pub(super) fn resolve(
    store: &HomeStore,
    storage: SyndicStorage,
    target: &LiveSourceTarget,
    lifecycle: ProviderFrameObservationSummaryV1,
    kind: ProviderItemKind,
    cas_item_id: CasItemId,
    limit: SyndicPointReadLimit,
) -> Result<ResolvedObservation, ProviderObservationLifecycleError> {
    let source = CasItemSource::new(target.source().clone(), cas_item_id.clone());
    let expected_item_id = syndic_item_id(
        target.thread_id(),
        target.turn_id(),
        target.source(),
        &cas_item_id,
    );
    let existing = storage.capture_item(store, &source, limit)?;
    if existing
        .as_ref()
        .is_some_and(|existing| existing.item().turn_id() != target.turn_id())
    {
        return Err(ProviderObservationLifecycleError::TurnMismatch);
    }
    let Some(existing) = existing else {
        return Ok(match lifecycle {
            ProviderFrameObservationSummaryV1::Started(_) if kind.permits_completion_only() => {
                ResolvedObservation::Issue(
                    ProviderObservationIssueReason::CompletionOnlyItemStarted,
                )
            }
            ProviderFrameObservationSummaryV1::Started(_) => {
                ResolvedObservation::Frame(ResolvedFrame {
                    item_id: expected_item_id,
                    source,
                    prior: None,
                })
            }
            ProviderFrameObservationSummaryV1::Delta => {
                ResolvedObservation::Issue(ProviderObservationIssueReason::MissingItemStart)
            }
            ProviderFrameObservationSummaryV1::Completed(_) if kind.permits_completion_only() => {
                ResolvedObservation::Frame(ResolvedFrame {
                    item_id: expected_item_id,
                    source,
                    prior: None,
                })
            }
            ProviderFrameObservationSummaryV1::Completed(_) => {
                ResolvedObservation::Issue(ProviderObservationIssueReason::MissingItemStart)
            }
        });
    };
    if existing.item().id() != expected_item_id {
        return Err(ProviderObservationLifecycleError::ItemIdentityMismatch);
    }
    let prior = existing
        .item()
        .provider()
        .cloned()
        .ok_or(ProviderObservationLifecycleError::ProviderReferenceMissing)?;
    let prior_state = prior.stream_state();
    let expected_complete =
        existing.item().provider_lifecycle() == ProviderItemLifecycle::Completed;
    if prior_state.item_id() != &cas_item_id
        || prior_state.kind() != existing.item().provider_kind()
        || prior_state.is_complete() != expected_complete
    {
        return Err(ProviderObservationLifecycleError::ProviderReferenceMismatch);
    }
    if prior_state.is_complete() {
        return Ok(ResolvedObservation::Issue(
            ProviderObservationIssueReason::EventAfterCompletion,
        ));
    }
    if prior_state.kind() != kind {
        return Ok(ResolvedObservation::Issue(
            ProviderObservationIssueReason::ItemKindMismatch,
        ));
    }
    match lifecycle {
        ProviderFrameObservationSummaryV1::Started(_) => Ok(ResolvedObservation::Issue(
            ProviderObservationIssueReason::DuplicateItemStart,
        )),
        ProviderFrameObservationSummaryV1::Delta => Ok(ResolvedObservation::Frame(ResolvedFrame {
            item_id: expected_item_id,
            source,
            prior: Some(prior),
        })),
        ProviderFrameObservationSummaryV1::Completed(completed_at) => {
            let started_at = prior_state
                .started_at()
                .ok_or(ProviderObservationLifecycleError::ProviderReferenceMismatch)?;
            if completed_at < started_at {
                return Ok(ResolvedObservation::Issue(
                    ProviderObservationIssueReason::CompletionBeforeStart,
                ));
            }
            Ok(ResolvedObservation::Frame(ResolvedFrame {
                item_id: expected_item_id,
                source,
                prior: Some(prior),
            }))
        }
    }
}

#[derive(Debug, Error)]
pub(in crate::cas_projection::connection::provider_broker) enum ProviderObservationLifecycleError {
    #[error(transparent)]
    Read(#[from] syndic_storage::SyndicReadError),
    #[error("the provider observation selected a different durable Syndic turn")]
    TurnMismatch,
    #[error("the durable CAS item disagrees with its deterministic Syndic item identity")]
    ItemIdentityMismatch,
    #[error("the durable provider item omitted its sealed predecessor reference")]
    ProviderReferenceMissing,
    #[error("the durable provider item disagrees with its sealed predecessor frontier")]
    ProviderReferenceMismatch,
}
