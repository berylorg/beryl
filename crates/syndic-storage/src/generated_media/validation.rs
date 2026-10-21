use super::{SourceValue, codec::*};
use crate::{
    codec::*,
    domain::SyndicDomain,
    error::SyndicValidationError,
    validation::scan::{point, require, scan},
    *,
};
use beryl_home_store::DomainReader;
use std::collections::BTreeSet;

fn invalid() -> SyndicValidationError {
    SyndicValidationError::Invariant("generated-media source/index association disagrees")
}

pub(crate) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let high_water = match point::<SourcesFamily>(reader, &SourceKey::Allocator)? {
        None => 0,
        Some(SourceValue::Allocator(high)) => high,
        _ => return Err(invalid()),
    };
    let mut sequences = BTreeSet::new();
    scan::<SourcesFamily>(reader, |key, value| {
        let (resource_id, source) = match (key, value) {
            (SourceKey::Allocator, SourceValue::Allocator(_)) => return Ok(()),
            (SourceKey::Owner(snapshot_id), SourceValue::Owner(_)) => {
                let snapshot = require::<ExecutionSnapshotsFamily>(
                    reader,
                    snapshot_id,
                    "generated source owner snapshot missing",
                )?;
                if snapshot.id() != *snapshot_id
                    || snapshot.kind() != ExecutionSnapshotKind::OrdinaryConversation
                {
                    return Err(invalid());
                }
                return Ok(());
            }
            (SourceKey::Resource(resource), SourceValue::Source(source)) => (*resource, source),
            _ => return Err(invalid()),
        };
        if source.sequence == 0
            || source.sequence > high_water
            || !sequences.insert(source.sequence)
        {
            return Err(invalid());
        }
        let resource =
            require::<ResourcesFamily>(reader, &resource_id, "generated resource missing")?;
        let item = require::<CanonicalItemsFamily>(reader, &source.item, "generated item missing")?;
        let turn = require::<TurnsFamily>(reader, &source.turn, "generated turn missing")?;
        let state = require::<TurnStatesFamily>(reader, &source.turn, "generated state missing")?;
        let snapshot = require::<ExecutionSnapshotsFamily>(
            reader,
            &source.snapshot,
            "generated execution snapshot missing",
        )?;
        let active = require::<ActiveCasTurnsFamily>(
            reader,
            &source.snapshot,
            "generated active CAS turn missing",
        )?;
        if resource.item_id() != item.id()
            || item.turn_id() != source.turn
            || turn.origin_thread_id() != source.thread
            || item.presentation() != &(CanonicalItemPresentation::GeneratedMedia { resource_id })
            || !matches!(state.dispatch_provenance(), TurnDispatchProvenance::Activated(anchor) | TurnDispatchProvenance::Cancelled(anchor) if anchor.snapshot_id() == source.snapshot && anchor.binding_revision() == snapshot.binding_revision())
        {
            return Err(invalid());
        }
        super::source::authenticate_ownership(&snapshot, &active, source.thread, source.turn)
            .map_err(|_| invalid())?;
        if point::<SourcesFamily>(reader, &SourceKey::Owner(source.snapshot))?
            != source.source_incarnation.map(SourceValue::Owner)
        {
            return Err(invalid());
        }
        let initial = require::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: source.turn,
                ordinal: SourceEventSequence::new(source.initial_event).map_err(|_| invalid())?,
            },
            "generated initial source event missing",
        )?;
        super::source::authenticate_event(&initial, &item, &active).map_err(|_| invalid())?;
        if !matches!(initial.payload(), SourceEventPayload::ItemFrame { frame, .. } if frame.frame().ordinal() == ProviderFrameOrdinalV1::FIRST && matches!(frame.observation(), ProviderFrameObservationSummaryV1::Started(_)))
        {
            return Err(invalid());
        }
        if let Some((sequence, digest)) = source.completion {
            let completed = require::<SourceEventsFamily>(
                reader,
                &TurnEventKey {
                    owner: source.turn,
                    ordinal: SourceEventSequence::new(sequence).map_err(|_| invalid())?,
                },
                "generated completed source event missing",
            )?;
            super::source::authenticate_event(&completed, &item, &active).map_err(|_| invalid())?;
            if super::source::event_digest(&completed) != digest
                || sequence < source.initial_event
                || !matches!(completed.payload(), SourceEventPayload::ItemFrame { frame, .. } if matches!(frame.observation(), ProviderFrameObservationSummaryV1::Completed(_)) && frame.stream_state().is_complete())
            {
                return Err(invalid());
            }
        } else if matches!(initial.payload(), SourceEventPayload::ItemFrame { frame, .. } if matches!(frame.observation(), ProviderFrameObservationSummaryV1::Completed(_)))
        {
            return Err(invalid());
        }
        let pending = point::<PendingFamily>(reader, &PendingKey(source.sequence))?;
        match resource.backing() {
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::PendingAsset)
                if pending == Some(resource_id) => {}
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(_))
                if pending.is_none() && source.completion.is_some() => {}
            ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Unavailable(_))
                if pending.is_none() => {}
            _ => return Err(invalid()),
        }
        Ok(())
    })?;
    scan::<PendingFamily>(reader, |key, resource| {
        if key.0 == 0 || key.0 > high_water {
            return Err(invalid());
        }
        match require::<SourcesFamily>(
            reader,
            &SourceKey::Resource(*resource),
            "pending generated source missing",
        )? {
            SourceValue::Source(source) if source.sequence == key.0 => Ok(()),
            _ => Err(invalid()),
        }
    })?;
    scan::<ResourcesFamily>(reader, |key, resource| {
        if matches!(resource.backing(), ResourceBacking::GeneratedMedia(_))
            && !matches!(
                point::<SourcesFamily>(reader, &SourceKey::Resource(*key))?,
                Some(SourceValue::Source(_))
            )
        {
            return Err(invalid());
        }
        Ok(())
    })
}
