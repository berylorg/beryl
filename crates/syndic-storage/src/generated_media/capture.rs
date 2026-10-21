use super::{SourceRecord, SourceValue, codec::*};
use crate::{
    codec::*,
    domain::SyndicDomain,
    mutation::{point, required},
    *,
};
use beryl_home_store::{DomainReader, MutationBuilder};
use beryl_model::SyndicThreadId;

pub(crate) struct CaptureEffect {
    resource: beryl_model::SyndicResourceId,
    source: SourceRecord,
    allocated: bool,
}

impl CaptureEffect {
    pub(crate) fn prepare(
        reader: &DomainReader<'_, SyndicDomain>,
        event: &SourceEventRecord,
        thread: SyndicThreadId,
        item: &CanonicalItemRecord,
        born: bool,
    ) -> Result<Option<Self>, SyndicMutationError> {
        let CanonicalItemPresentation::GeneratedMedia { resource_id } = item.presentation() else {
            return Ok(None);
        };
        let current = required::<TurnStatesFamily>(reader, &item.turn_id())?;
        let snapshot = match current.dispatch_provenance() {
            TurnDispatchProvenance::Activated(anchor)
            | TurnDispatchProvenance::Cancelled(anchor) => anchor.snapshot_id(),
            _ => return Err(SyndicMutationError::GeneratedMediaResourceCollision),
        };
        let historical = required::<ExecutionSnapshotsFamily>(reader, &snapshot)?;
        let active = required::<ActiveCasTurnsFamily>(reader, &snapshot)?;
        super::source::authenticate_ownership(&historical, &active, thread, item.turn_id())
            .map_err(|_| SyndicMutationError::GeneratedMediaResourceCollision)?;
        super::source::authenticate_event(event, item, &active)
            .map_err(|_| SyndicMutationError::GeneratedMediaResourceCollision)?;
        let source_incarnation = match point::<SourcesFamily>(reader, &SourceKey::Owner(snapshot))?
        {
            None => None,
            Some(SourceValue::Owner(owner)) => Some(owner),
            _ => return Err(SyndicMutationError::GeneratedMediaResourceCollision),
        };
        let existing = point::<SourcesFamily>(reader, &SourceKey::Resource(*resource_id))?;
        let mut source = if born {
            if existing.is_some() {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
            let last = match point::<SourcesFamily>(reader, &SourceKey::Allocator)? {
                None => 0,
                Some(SourceValue::Allocator(last)) => last,
                _ => return Err(SyndicMutationError::GeneratedMediaResourceCollision),
            };
            let sequence = last
                .checked_add(1)
                .ok_or(SyndicMutationError::GeneratedMediaResourceCollision)?;
            if point::<PendingFamily>(reader, &PendingKey(sequence))?.is_some() {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
            SourceRecord {
                sequence,
                thread,
                turn: item.turn_id(),
                item: item.id(),
                snapshot,
                initial_event: event.sequence().get(),
                source_incarnation,
                completion: None,
            }
        } else {
            let Some(SourceValue::Source(source)) = existing else {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            };
            if source.thread != thread
                || source.turn != item.turn_id()
                || source.item != item.id()
                || source.snapshot != snapshot
                || source.source_incarnation != source_incarnation
            {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
            source
        };
        let SourceEventPayload::ItemFrame { frame, .. } = event.payload() else {
            return Err(SyndicMutationError::GeneratedMediaResourceCollision);
        };
        if matches!(
            frame.observation(),
            ProviderFrameObservationSummaryV1::Completed(_)
        ) {
            let completion = (event.sequence().get(), super::source::event_digest(event));
            if source.completion.is_some_and(|old| old != completion) {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
            source.completion = Some(completion);
        }
        Ok(Some(Self {
            resource: *resource_id,
            source,
            allocated: born,
        }))
    }

    pub(crate) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if self.allocated {
            mutations.put::<SourcesCodec>(
                &SourceKey::Allocator,
                &SourceValue::Allocator(self.source.sequence),
            )?;
            mutations.put::<PendingCodec>(&PendingKey(self.source.sequence), &self.resource)?;
        }
        mutations.put::<SourcesCodec>(
            &SourceKey::Resource(self.resource),
            &SourceValue::Source(self.source),
        )?;
        Ok(())
    }
}
