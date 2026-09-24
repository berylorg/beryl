use super::*;

pub(super) fn classify(
    reader: &impl TerminalHistoryReader,
    gate: DiscussionHandoffGateRecord,
    input: &AcceptedInputRecord,
) -> Result<
    (
        ThreadAttributesRecord,
        Option<DiscussionParentExecutionDisposition>,
    ),
    SyndicMutationError,
> {
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        return conflict();
    };
    let child = required::<ThreadsFamily>(reader, &receipt.child_thread_id)?;
    let attributes = required::<ThreadAttributesFamily>(reader, &child.id())?;
    let context =
        required::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(receipt.context_owner))?;
    if reader.read::<AcceptedInputsFamily>(&input.id())?.as_ref() != Some(input)
        || reader.read::<DiscussionHandoffGatesFamily>(&child.id())? != Some(gate)
        || gate.thread_id() != child.id()
        || gate.state()
            != (DiscussionHandoffGateState::Pending {
                intent_id: receipt.intent_id,
                job_id: receipt.job_id,
                resolving_turn_id: receipt.resolving_turn_id,
            })
        || child.id() != receipt.child_thread_id
        || child.parent_thread_id() != Some(input.thread_id())
        || child.committed_tail() != Some(receipt.resolving_turn_id)
        || child.context_owner_id() != Some(receipt.context_owner)
        || attributes.thread_id() != child.id()
        || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
        || context.owner() != receipt.context_owner
        || context.envelope().descriptor().digest() != receipt.context_digest
        || context.envelope().descriptor().source().thread_id() != input.thread_id()
        || required::<ContentManifestsFamily>(reader, &input.content().id())?.sealed_reference()
            != Some(input.content())
        || reader.read::<ThreadParentFamily>(&ThreadPairKey {
            first: input.thread_id(),
            second: child.id(),
        })? != Some(ThreadParentIndexRecord::new(
            input.thread_id(),
            child.id(),
            child.revision(),
            receipt.context_owner,
        ))
    {
        return conflict();
    }
    let turn = required::<TurnsFamily>(reader, &receipt.parent_turn_id)?;
    let state = required::<TurnStatesFamily>(reader, &turn.id())?;
    if turn.id() != receipt.parent_turn_id
        || turn.origin_thread_id() != input.thread_id()
        || turn.kind() != TurnKind::BerylDiscussionHandoff
        || state.turn_id() != turn.id()
    {
        return conflict();
    }
    let anchor = match state.dispatch_provenance() {
        TurnDispatchProvenance::Unattempted | TurnDispatchProvenance::Cancelled(_)
            if state.lifecycle() == TurnLifecycle::Pending =>
        {
            return Ok((attributes, None));
        }
        TurnDispatchProvenance::Activated(anchor) => anchor,
        _ => return conflict(),
    };
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &anchor.snapshot_id())?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: input.thread_id(),
            revision: anchor.binding_revision(),
        },
    )?;
    let execution = required::<ThreadExecutionsFamily>(reader, &input.thread_id())?;
    if !crate::dispatch_provenance::activation_matches(
        input.thread_id(),
        turn.id(),
        anchor,
        &snapshot,
        &binding,
    ) || snapshot.selected_path().digest() != turn.chain_digest()
        || execution.thread_id() != input.thread_id()
        || execution.execution() != snapshot.execution()
    {
        return conflict();
    }
    let accepted = reader.read::<ActiveCasTurnsFamily>(&snapshot.id())?;
    let cas = if let Some(accepted) = accepted {
        if accepted.snapshot_id() != snapshot.id()
            || accepted.thread_id() != input.thread_id()
            || accepted.turn_id() != turn.id()
            || accepted.binding_revision() != anchor.binding_revision()
            || accepted.cas_thread_id() != snapshot.cas_thread_id()
            || accepted.published_at() < snapshot.started_at()
        {
            return conflict();
        }
        let expected = CasTurnIndexRecord::new(
            accepted.cas_thread_id().clone(),
            accepted.cas_turn_id().clone(),
            input.thread_id(),
            turn.id(),
            anchor.binding_revision(),
            snapshot.id(),
            snapshot
                .represented_base_native_turn_count()
                .checked_next()?,
        );
        if reader.read::<CasTurnIndexFamily>(&CasTurnKey::Record(
            accepted.cas_thread_id().clone(),
            accepted.cas_turn_id().clone(),
        ))? != Some(expected)
        {
            return conflict();
        }
        Some(CasTurnSource::new(
            accepted.cas_thread_id().clone(),
            accepted.cas_turn_id().clone(),
        ))
    } else {
        None
    };
    let disposition = if state.lifecycle().is_proven_terminal() {
        let sequence = SourceEventSequence::new(state.source_event_count())?;
        let event = required::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: turn.id(),
                ordinal: sequence,
            },
        )?;
        if event.turn_id() != turn.id()
            || event.sequence() != sequence
            || !matches!(event.payload(), SourceEventPayload::TurnEnded(status) if Some(*status) == state.source_end_status())
            || event
                .source()
                .is_some_and(|source| Some(source) != cas.as_ref())
            || (event.source().is_none() && state.lifecycle() != TurnLifecycle::Incomplete)
            || (cas.is_none() && state.lifecycle() != TurnLifecycle::Incomplete)
        {
            return conflict();
        }
        Some(DiscussionParentExecutionDisposition::Terminal {
            status: state
                .end_status()
                .ok_or(SyndicMutationError::DiscussionHandoffConflict)?,
            cas,
        })
    } else {
        cas.map(DiscussionParentExecutionDisposition::Accepted)
    };
    Ok((attributes, disposition))
}

fn required<F: Family>(
    reader: &impl TerminalHistoryReader,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError> {
    reader
        .read::<F>(key)?
        .ok_or(SyndicMutationError::DiscussionHandoffConflict)
}
fn conflict<T>() -> Result<T, SyndicMutationError> {
    Err(SyndicMutationError::DiscussionHandoffConflict)
}
