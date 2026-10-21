use super::{GeneratedMediaSourceProof, codec::*};
use crate::{
    codec::*,
    domain::SyndicDomain,
    mutation::{point, required},
    *,
};
use beryl_home_store::{
    CommandPairRequirement, CommandParticipantKind, DomainMutation, DomainReader, DomainValidator,
    MutationBuilder, MutationContribution, ReconciliationReservation, ValidationContribution,
};
use beryl_model::{AssetId, GeneratedMediaUnavailableReason};
use sha2::{Digest, Sha256};

const PROTOCOL: u64 = 0x67656e6d65646961;

#[derive(Clone)]
struct AssetFacts {
    asset: AssetId,
    media_type: Box<str>,
    dimensions: Option<[u32; 2]>,
}
impl AssetFacts {
    fn new(
        asset: AssetId,
        media_type: &str,
        dimensions: Option<[u32; 2]>,
    ) -> Result<Self, SyndicMutationError> {
        if media_type.is_empty()
            || media_type.len() > 255
            || !media_type.is_ascii()
            || dimensions.is_some_and(|[width, height]| width == 0 || height == 0)
        {
            return Err(conflict());
        }
        Ok(Self {
            asset,
            media_type: media_type.into(),
            dimensions,
        })
    }
}

fn correlation(
    proof: &GeneratedMediaSourceProof,
    operation: u64,
    facts: Option<&AssetFacts>,
    reason: Option<GeneratedMediaUnavailableReason>,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"beryl/generated-media-command/v1\0");
    hash.update(operation.to_le_bytes());
    for id in [
        proof.resource_id().as_bytes(),
        proof.thread_id().as_bytes(),
        proof.turn_id().as_bytes(),
        proof.item_id().as_bytes(),
    ] {
        hash.update(id);
    }
    hash.update(proof.source_fingerprint());
    if let Some(facts) = facts {
        hash.update(facts.asset.digest());
        hash.update(facts.asset.length().get().to_le_bytes());
        hash.update([facts.media_type.len() as u8]);
        hash.update(facts.media_type.as_bytes());
        match facts.dimensions {
            None => hash.update([0]),
            Some([width, height]) => {
                hash.update([1]);
                hash.update(width.to_le_bytes());
                hash.update(height.to_le_bytes());
            }
        }
    } else if let Some(reason) = reason {
        hash.update([reason.code()]);
    }
    hash.finalize().into()
}

fn conflict() -> SyndicMutationError {
    SyndicMutationError::GeneratedMediaResourceCollision
}
struct PrepareValidation {
    proof: GeneratedMediaSourceProof,
    pair: CommandPairRequirement,
}
struct ResolveMutation {
    proof: GeneratedMediaSourceProof,
    disposition: GeneratedMediaResourceDisposition,
    updated_at: SyndicTimestamp,
    pair: CommandPairRequirement,
}
struct Resolution {
    resource: ResourceMetadataRecord,
    sequence: u64,
    state: Option<TurnStateRecord>,
    summary: Option<HistorySummaryRecord>,
    head: Option<TranscriptViewHeadRecord>,
    build: Option<TranscriptBuildRecord>,
}

impl SyndicStorage {
    pub fn generated_media_preparation(
        &self,
        proof: GeneratedMediaSourceProof,
        asset: AssetId,
        media_type: &str,
        dimensions: Option<[u32; 2]>,
    ) -> Result<ValidationContribution, SyndicMutationError> {
        self.authenticate_generated_proof(&proof)?;
        if proof.source_incarnation().is_none() || !matches!(proof.saved_path(), Some(Ok(_))) {
            return Err(conflict());
        }
        let facts = AssetFacts::new(asset, media_type, dimensions)?;
        let pair = CommandPairRequirement::new::<PROTOCOL, 1>(
            "beryl-assets",
            CommandParticipantKind::Mutation,
            correlation(&proof, 1, Some(&facts), None),
        );
        Ok(self
            .handle
            .validation(proof.revision, PrepareValidation { proof, pair }))
    }
    pub fn publish_generated_media(
        &self,
        proof: GeneratedMediaSourceProof,
        asset: AssetId,
        media_type: &str,
        dimensions: Option<[u32; 2]>,
        updated_at: SyndicTimestamp,
    ) -> Result<MutationContribution, SyndicMutationError> {
        self.authenticate_generated_proof(&proof)?;
        if !matches!(proof.saved_path(), Some(Ok(_))) {
            return Err(conflict());
        }
        let facts = AssetFacts::new(asset, media_type, dimensions)?;
        let pair = CommandPairRequirement::new::<PROTOCOL, 2>(
            "beryl-assets",
            CommandParticipantKind::Mutation,
            correlation(&proof, 2, Some(&facts), None),
        );
        Ok(self.handle.contribution(
            proof.revision,
            ResolveMutation {
                proof,
                disposition: GeneratedMediaResourceDisposition::Asset(asset),
                updated_at,
                pair,
            },
        ))
    }
    pub fn unavailable_generated_media(
        &self,
        proof: GeneratedMediaSourceProof,
        reason: GeneratedMediaUnavailableReason,
        updated_at: SyndicTimestamp,
    ) -> Result<MutationContribution, SyndicMutationError> {
        self.authenticate_generated_proof(&proof)?;
        if proof.source_incarnation().is_none()
            && reason != GeneratedMediaUnavailableReason::LostSourceAuthority
        {
            return Err(conflict());
        }
        match proof.saved_path() {
            None if reason != GeneratedMediaUnavailableReason::LostSourceAuthority => {
                return Err(conflict());
            }
            Some(Err(actual))
                if reason != actual
                    && reason != GeneratedMediaUnavailableReason::LostSourceAuthority =>
            {
                return Err(conflict());
            }
            Some(Ok(_))
                if matches!(
                    reason,
                    GeneratedMediaUnavailableReason::ProducerFailed
                        | GeneratedMediaUnavailableReason::MalformedOutput
                ) =>
            {
                return Err(conflict());
            }
            _ => {}
        }
        let pair = CommandPairRequirement::new::<PROTOCOL, 3>(
            "beryl-assets",
            CommandParticipantKind::Validation,
            correlation(&proof, 3, None, Some(reason)),
        );
        Ok(self.handle.contribution(
            proof.revision,
            ResolveMutation {
                proof,
                disposition: GeneratedMediaResourceDisposition::Unavailable(reason),
                updated_at,
                pair,
            },
        ))
    }
    fn authenticate_generated_proof(
        &self,
        proof: &GeneratedMediaSourceProof,
    ) -> Result<(), SyndicMutationError> {
        if self.home_id != proof.home || self.context_generation != proof.generation {
            return Err(conflict());
        }
        Ok(())
    }
}

impl DomainValidator<SyndicDomain> for PrepareValidation {
    type Error = SyndicMutationError;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        Some(self.pair)
    }
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.proof.require_current(reader)
    }
}

impl DomainMutation<SyndicDomain> for ResolveMutation {
    type Error = SyndicMutationError;
    type Prepared = Resolution;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        Some(self.pair)
    }
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.proof.require_current(reader)?;
        let proof = &self.proof;
        let thread = required::<ThreadsFamily>(reader, &proof.thread_id())?;
        let turn = required::<TurnsFamily>(reader, &proof.turn_id())?;
        let current = required::<TurnStatesFamily>(reader, &proof.turn_id())?;
        let summary = required::<HistorySummariesFamily>(reader, &proof.thread_id())?;
        if turn.origin_thread_id() != thread.id()
            || current.turn_id() != turn.id()
            || self.updated_at < current.updated_at()
        {
            return Err(conflict());
        }
        let resource = ResourceMetadataRecord::generated_media(
            proof.resource_id(),
            proof.resource.revision().checked_next()?,
            proof.item_id(),
            self.disposition,
        );
        let index = required::<TurnItemsFamily>(
            reader,
            &TurnItemKey {
                owner: proof.turn_id(),
                ordinal: proof.item.ordinal(),
            },
        )?;
        if index.item_id() != proof.item_id() || index.item_revision() != proof.item.revision() {
            return Err(conflict());
        }
        let current_projection = current_generated_projection(reader, proof)?;
        let can_advance = current_projection
            && matches!(
                self.disposition,
                GeneratedMediaResourceDisposition::Asset(_)
            )
            && current.lifecycle().is_proven_terminal()
            && proof.manifest.lifecycle() == ContentLifecycle::Finalized
            && proof.item.provider_lifecycle() == ProviderItemLifecycle::Completed
            && !proof.item.is_history_blocking()
            && current.finalized_item_count().checked_add(1) == Some(proof.item.ordinal().get());
        let state = if can_advance {
            Some(
                TurnStateRecord::with_capture_frontiers_and_issue(
                    current.turn_id(),
                    current.revision().checked_next()?,
                    current.lifecycle(),
                    current.source_event_count(),
                    current.item_count(),
                    proof.item.ordinal().get(),
                    current.open_item_count(),
                    current.history_blocking_item_count(),
                    current.provider_observation_issue(),
                    current.end_status(),
                    self.updated_at,
                    current.dispatch_provenance(),
                )?
                .preserve_resolved_repair(&current)?,
            )
        } else {
            None
        };
        let selected = crate::mutation::turn_is_on_selected_path(reader, &thread, &turn)?;
        let next_activity = summary.last_activity_at().max(
            state
                .as_ref()
                .map_or(current.updated_at(), TurnStateRecord::updated_at),
        );
        let summary =
            if selected && (summary.complete() || summary.last_activity_at() != next_activity) {
                Some(HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next()?,
                    summary.thread_revision(),
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    false,
                    next_activity,
                ))
            } else {
                None
            };
        let (head, build) = if selected {
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?
        } else {
            (None, None)
        };
        Ok(Resolution {
            resource,
            sequence: proof.source.sequence,
            state,
            summary,
            head,
            build,
        })
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ResourcesCodec>(1)?;
        reservation.reserve_records::<PendingCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ResourcesCodec>(&prepared.resource.id(), &prepared.resource)?;
        mutations.delete::<PendingCodec>(&PendingKey(prepared.sequence))?;
        if let Some(state) = prepared.state {
            mutations.put::<TurnStatesCodec>(&state.turn_id(), &state)?;
        }
        if let Some(summary) = prepared.summary {
            mutations.put::<HistorySummariesCodec>(&summary.thread_id(), &summary)?;
        }
        if let Some(head) = prepared.head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), &head)?;
        }
        if let Some(build) = prepared.build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                &build,
            )?;
        }
        Ok(())
    }
}

fn current_generated_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    proof: &GeneratedMediaSourceProof,
) -> Result<bool, SyndicMutationError> {
    let Some(head) = point::<ItemProjectionHeadsFamily>(reader, &proof.item_id())? else {
        return Ok(false);
    };
    if head.lifecycle() != ProjectionLifecycle::Current
        || head.source_item_revision() != proof.item.revision()
    {
        return Ok(false);
    }
    let set = required::<ItemProjectionSetsFamily>(
        reader,
        &ItemProjectionSetKey {
            item: proof.item_id(),
            generation: head.generation(),
        },
    )?;
    if set.source_item_revision() != proof.item.revision()
        || set.source() != crate::ItemProjectionSource::GeneratedResource(proof.resource_id())
        || set.projection_count() != 1
        || set.resource_count() != 0
        || set.text_state().is_some()
    {
        return Err(conflict());
    }
    Ok(true)
}
