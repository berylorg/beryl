use super::*;

pub(in crate::mutation::binding) fn validate_native_recovery(
    reader: &DomainReader<'_, SyndicDomain>,
    basis: crate::NativeProjectionBasis,
    request: &crate::PublishValidBinding,
) -> Result<(), SyndicMutationError> {
    use crate::native_projection::recovery::RecoveryOperation;
    let Some(anchor) = basis.recovery else {
        return Ok(());
    };
    let thread = required::<ThreadsFamily>(reader, &basis.thread_id())?;
    if SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    ) != basis.selected_path()
    {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    let gate = required::<InputGatesFamily>(reader, &basis.thread_id())?;
    if gate.revision() != anchor.gate_revision {
        return Err(SyndicMutationError::InputGateRevisionConflict {
            expected: anchor.gate_revision,
            current: gate.revision(),
        });
    }
    match gate.state() {
        crate::InputGateState::Idle => {}
        crate::InputGateState::PendingTurn(turn) if Some(*turn) == basis.selected_path().tail() => {
        }
        _ => return Err(SyndicMutationError::InputGateStateConflict),
    }
    if request.native_turn_count != anchor.native_turn_count {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    let source = if let Some(source) = anchor.source {
        let source_thread = required::<ThreadsFamily>(reader, &source.thread_id)?;
        let source_head = required::<BindingHeadsFamily>(reader, &source.thread_id)?;
        if SelectedPathProof::new(
            source_thread.committed_tail(),
            source_thread.revision(),
            source_thread.selected_path_digest(),
        ) != source.current_selected_path
            || source_head.revision() != source.current_binding_revision
        {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        let binding = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: source.thread_id,
                revision: source.binding_revision,
            },
        )?;
        if binding.selected_path() != source.selected_path {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        let usable = match binding.state() {
            BindingState::Valid(usable) => usable.clone(),
            BindingState::Active(active) => active.usable().clone(),
            _ => return Err(SyndicMutationError::BindingPathConflict),
        };
        validate_canonical_execution(reader, source.thread_id, usable.execution())?;
        if usable.execution() != &request.execution || usable.tool_profile() != basis.tool_profile()
        {
            return Err(SyndicMutationError::ExecutionBindingConflict);
        }
        let reservation = required::<CasThreadIndexFamily>(
            reader,
            &CasThreadKey::Record(usable.cas_thread_id().clone()),
        )?;
        if reservation.thread_id() != source.thread_id
            || reservation.latest_binding_revision() != source.binding_revision
            || reservation.retired_binding_revision().is_some()
        {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        Some(usable)
    } else {
        None
    };
    let expected_lineage = match anchor.operation {
        RecoveryOperation::Fresh => {
            CasLineageProof::native(NativeCasLineage::Fresh, basis.represented_prefix())?
        }
        RecoveryOperation::Fork => {
            CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix())?
        }
        RecoveryOperation::Current | RecoveryOperation::Resume => {
            let source = source
                .as_ref()
                .ok_or(SyndicMutationError::BindingPathConflict)?;
            if request.cas_thread_id != *source.cas_thread_id() {
                return Err(SyndicMutationError::BindingPathConflict);
            }
            if request.lineage == source.lineage() {
                return Ok(());
            }
            CasLineageProof::native(NativeCasLineage::Resume, source.represented_prefix())?
        }
    };
    if request.lineage != expected_lineage {
        return Err(SyndicMutationError::BindingPathConflict);
    }
    Ok(())
}
