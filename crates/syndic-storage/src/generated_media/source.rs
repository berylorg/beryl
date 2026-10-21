use super::{SourceRecord, SourceValue, codec::*};
use crate::read::access::ReadAccess;
use crate::{codec::*, domain::SyndicDomain, *};
use beryl_home_store::{DomainReader, HomeStore};
use beryl_model::*;
use sha2::{Digest, Sha256};

pub type GeneratedMediaReadError = SyndicReadError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneratedMediaSourceState {
    AwaitingCompletion,
    CompletedSource {
        saved_path: Result<Box<str>, GeneratedMediaUnavailableReason>,
    },
}

#[derive(Clone, Debug)]
pub struct GeneratedMediaSourceProof {
    pub(super) revision: DomainRevision,
    pub(super) home: BerylHomeId,
    pub(super) generation: beryl_home_store::HomeGenerationIdentity,
    pub(super) resource: ResourceMetadataRecord,
    pub(super) item: CanonicalItemRecord,
    pub(super) manifest: ContentManifestRecord,
    pub(super) source: SourceRecord,
    pub(super) initial: SourceEventRecord,
    pub(super) completed: Option<SourceEventRecord>,
    pub(super) snapshot: ExecutionSnapshotRecord,
    pub(super) active: ActiveCasTurnRecord,
    fingerprint: [u8; 32],
    state: GeneratedMediaSourceState,
}

impl GeneratedMediaSourceProof {
    pub fn resource_id(&self) -> SyndicResourceId {
        self.resource.id()
    }
    pub fn thread_id(&self) -> SyndicThreadId {
        self.source.thread
    }
    pub fn turn_id(&self) -> SyndicTurnId {
        self.source.turn
    }
    pub fn item_id(&self) -> SyndicItemId {
        self.source.item
    }
    pub fn source_fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn source_incarnation(&self) -> Option<GeneratedSourceIncarnation> {
        self.source.source_incarnation
    }
    pub fn execution_snapshot(&self) -> &ExecutionSnapshotRecord {
        &self.snapshot
    }
    pub fn active_cas_turn(&self) -> &ActiveCasTurnRecord {
        &self.active
    }
    pub fn resource(&self) -> &ResourceMetadataRecord {
        &self.resource
    }
    pub fn item(&self) -> &CanonicalItemRecord {
        &self.item
    }
    pub fn manifest(&self) -> &ContentManifestRecord {
        &self.manifest
    }
    pub fn source_state(&self) -> &GeneratedMediaSourceState {
        &self.state
    }
    pub fn saved_path(&self) -> Option<Result<&str, GeneratedMediaUnavailableReason>> {
        match &self.state {
            GeneratedMediaSourceState::AwaitingCompletion => None,
            GeneratedMediaSourceState::CompletedSource { saved_path } => {
                Some(saved_path.as_deref().map_err(|reason| *reason))
            }
        }
    }
    pub(super) fn require_current(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        use crate::mutation::{point, required};
        if reader.home_id() != self.home
            || required::<ResourcesFamily>(reader, &self.resource_id())? != self.resource
            || self.resource.backing()
                != ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
            || required::<CanonicalItemsFamily>(reader, &self.item_id())? != self.item
            || required::<ContentManifestsFamily>(reader, &self.manifest.id())? != self.manifest
            || point::<SourcesFamily>(reader, &SourceKey::Resource(self.resource_id()))?
                != Some(SourceValue::Source(self.source.clone()))
            || point::<PendingFamily>(reader, &PendingKey(self.source.sequence))?
                != Some(self.resource_id())
            || required::<ExecutionSnapshotsFamily>(reader, &self.snapshot.id())? != self.snapshot
            || point::<SourcesFamily>(reader, &SourceKey::Owner(self.snapshot.id()))?
                != self.source.source_incarnation.map(SourceValue::Owner)
            || required::<ActiveCasTurnsFamily>(reader, &self.snapshot.id())? != self.active
            || required::<SourceEventsFamily>(reader, &event_key(&self.initial))? != self.initial
        {
            return Err(SyndicMutationError::GeneratedMediaResourceCollision);
        }
        if let Some(completed) = &self.completed {
            if required::<SourceEventsFamily>(reader, &event_key(completed))? != *completed {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
        }
        Ok(())
    }
}

pub(super) fn event_digest(event: &SourceEventRecord) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"syndic/generated-source-event/v1\0");
    digest.update(
        SourceEventsFamily::encode_value(event)
            .expect("source-event canonical encoding is infallible"),
    );
    digest.finalize().into()
}

pub(super) fn authenticate_ownership(
    snapshot: &ExecutionSnapshotRecord,
    active: &ActiveCasTurnRecord,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> Result<(), SyndicReadError> {
    if snapshot.kind() != ExecutionSnapshotKind::OrdinaryConversation
        || snapshot.thread_id() != thread
        || snapshot.active_turn_id() != turn
        || active.snapshot_id() != snapshot.id()
        || active.thread_id() != thread
        || active.turn_id() != turn
        || active.binding_revision() != snapshot.binding_revision()
        || active.cas_thread_id() != snapshot.cas_thread_id()
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn authenticate_event(
    event: &SourceEventRecord,
    item: &CanonicalItemRecord,
    active: &ActiveCasTurnRecord,
) -> Result<(), SyndicReadError> {
    let SourceEventPayload::ItemFrame { item_id, frame } = event.payload() else {
        return Err(invalid());
    };
    let source = item.cas_source().ok_or_else(invalid)?;
    if *item_id != item.id()
        || event.turn_id() != item.turn_id()
        || item.provider_kind() != ProviderItemKind::StandaloneImageGeneration
        || event.source() != Some(source.turn())
        || source.turn().thread_id() != active.cas_thread_id()
        || source.turn().turn_id() != active.cas_turn_id()
        || frame.frame().item_id() != source.item_id()
        || frame.frame().item_kind() != ProviderItemKind::StandaloneImageGeneration
        || item.provider_content().is_none_or(|content| {
            content.id() != frame.content().id()
                || content.summary().encoded_bytes() < frame.frame().encoded_end()
        })
    {
        return Err(invalid());
    }
    Ok(())
}

fn event_key(event: &SourceEventRecord) -> TurnEventKey {
    TurnEventKey {
        owner: event.turn_id(),
        ordinal: event.sequence(),
    }
}
pub(super) fn invalid() -> SyndicReadError {
    SyndicReadError::Invariant("generated-media source authority disagrees")
}

impl SyndicStorage {
    pub fn generated_media_source(
        &self,
        store: &HomeStore,
        resource_id: SyndicResourceId,
    ) -> Result<Option<GeneratedMediaSourceProof>, GeneratedMediaReadError> {
        self.generated_media_source_with_access(ReadAccess::Ordinary(store), resource_id)
    }
    pub fn generated_media_source_candidate(
        &self,
        store: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
        resource_id: SyndicResourceId,
    ) -> Result<Option<GeneratedMediaSourceProof>, GeneratedMediaReadError> {
        self.generated_media_source_with_access(ReadAccess::Candidate(store), resource_id)
    }
    fn generated_media_source_with_access(
        &self,
        store: ReadAccess<'_>,
        resource_id: SyndicResourceId,
    ) -> Result<Option<GeneratedMediaSourceProof>, GeneratedMediaReadError> {
        let revision = self.revision_with_access(store)?;
        let result = self.generated_media_source_at_revision(store, resource_id, revision);
        if self.revision_with_access(store)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "generated-media source proof",
            });
        }
        result
    }
    fn generated_media_source_at_revision(
        &self,
        store: ReadAccess<'_>,
        resource_id: SyndicResourceId,
        revision: DomainRevision,
    ) -> Result<Option<GeneratedMediaSourceProof>, GeneratedMediaReadError> {
        let source = match self
            .generated_point_with_access::<SourcesFamily>(store, SourceKey::Resource(resource_id))?
        {
            Some(SourceValue::Source(source)) => source,
            Some(_) => return Err(invalid()),
            None => {
                if self
                    .generated_point_with_access::<ResourcesFamily>(store, resource_id)?
                    .is_some_and(|resource| {
                        matches!(resource.backing(), ResourceBacking::GeneratedMedia(_))
                    })
                {
                    return Err(invalid());
                }
                return Ok(None);
            }
        };
        let resource = self
            .generated_point_with_access::<ResourcesFamily>(store, resource_id)?
            .ok_or_else(invalid)?;
        let item = self
            .generated_point_with_access::<CanonicalItemsFamily>(store, source.item)?
            .ok_or_else(invalid)?;
        let manifest = self
            .generated_point_with_access::<ContentManifestsFamily>(
                store,
                item.provider_content().ok_or_else(invalid)?.id(),
            )?
            .ok_or_else(invalid)?;
        let snapshot = self
            .generated_point_with_access::<ExecutionSnapshotsFamily>(store, source.snapshot)?
            .ok_or_else(invalid)?;
        if self.generated_point_with_access::<SourcesFamily>(
            store,
            SourceKey::Owner(source.snapshot),
        )? != source.source_incarnation.map(SourceValue::Owner)
        {
            return Err(invalid());
        }
        let active = self
            .generated_point_with_access::<ActiveCasTurnsFamily>(store, source.snapshot)?
            .ok_or_else(invalid)?;
        let turn = self
            .generated_point_with_access::<TurnsFamily>(store, source.turn)?
            .ok_or_else(invalid)?;
        let turn_state = self
            .generated_point_with_access::<TurnStatesFamily>(store, source.turn)?
            .ok_or_else(invalid)?;
        if turn.origin_thread_id() != source.thread
            || !matches!(turn_state.dispatch_provenance(), TurnDispatchProvenance::Activated(anchor) | TurnDispatchProvenance::Cancelled(anchor) if anchor.snapshot_id() == source.snapshot && anchor.binding_revision() == snapshot.binding_revision())
        {
            return Err(invalid());
        }
        let pending =
            self.generated_point_with_access::<PendingFamily>(store, PendingKey(source.sequence))?;
        match resource.backing() {
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
                if pending == Some(resource_id) => {}
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(_))
                if pending.is_none() && source.completion.is_some() => {}
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Unavailable(_))
                if pending.is_none() => {}
            _ => return Err(invalid()),
        }
        authenticate_ownership(&snapshot, &active, source.thread, source.turn)?;
        let initial = self
            .generated_point_with_access::<SourceEventsFamily>(
                store,
                TurnEventKey {
                    owner: source.turn,
                    ordinal: SourceEventSequence::new(source.initial_event)
                        .map_err(|_| invalid())?,
                },
            )?
            .ok_or_else(invalid)?;
        authenticate_event(&initial, &item, &active)?;
        if !matches!(initial.payload(), SourceEventPayload::ItemFrame { frame, .. } if frame.frame().ordinal() == ProviderFrameOrdinalV1::FIRST && matches!(frame.observation(), ProviderFrameObservationSummaryV1::Started(_)))
        {
            return Err(invalid());
        }
        let completed = source
            .completion
            .map(|(sequence, _)| {
                self.generated_point_with_access::<SourceEventsFamily>(
                    store,
                    TurnEventKey {
                        owner: source.turn,
                        ordinal: SourceEventSequence::new(sequence).map_err(|_| invalid())?,
                    },
                )?
                .ok_or_else(invalid)
            })
            .transpose()?;
        if source.sequence == 0
            || resource.item_id() != item.id()
            || item.turn_id() != source.turn
            || item.presentation() != &(CanonicalItemPresentation::GeneratedMedia { resource_id })
            || manifest.owner() != Some(item.id())
            || manifest.current_reference() != item.provider_content()
            || !matches!(resource.backing(), ResourceBacking::GeneratedMedia(_))
        {
            return Err(invalid());
        }
        let state = if let Some(event) = &completed {
            authenticate_event(event, &item, &active)?;
            if source.completion != Some((event.sequence().get(), event_digest(event)))
                || event.sequence().get() < source.initial_event
            {
                return Err(invalid());
            }
            let SourceEventPayload::ItemFrame { frame, .. } = event.payload() else {
                return Err(invalid());
            };
            if !matches!(
                frame.observation(),
                ProviderFrameObservationSummaryV1::Completed(_)
            ) || !frame.stream_state().is_complete()
            {
                return Err(invalid());
            }
            GeneratedMediaSourceState::CompletedSource {
                saved_path: super::extraction::saved_path(self, store, frame)?,
            }
        } else {
            GeneratedMediaSourceState::AwaitingCompletion
        };
        let (sequence, digest) = source
            .completion
            .unwrap_or((source.initial_event, event_digest(&initial)));
        let mut hash = Sha256::new();
        hash.update(b"beryl/generated-media-source/v1\0");
        hash.update([1, u8::from(source.completion.is_some())]);
        for id in [
            resource_id.as_bytes(),
            source.thread.as_bytes(),
            source.turn.as_bytes(),
            source.item.as_bytes(),
            source.snapshot.as_bytes(),
        ] {
            hash.update(id);
        }
        hash.update(sequence.to_le_bytes());
        hash.update(digest);
        hash.update([u8::from(source.source_incarnation.is_some())]);
        if let Some(owner) = source.source_incarnation {
            hash.update(owner.as_bytes());
        }
        Ok(Some(GeneratedMediaSourceProof {
            revision,
            home: store.home_id(),
            generation: self.context_generation,
            resource,
            item,
            manifest,
            source,
            initial,
            completed,
            snapshot,
            active,
            fingerprint: hash.finalize().into(),
            state,
        }))
    }

    pub(crate) fn generated_point<F: Family>(
        &self,
        store: &HomeStore,
        key: F::Key,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        self.generated_point_with_access::<F>(ReadAccess::Ordinary(store), key)
    }
    pub(super) fn generated_point_with_access<F: Family>(
        &self,
        store: ReadAccess<'_>,
        key: F::Key,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        self.point_with_access::<F>(
            store,
            key,
            SyndicPointReadLimit::new(F::MAX_VALUE_BYTES + beryl_home_store::RECORD_VERSION_BYTES)
                .expect("family point bound"),
        )
    }
}
