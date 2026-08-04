use beryl_home_store::{DomainReader, ReadError};
use beryl_model::{CasItemId, SyndicTurnId};

use crate::{
    ProviderFrameObservationSummaryV1, ProviderItemStreamStateV1, ProviderLifecycleTimestampMsV1,
    ProviderObservationBuildLifecycle, ProviderObservationChunkPayload, ProviderObservationControl,
    ProviderObservationIssue, ProviderObservationIssueReason, ProviderObservationValidatorState,
    ProviderScalar, ProviderValueContext, SourceEventPayload, SourceEventSequence,
    codec::{
        ExactCodec, Family, ProviderObservationBuildsFamily, ProviderObservationChunkKey,
        ProviderObservationChunksFamily, SourceEventsFamily, TurnEventKey, family_point_limit,
    },
    domain::SyndicDomain,
};

use super::{CanonicalObservationState, PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES, replay_chunk};
use crate::provider_observation_item_kind;

/// Fixed-resident verification failure for one persisted observation issue reference.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ProviderObservationIssueEvidenceError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("the referenced sealed provider observation is missing")]
    BuildMissing,
    #[error("the provider observation issue reference disagrees with its sealed build")]
    ReferenceMismatch,
    #[error("the referenced provider observation has a missing chunk")]
    ChunkMissing,
    #[error("the referenced provider observation cannot be replayed exactly")]
    ReplayMismatch,
    #[error("the provider observation issue identity evidence disagrees")]
    IdentityMismatch,
    #[error("the provider observation was already referenced by an earlier issue event")]
    AlreadyReferenced,
    #[error("the preceding source-event frontier is incomplete")]
    SourcePrefixMissing,
}

/// Replays and verifies all exact facts retained by a compact issue reference.
pub(crate) fn validate_provider_observation_issue_evidence(
    reader: &DomainReader<'_, SyndicDomain>,
    issue: &ProviderObservationIssue,
) -> Result<(), ProviderObservationIssueEvidenceError> {
    let reference = issue.observation();
    let build = point::<ProviderObservationBuildsFamily>(reader, &reference.identity())?
        .ok_or(ProviderObservationIssueEvidenceError::BuildMissing)?;
    if build.lifecycle() != ProviderObservationBuildLifecycle::Sealed
        || !reference.matches_build(&build)
    {
        return Err(ProviderObservationIssueEvidenceError::ReferenceMismatch);
    }

    let mut validator = ProviderObservationValidatorState::initial();
    let mut canonical = CanonicalObservationState::initial(build.begin());
    let mut item_id = ItemIdCapture::default();
    let mut observed_at = None;
    for ordinal in 1..=build.chunk_count() {
        let key = ProviderObservationChunkKey::new(build.identity(), ordinal);
        let chunk = point::<ProviderObservationChunksFamily>(reader, &key)?
            .ok_or(ProviderObservationIssueEvidenceError::ChunkMissing)?;
        if chunk.identity() != build.identity() || chunk.ordinal() != ordinal {
            return Err(ProviderObservationIssueEvidenceError::ReplayMismatch);
        }
        capture_issue_fact(chunk.payload(), &mut item_id, &mut observed_at)?;
        replay_chunk(build.begin(), &mut validator, &mut canonical, &chunk)
            .map_err(|_| ProviderObservationIssueEvidenceError::ReplayMismatch)?;
    }
    validator
        .finish(build.begin())
        .map_err(|_| ProviderObservationIssueEvidenceError::ReplayMismatch)?;
    if build.validator() != &validator
        || build.canonical_bytes() != canonical.canonical_bytes()
        || build.digest() != canonical.digest()
    {
        return Err(ProviderObservationIssueEvidenceError::ReplayMismatch);
    }

    let item_id = item_id.finish()?;
    let lifecycle = lifecycle_summary(build.begin(), observed_at)?;
    if &item_id != issue.item_id()
        || provider_observation_item_kind(build.begin()) != issue.item_kind()
        || lifecycle != issue.lifecycle()
    {
        return Err(ProviderObservationIssueEvidenceError::IdentityMismatch);
    }
    Ok(())
}

/// Reconstructs the exact item frontier preceding an issue and returns its actual conflict.
pub(crate) fn classify_provider_observation_issue(
    reader: &DomainReader<'_, SyndicDomain>,
    turn_id: SyndicTurnId,
    issue_sequence: SourceEventSequence,
    issue: &ProviderObservationIssue,
) -> Result<Option<ProviderObservationIssueReason>, ProviderObservationIssueEvidenceError> {
    let mut prior = None;
    for ordinal in 1..issue_sequence.get() {
        let sequence = SourceEventSequence::new(ordinal)
            .map_err(|_| ProviderObservationIssueEvidenceError::SourcePrefixMissing)?;
        let event = point::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: turn_id,
                ordinal: sequence,
            },
        )?
        .ok_or(ProviderObservationIssueEvidenceError::SourcePrefixMissing)?;
        match event.payload() {
            SourceEventPayload::ItemFrame { frame, .. }
                if event.source() == Some(issue.source())
                    && frame.frame().item_id() == issue.item_id() =>
            {
                prior = Some(frame.stream_state().clone());
            }
            SourceEventPayload::ProviderObservationIssue(existing)
                if existing.observation().identity() == issue.observation().identity() =>
            {
                return Err(ProviderObservationIssueEvidenceError::AlreadyReferenced);
            }
            _ => {}
        }
    }
    Ok(lifecycle_conflict(prior.as_ref(), issue))
}

fn lifecycle_conflict(
    prior: Option<&ProviderItemStreamStateV1>,
    issue: &ProviderObservationIssue,
) -> Option<ProviderObservationIssueReason> {
    if let Some(prior) = prior {
        if prior.is_complete() {
            return Some(ProviderObservationIssueReason::EventAfterCompletion);
        }
        if prior.kind() != issue.item_kind() {
            return Some(ProviderObservationIssueReason::ItemKindMismatch);
        }
        return match issue.lifecycle() {
            ProviderFrameObservationSummaryV1::Started(_) => {
                Some(ProviderObservationIssueReason::DuplicateItemStart)
            }
            ProviderFrameObservationSummaryV1::Delta => None,
            ProviderFrameObservationSummaryV1::Completed(completed_at) => prior
                .started_at()
                .filter(|started_at| completed_at < *started_at)
                .map(|_| ProviderObservationIssueReason::CompletionBeforeStart),
        };
    }
    match issue.lifecycle() {
        ProviderFrameObservationSummaryV1::Started(_)
            if issue.item_kind().permits_completion_only() =>
        {
            Some(ProviderObservationIssueReason::CompletionOnlyItemStarted)
        }
        ProviderFrameObservationSummaryV1::Started(_) => None,
        ProviderFrameObservationSummaryV1::Delta => {
            Some(ProviderObservationIssueReason::MissingItemStart)
        }
        ProviderFrameObservationSummaryV1::Completed(_)
            if issue.item_kind().permits_completion_only() =>
        {
            None
        }
        ProviderFrameObservationSummaryV1::Completed(_) => {
            Some(ProviderObservationIssueReason::MissingItemStart)
        }
    }
}

struct ItemIdCapture {
    bytes: [u8; PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES],
    length: usize,
    active: bool,
    found: bool,
}

impl Default for ItemIdCapture {
    fn default() -> Self {
        Self {
            bytes: [0; PROVIDER_OBSERVATION_IDENTITY_MAX_BYTES],
            length: 0,
            active: false,
            found: false,
        }
    }
}

impl ItemIdCapture {
    fn fragment(&mut self, bytes: &[u8]) -> Result<(), ProviderObservationIssueEvidenceError> {
        let end = self
            .length
            .checked_add(bytes.len())
            .ok_or(ProviderObservationIssueEvidenceError::IdentityMismatch)?;
        let target = self
            .bytes
            .get_mut(self.length..end)
            .ok_or(ProviderObservationIssueEvidenceError::IdentityMismatch)?;
        target.copy_from_slice(bytes);
        self.length = end;
        Ok(())
    }

    fn finish(self) -> Result<CasItemId, ProviderObservationIssueEvidenceError> {
        if self.active || !self.found {
            return Err(ProviderObservationIssueEvidenceError::IdentityMismatch);
        }
        let text = std::str::from_utf8(&self.bytes[..self.length])
            .map_err(|_| ProviderObservationIssueEvidenceError::IdentityMismatch)?;
        CasItemId::new(text).map_err(|_| ProviderObservationIssueEvidenceError::IdentityMismatch)
    }
}

fn capture_issue_fact(
    payload: &ProviderObservationChunkPayload,
    item_id: &mut ItemIdCapture,
    observed_at: &mut Option<u64>,
) -> Result<(), ProviderObservationIssueEvidenceError> {
    match payload {
        ProviderObservationChunkPayload::Control(ProviderObservationControl::BeginField(
            ProviderValueContext::Field(crate::ProviderField::ItemId),
        )) => {
            if item_id.active || item_id.found {
                return Err(ProviderObservationIssueEvidenceError::IdentityMismatch);
            }
            item_id.active = true;
            item_id.found = true;
        }
        ProviderObservationChunkPayload::Fragment { bytes, .. } if item_id.active => {
            item_id.fragment(bytes)?;
        }
        ProviderObservationChunkPayload::Control(ProviderObservationControl::EndField(
            ProviderValueContext::Field(crate::ProviderField::ItemId),
        )) if item_id.active => item_id.active = false,
        ProviderObservationChunkPayload::Control(ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(crate::ProviderField::LifecycleObservedAt),
            value: ProviderScalar::Unsigned(value),
        }) => {
            if observed_at.replace(*value).is_some() {
                return Err(ProviderObservationIssueEvidenceError::IdentityMismatch);
            }
        }
        _ => {}
    }
    Ok(())
}

fn lifecycle_summary(
    begin: crate::ProviderObservationBegin,
    observed_at: Option<u64>,
) -> Result<ProviderFrameObservationSummaryV1, ProviderObservationIssueEvidenceError> {
    match begin {
        crate::ProviderObservationBegin::Item { lifecycle, .. } => {
            let timestamp = ProviderLifecycleTimestampMsV1::new(
                observed_at.ok_or(ProviderObservationIssueEvidenceError::IdentityMismatch)?,
            );
            Ok(match lifecycle {
                crate::ProviderObservationItemLifecycle::Started => {
                    ProviderFrameObservationSummaryV1::Started(timestamp)
                }
                crate::ProviderObservationItemLifecycle::Completed => {
                    ProviderFrameObservationSummaryV1::Completed(timestamp)
                }
            })
        }
        crate::ProviderObservationBegin::Delta { .. } if observed_at.is_none() => {
            Ok(ProviderFrameObservationSummaryV1::Delta)
        }
        crate::ProviderObservationBegin::Delta { .. } => {
            Err(ProviderObservationIssueEvidenceError::IdentityMismatch)
        }
    }
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, ReadError> {
    reader.point::<ExactCodec<F>>(key, family_point_limit::<F>())
}
