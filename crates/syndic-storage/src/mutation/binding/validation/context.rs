use super::*;
use crate::context_projection::{ContextEstablishment, ContextPreparationKind};

pub(in crate::mutation::binding) fn validate_context_publication(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &crate::PublishValidBinding,
) -> Result<(), SyndicMutationError> {
    let CasLineageProof::ContextAware(proof) = request.lineage else {
        use crate::context_projection::presence::ContextPresence;
        return match crate::context_projection::presence::ContextPresenceAccess::Writer(reader)
            .classify(request.thread_id, request.represented_prefix)
            .map_err(|_| SyndicMutationError::BindingPathConflict)?
        {
            ContextPresence::Required => Err(SyndicMutationError::BindingPathConflict),
            ContextPresence::Absent => Ok(()),
            ContextPresence::Unclassified => {
                let classified = request
                    .context_classification
                    .ok_or(SyndicMutationError::BindingPathConflict)?;
                let gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
                if request.context_provenance_valid
                    && classified.home == reader.home_id()
                    && classified.thread == request.thread_id
                    && classified.selected == request.selected_path
                    && classified.prefix == request.represented_prefix
                    && classified.gate == gate.revision()
                    && classified.summary == crate::ContextSummary::empty()
                {
                    Ok(())
                } else {
                    Err(SyndicMutationError::BindingPathConflict)
                }
            }
        };
    };
    let witness = request
        .context_witness
        .ok_or(SyndicMutationError::BindingPathConflict)?;
    let gate = required::<InputGatesFamily>(reader, &request.thread_id)?;
    let retained = if let ContextPreparationKind::Native {
        source, seed: None, ..
    } = witness.kind
    {
        let source_record = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: source.thread,
                revision: source.revision,
            },
        )?;
        match source_record.state() {
            BindingState::Valid(usable) => {
                source.thread == request.thread_id
                    && usable.cas_thread_id() == &request.cas_thread_id
                    && usable.lineage() == request.lineage
            }
            BindingState::Active(active) => {
                source.thread == request.thread_id
                    && active.usable().cas_thread_id() == &request.cas_thread_id
                    && active.usable().lineage() == request.lineage
            }
            _ => false,
        }
    } else {
        false
    };
    if !request.context_provenance_valid
        || witness.home != reader.home_id()
        || witness.thread != request.thread_id
        || witness.home != proof.home_id()
        || (!retained && witness.prefix != proof.established_prefix())
        || witness.summary != proof.summary()
        || (!retained && witness.projection_digest != proof.projection_digest())
        || witness.selected_path != request.selected_path
        || gate.revision() != witness.expected_gate
        || crate::context_projection::read::projection_digest(
            witness.home,
            request.thread_id,
            proof.established_prefix(),
            proof.summary(),
            &request.execution,
            request.tool_profile,
        ) != proof.projection_digest()
    {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    match (witness.kind, proof.establishment) {
        (ContextPreparationKind::Replay, ContextEstablishment::Replay { injection }) => {
            if request.native_turn_count != CasNativeTurnCount::ZERO
                || injection.completed_at != proof.ordering_floor()
            {
                return Err(SyndicMutationError::BindingStateConflict);
            }
        }
        (
            ContextPreparationKind::Native {
                source,
                cut,
                seed,
                seed_sequence,
                ordering_floor,
            },
            establishment,
        ) => {
            let head = required::<BindingHeadsFamily>(reader, &source.thread)?;
            if Some(head.revision()) != witness.source_head_revision {
                return Err(SyndicMutationError::BindingStateConflict);
            }
            let record = required::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: source.thread,
                    revision: source.revision,
                },
            )?;
            let usable = match record.state() {
                BindingState::Valid(usable) => usable,
                BindingState::Active(active) => active.usable(),
                _ => return Err(SyndicMutationError::BindingStateConflict),
            };
            let reservation = required::<CasThreadIndexFamily>(
                reader,
                &CasThreadKey::Record(usable.cas_thread_id().clone()),
            )?;
            if reservation.thread_id() != source.thread
                || reservation.latest_binding_revision() != source.revision
                || reservation.retired_binding_revision().is_some()
                || usable.execution() != &request.execution
                || usable.tool_profile() != request.tool_profile
                || usable
                    .lineage()
                    .recovered_injection_generation()
                    .is_some_and(|generation| {
                        generation.process() != proof.loaded_generation().process()
                    })
                || (usable.cas_thread_id() == &request.cas_thread_id
                    && usable.lineage() != request.lineage)
            {
                return Err(SyndicMutationError::BindingStateConflict);
            }
            match (seed, establishment) {
                (
                    Some(expected),
                    ContextEstablishment::NativeSeed {
                        source: actual_source,
                        cut: actual_cut,
                        entry,
                        injection,
                    },
                ) if expected == entry
                    && actual_source == source
                    && actual_cut == cut
                    && seed_sequence
                        == Some((injection.utf8_bytes().get(), injection.sequence_digest()))
                    && injection.completed_at >= ordering_floor => {}
                (
                    None,
                    ContextEstablishment::Inherited {
                        source: actual_source,
                        cut: actual_cut,
                        ..
                    },
                ) if actual_source == source
                    && actual_cut == cut
                    && proof.ordering_floor() == ordering_floor => {}
                _ if seed.is_none() && request.lineage == usable.lineage() => {}
                _ => return Err(SyndicMutationError::BindingPathConflict),
            }
        }
        _ => return Err(SyndicMutationError::BindingPathConflict),
    }
    Ok(())
}

pub(in crate::mutation::binding) fn validate_context_activation(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &crate::ActivateBinding,
    usable: &UsableCasBinding,
) -> Result<(), SyndicMutationError> {
    let thread_id = request.thread_id;
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    match (thread.context_owner_id(), usable.lineage()) {
        (
            None | Some(beryl_model::DiscussionContextOwnerId::SubmittedTurn(_)),
            CasLineageProof::ContextAware(proof),
        ) if reader.home_id() == proof.home_id()
            && crate::context_projection::read::projection_digest(
                proof.home_id(),
                thread_id,
                proof.established_prefix(),
                proof.summary(),
                usable.execution(),
                usable.tool_profile(),
            ) == proof.projection_digest() =>
        {
            Ok(())
        }
        (None, CasLineageProof::Native { .. } | CasLineageProof::RecoveredInjection(_)) => {
            use crate::context_projection::presence::ContextPresence;
            match crate::context_projection::presence::ContextPresenceAccess::Writer(reader)
                .classify(thread_id, usable.represented_prefix())
                .map_err(|_| SyndicMutationError::BindingPathConflict)?
            {
                ContextPresence::Required => Err(SyndicMutationError::BindingPathConflict),
                ContextPresence::Absent => Ok(()),
                ContextPresence::Unclassified => {
                    let classified = request
                        .context_classification
                        .ok_or(SyndicMutationError::BindingPathConflict)?;
                    if request.context_provenance_valid
                        && classified.context.home == reader.home_id()
                        && classified.binding_revision == request.expected_binding_revision
                        && classified.context.thread == thread_id
                        && classified.context.selected == request.selected_path
                        && classified.context.prefix == usable.represented_prefix()
                        && classified.context.gate == request.expected_gate_revision
                        && classified.context.summary == crate::ContextSummary::empty()
                    {
                        Ok(())
                    } else {
                        Err(SyndicMutationError::BindingPathConflict)
                    }
                }
            }
        }
        _ => Err(SyndicMutationError::BindingPathConflict),
    }
}
