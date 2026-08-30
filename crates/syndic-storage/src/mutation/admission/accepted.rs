use super::*;
use beryl_home_store::{DomainReader, MutationBuilder};
use beryl_model::AcceptedInputRevision;

use crate::{
    AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextSourceRecord, AcceptedOrderIndexRecord, AcceptedReadySourceRecord,
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord,
    AcceptedRouteLeafState, AcceptedRouteRevision, AcceptedRouteTarget, DraftSubmissionIntent,
    InputGateRecord, InputGateState, NextTurnReason, SelectedPathProof, SyndicRecordError,
    ThreadRecord, TurnKind, codec::Family, domain::SyndicDomain,
};

use super::shared::{AcceptanceBase, AcceptedSpecificRecords, CommonRecords};

pub(super) struct AcceptedRecords {
    common: CommonRecords,
    specific: AcceptedSpecificRecords,
}

pub(super) fn records(
    reader: &DomainReader<'_, SyndicDomain>,
    acceptance: &FirstAcceptance,
    base: AcceptanceBase,
) -> Result<AcceptedRecords, SyndicMutationError> {
    if !matches!(
        base.draft.submission_intent(),
        DraftSubmissionIntent::Ordinary
    ) {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let input_id = acceptance.accepted_input_id();
    if point::<AcceptedInputsFamily>(reader, &input_id)?.is_some()
        || point::<TurnsFamily>(reader, &acceptance.submitted_turn_id())?.is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let ordinal_value = base.gate.accepted_high_water().checked_add(1).ok_or(
        SyndicRecordError::LengthOverflow {
            kind: "accepted-input order",
        },
    )?;
    let ordinal = AcceptedInputOrdinal::new(ordinal_value)?;
    let input_revision = AcceptedInputRevision::new(1)?;
    let gate_revision = base.gate.revision().checked_next()?;
    let logical_bytes = acceptance
        .materialization()
        .content()
        .summary()
        .logical_utf8_bytes();
    let extends_selected = matches!(
        base.gate.state(),
        InputGateState::AwaitingSteering(_) | InputGateState::Steerable(_)
    );
    let prior_head = point::<AcceptedRouteGenerationHeadsFamily>(reader, &base.thread.id())?;
    let current_generation = match (extends_selected, base.gate.selected_route()) {
        (true, Some(proof)) => {
            if prior_head.as_ref().map(|head| head.proof()) != Some(proof) {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
            let generation = required::<AcceptedRouteGenerationsFamily>(
                reader,
                &ThreadRouteKey {
                    thread: base.thread.id(),
                    generation: proof.generation(),
                },
            )?;
            if generation.revision() != proof.revision()
                || generation.target().active_turn_id() != base.gate.state().blocking_turn_id()
            {
                return Err(SyndicMutationError::ActiveSteeringRouteConflict);
            }
            Some(generation)
        }
        (true, None) => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
        (false, _) => None,
    };
    let generation_id = current_generation.as_ref().map_or_else(
        || base.gate.next_route_generation(),
        |generation| Ok(generation.generation()),
    )?;
    if current_generation.is_none()
        && point::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: base.thread.id(),
                generation: generation_id,
            },
        )?
        .is_some()
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    let generation_revision = current_generation
        .as_ref()
        .map_or(Ok(AcceptedRouteRevision::FIRST), |generation| {
            generation.revision().checked_next()
        })?;
    let first_ordinal = current_generation
        .as_ref()
        .and_then(AcceptedRouteGenerationRecord::first_ordinal)
        .or(Some(ordinal));
    if let Some(last) = current_generation
        .as_ref()
        .and_then(AcceptedRouteGenerationRecord::last_ordinal)
        && last.checked_next()? != ordinal
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    let input_count = current_generation
        .as_ref()
        .map_or(0, AcceptedRouteGenerationRecord::input_count)
        .checked_add(1)
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "accepted-route input count",
        })?;
    let next_reason = match base.gate.state() {
        InputGateState::PendingTurn(_) => Some(NextTurnReason::PendingTurn),
        InputGateState::Compacting { .. } => Some(NextTurnReason::Compaction),
        InputGateState::Stopping {
            turn_id,
            operation_nonce,
        } => Some(stopping_next_turn_reason(
            reader,
            &base.gate,
            *turn_id,
            *operation_nonce,
        )?),
        InputGateState::FinalizingHistory(_) => Some(NextTurnReason::TerminalHistory),
        InputGateState::AwaitingTerminal(_) => Some(NextTurnReason::UnknownTerminal),
        InputGateState::AwaitingSteering(_) | InputGateState::Steerable(_) => None,
        InputGateState::Idle => return Err(SyndicMutationError::InputGateStateConflict),
    };
    let is_next_turn = next_reason.is_some();
    let ready_retryable_count = current_generation
        .as_ref()
        .map_or(0, AcceptedRouteGenerationRecord::ready_retryable_count)
        .checked_add(u64::from(!is_next_turn))
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "accepted-route ready count",
        })?;
    let next_turn_count = current_generation
        .as_ref()
        .map_or(0, AcceptedRouteGenerationRecord::next_turn_count)
        .checked_add(u64::from(is_next_turn))
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "accepted-route next-turn count",
        })?;
    let route_live_bytes = current_generation
        .as_ref()
        .map_or(0, AcceptedRouteGenerationRecord::live_logical_utf8_bytes)
        .checked_add(logical_bytes)
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "accepted-route live bytes",
        })?;
    let route_generation = AcceptedRouteGenerationRecord::new(
        base.thread.id(),
        generation_id,
        generation_revision,
        current_generation.as_ref().map_or_else(
            || {
                AcceptedRouteTarget::NextTurn(
                    next_reason.expect("non-active admission has a next reason"),
                )
            },
            |generation| generation.target().clone(),
        ),
        first_ordinal,
        Some(ordinal),
        input_count,
        ready_retryable_count,
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::delivering_count),
        next_turn_count,
        current_generation
            .as_ref()
            .map_or(0, AcceptedRouteGenerationRecord::terminal_count),
        route_live_bytes,
        current_generation.as_ref().map_or(
            0,
            AcceptedRouteGenerationRecord::delivering_logical_utf8_bytes,
        ),
    )?;
    let route_proof = crate::AcceptedRouteHeadProof::new(generation_id, generation_revision);
    let route_head = extends_selected
        .then(|| AcceptedRouteGenerationHeadRecord::new(base.thread.id(), route_proof));
    let route_leaf = AcceptedRouteLeafRecord::new(
        input_id,
        base.thread.id(),
        generation_id,
        ordinal,
        input_revision,
        next_reason.map_or(
            AcceptedRouteLeafState::Routed,
            AcceptedRouteLeafState::NextTurn,
        ),
        AcceptedInputLifecycle::Admitted,
    );
    let (steering_count, next_count) = if extends_selected {
        (
            base.gate.live_steering_count().checked_add(1).ok_or(
                SyndicRecordError::LengthOverflow {
                    kind: "live steering count",
                },
            )?,
            base.gate.live_next_turn_count(),
        )
    } else {
        (
            base.gate.live_steering_count(),
            base.gate.live_next_turn_count().checked_add(1).ok_or(
                SyndicRecordError::LengthOverflow {
                    kind: "live next-turn count",
                },
            )?,
        )
    };
    let live_bytes = base
        .gate
        .live_logical_utf8_bytes()
        .checked_add(logical_bytes)
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "live accepted-input bytes",
        })?;
    let gate = InputGateRecord::new(
        base.thread.id(),
        gate_revision,
        base.gate.state().clone(),
        ordinal_value,
        if current_generation.is_some() {
            base.gate.route_generation_high_water()
        } else {
            Some(generation_id)
        },
        if extends_selected {
            Some(route_proof)
        } else {
            base.gate.selected_route()
        },
        steering_count,
        next_count,
        live_bytes,
    )?;
    let (advanced_image_label_authority, origin_span) =
        super::shared::advance_image_label_authority(
            reader,
            &base.thread,
            base.image_label_authority,
            base.draft_image_label_protection,
            crate::ImageLabelOriginOwner::AcceptedInput(input_id),
            acceptance.asset_reference_set(),
        )?;
    let admission = AcceptedInputAdmissionProof::new(
        acceptance.expected_thread_revision(),
        acceptance.draft_id(),
        acceptance.expected_draft_revision(),
        acceptance.expected_gate_revision(),
        acceptance.next_draft_id(),
    )?;
    let input = AcceptedInputRecord::new(
        input_id,
        base.thread.id(),
        ordinal,
        admission,
        generation_id,
        acceptance.materialization().content(),
        acceptance.asset_reference_set(),
        acceptance.admitted_at(),
    )?;
    let order_index =
        AcceptedOrderIndexRecord::new(base.thread.id(), ordinal, input_id, generation_id);
    let next_source = (route_generation.next_turn_count() > 0).then(|| {
        AcceptedNextSourceRecord::new(
            base.thread.id(),
            generation_id,
            generation_revision,
            first_ordinal.expect("an admitted generation is nonempty"),
            ordinal,
        )
    });
    let ready_source = matches!(gate.state(), InputGateState::Steerable(_)).then(|| {
        AcceptedReadySourceRecord::new(
            base.thread.id(),
            gate.revision(),
            generation_id,
            generation_revision,
            first_ordinal.expect("a steerable admitted generation is nonempty"),
            ordinal,
        )
    });
    let thread_revision = base.thread.revision().checked_next()?;
    let thread = ThreadRecord::new(
        base.thread.id(),
        SelectedPathProof::new(
            base.thread.committed_tail(),
            thread_revision,
            base.thread.selected_path_digest(),
        ),
        acceptance.next_draft_id(),
        base.thread.lineage(),
        base.thread.context_owner_id(),
    );
    let (draft, draft_index) = super::shared::fresh_draft(acceptance, &base, &thread)?;
    let summary = crate::HistorySummaryRecord::new(
        thread.id(),
        base.summary.revision().checked_next()?,
        thread_revision,
        thread.committed_tail(),
        thread.selected_path_digest(),
        base.summary.complete(),
        acceptance.admitted_at(),
    );
    let thread_parent_index = super::shared::thread_parent_index(&thread);
    Ok(AcceptedRecords {
        common: CommonRecords {
            old_draft_id: base.draft.id(),
            thread,
            draft,
            draft_index,
            fresh_root: base.fresh_root,
            fresh_history: base.fresh_history,
            disposed_session: base.disposed_session,
            origin_span,
            advanced_image_label_authority,
            summary,
            gate,
            thread_parent_index,
        },
        specific: AcceptedSpecificRecords {
            input,
            order_index,
            route_head,
            route_generation,
            route_leaf,
            ready_source,
            next_source,
        },
    })
}

impl AcceptedRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        self.common.contribute(mutations)?;
        let records = self.specific;
        mutations.put::<AcceptedInputsCodec>(&records.input.id(), &records.input)?;
        mutations.put::<AcceptedOrderCodec>(
            &ThreadAcceptedKey {
                owner: self.common.thread.id(),
                ordinal: records.input.ordinal(),
            },
            &records.order_index,
        )?;
        if let Some(head) = &records.route_head {
            mutations.put::<AcceptedRouteGenerationHeadsCodec>(&head.thread_id(), head)?;
        }
        mutations.put::<AcceptedRouteGenerationsCodec>(
            &ThreadRouteKey {
                thread: records.route_generation.thread_id(),
                generation: records.route_generation.generation(),
            },
            &records.route_generation,
        )?;
        mutations
            .put::<AcceptedRouteLeavesCodec>(&records.route_leaf.input_id(), &records.route_leaf)?;
        if let Some(source) = &records.ready_source {
            mutations.put::<AcceptedReadySourcesCodec>(
                &ThreadRouteKey {
                    thread: source.thread_id(),
                    generation: source.generation(),
                },
                source,
            )?;
        }
        if let Some(source) = &records.next_source {
            mutations.put::<AcceptedNextSourcesCodec>(
                &ThreadRouteKey {
                    thread: source.thread_id(),
                    generation: source.generation(),
                },
                source,
            )?;
        }
        Ok(())
    }
}

fn stopping_next_turn_reason(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &InputGateRecord,
    turn_id: beryl_model::SyndicTurnId,
    operation_nonce: crate::StopOperationNonce,
) -> Result<NextTurnReason, SyndicMutationError> {
    let stop = required::<StopOperationsFamily>(
        reader,
        &crate::StopOperationId::new(gate.thread_id(), operation_nonce),
    )?;
    if stop.target().turn_id() != turn_id {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    if stop.admission().is_provider_operation() {
        if stop.target().turn_kind()
            != TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        Ok(NextTurnReason::Compaction)
    } else {
        Ok(NextTurnReason::Stop)
    }
}

fn required<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError>
where
    F::Key: std::fmt::Debug,
{
    crate::mutation::required::<F>(reader, key)
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, SyndicMutationError> {
    crate::mutation::point::<F>(reader, key)
}
