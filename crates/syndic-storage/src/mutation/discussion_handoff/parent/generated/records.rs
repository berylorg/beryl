use super::*;

pub(super) fn prepare(
    reader: &ParentRead<'_>,
    parent: &PreparedDiscussionParent,
    request: &GeneratedDiscussionInput,
    resolution_digest: [u8; 32],
    content: PreparedContent,
) -> Result<(AcceptedInputRecord, Vec<Change>), SyndicMutationError> {
    let thread_id = parent.request.parent_thread_id;
    let old_thread = required::<ThreadsFamily>(reader, &thread_id)?;
    let old_gate = required::<InputGatesFamily>(reader, &thread_id)?;
    let draft = required::<DraftsFamily>(reader, &old_thread.current_draft_id())?;
    let summary = required::<HistorySummariesFamily>(reader, &thread_id)?;
    if request.admitted_at < draft.updated_at() || request.admitted_at < summary.last_activity_at()
    {
        return Err(SyndicMutationError::TimestampRegressed);
    }
    let DiscussionHandoffGateState::Pending {
        intent_id,
        job_id,
        resolving_turn_id,
    } = parent.request.child_gate.state()
    else {
        return Err(SyndicMutationError::DiscussionHandoffConflict);
    };
    let input_id = SyndicAcceptedInputId::from_bytes(*job_id.as_bytes());
    let turn_id = request.parent_turn_id;
    let item_id = request.canonical_item_id;
    if turn_id.as_bytes() == input_id.as_bytes()
        || turn_id.as_bytes() == item_id.as_bytes()
        || input_id.as_bytes() == item_id.as_bytes()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let mut changes = Vec::new();
    macro_rules! change {
        ($variant:ident, $family:ty, $key:expr, $new:expr) => {{
            let key = $key;
            let old = reader.read::<$family>(&key)?;
            changes.push(Change::$variant {
                key,
                old,
                new: Some($new),
            });
        }};
    }
    macro_rules! insert {
        ($variant:ident, $family:ty, $key:expr, $new:expr) => {{
            let key = $key;
            if reader.read::<$family>(&key)?.is_some() {
                return Err(SyndicMutationError::AdmissionIdentityCollision);
            }
            changes.push(Change::$variant {
                key,
                old: None,
                new: Some($new),
            });
        }};
    }
    for bytes in [input_id.as_bytes(), turn_id.as_bytes()] {
        let key = SyndicDraftId::from_bytes(*bytes);
        if reader.read::<DraftsFamily>(&key)?.is_some() {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }
        changes.push(Change::DraftAbsence {
            key,
            old: None,
            new: None,
        });
    }
    let raw_turn = SyndicTurnId::from_bytes(*input_id.as_bytes());
    let raw_input = SyndicAcceptedInputId::from_bytes(*turn_id.as_bytes());
    if reader.read::<TurnsFamily>(&raw_turn)?.is_some()
        || reader.read::<AcceptedInputsFamily>(&raw_input)?.is_some()
        || reader
            .read::<AcceptedRouteLeavesFamily>(&input_id)?
            .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    changes.push(Change::Turn {
        key: raw_turn,
        old: None,
        new: None,
    });
    changes.push(Change::Input {
        key: raw_input,
        old: None,
        new: None,
    });
    changes.push(Change::RouteAbsence {
        key: input_id,
        old: None,
        new: None,
    });
    let content_ref = super::content::prepare_content(reader, content, &mut changes)?;
    let ordinal = AcceptedInputOrdinal::new(
        old_gate
            .accepted_high_water()
            .checked_add(1)
            .ok_or(SyndicMutationError::AdmissionIdentityCollision)?,
    )?;
    let receipt = DiscussionHandoffReceipt {
        parent_thread_revision: old_thread.revision(),
        parent_gate_revision: old_gate.revision(),
        child_thread_id: parent.request.child_gate.thread_id(),
        intent_id,
        job_id,
        context_owner: parent.request.context_owner,
        context_digest: parent.request.context_digest,
        resolving_turn_id,
        resolution_digest,
        parent_turn_id: turn_id,
        canonical_item_id: item_id,
    };
    let input = AcceptedInputRecord::new(
        input_id,
        thread_id,
        ordinal,
        AcceptedInputSource::DiscussionHandoff(receipt),
        content_ref,
        None,
        request.admitted_at,
    )?;
    insert!(Input, AcceptedInputsFamily, input_id, input.clone());
    let order = AcceptedOrderIndexRecord::from_source(
        thread_id,
        ordinal,
        input_id,
        AcceptedOrderSource::DiscussionHandoff,
    );
    insert!(
        Order,
        AcceptedOrderFamily,
        ThreadAcceptedKey {
            owner: thread_id,
            ordinal
        },
        order
    );
    let conversation_parent = ConversationParent::from_turn(old_thread.committed_tail());
    let (depth, digest, skip) =
        crate::mutation::admission_helpers::turn_shape(reader, turn_id, conversation_parent)?;
    let path = SelectedPathProof::new(Some(turn_id), old_thread.revision().checked_next()?, digest);
    let thread = ThreadRecord::new(
        thread_id,
        path,
        old_thread.current_draft_id(),
        old_thread.lineage(),
        old_thread.context_owner_id(),
    );
    change!(Thread, ThreadsFamily, thread_id, thread.clone());
    change!(
        DraftIndex,
        DraftByThreadFamily,
        thread_id,
        DraftByThreadRecord::new(thread_id, draft.id(), draft.revision(), thread.revision())
    );
    insert!(
        Turn,
        TurnsFamily,
        turn_id,
        TurnRecord::new(
            turn_id,
            thread_id,
            TurnKind::BerylDiscussionHandoff,
            conversation_parent,
            skip,
            depth,
            digest,
            request.admitted_at
        )
    );
    insert!(
        TurnState,
        TurnStatesFamily,
        turn_id,
        TurnStateRecord::with_capture_frontiers(
            turn_id,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            1,
            0,
            1,
            0,
            None,
            request.admitted_at,
            TurnDispatchProvenance::Unattempted
        )?
    );
    if let Some(parent_id) = conversation_parent.turn() {
        insert!(
            Child,
            TurnChildrenFamily,
            TurnPairKey {
                parent: parent_id,
                child: turn_id
            },
            TurnChildIndexRecord::new(parent_id, turn_id, depth, digest)
        );
    }
    let revision = ProjectionRevision::new(1)?;
    insert!(
        Item,
        CanonicalItemsFamily,
        item_id,
        CanonicalItemRecord::local_discussion_handoff(
            item_id,
            turn_id,
            TurnItemOrdinal::FIRST,
            revision,
            content_ref,
            input_id
        )
    );
    insert!(
        ItemIndex,
        TurnItemsFamily,
        TurnItemKey {
            owner: turn_id,
            ordinal: TurnItemOrdinal::FIRST
        },
        TurnItemIndexRecord::new(turn_id, TurnItemOrdinal::FIRST, item_id, revision)
    );
    let head = required::<TranscriptHeadsFamily>(reader, &thread_id)?;
    prepare_transcript_build(reader, &old_thread, &mut changes)?;
    change!(
        Transcript,
        TranscriptHeadsFamily,
        thread_id,
        TranscriptViewHeadRecord::new(
            thread_id,
            head.generation().checked_next()?,
            head.revision().checked_next()?,
            0,
            Some(turn_id),
            digest,
            ProjectionLifecycle::Stale
        )
    );
    change!(
        Summary,
        HistorySummariesFamily,
        thread_id,
        HistorySummaryRecord::new(
            thread_id,
            summary.revision().checked_next()?,
            thread.revision(),
            Some(turn_id),
            digest,
            false,
            request.admitted_at
        )
    );
    let gate = InputGateRecord::new(
        thread_id,
        old_gate.revision().checked_next()?,
        InputGateState::PendingTurn(turn_id),
        ordinal.get(),
        old_gate.route_generation_high_water(),
        None,
        0,
        0,
        0,
    )?;
    change!(Gate, InputGatesFamily, thread_id, gate.clone());
    insert!(
        NonIdle,
        NonIdleGateSourcesFamily,
        thread_id,
        NonIdleGateSourceRecord::new(thread_id, gate.revision())
    );
    let binding = required::<BindingHeadsFamily>(reader, &thread_id)?;
    let binding_revision = binding.revision().checked_next()?;
    insert!(
        Binding,
        BindingsFamily,
        BindingKey {
            thread: thread_id,
            revision: binding_revision
        },
        BindingRecord::new(
            thread_id,
            binding_revision,
            path,
            BindingState::unbound("submitted turn awaits an execution projection")?
        )
    );
    change!(
        BindingHead,
        BindingHeadsFamily,
        thread_id,
        BindingHeadRecord::new(
            thread_id,
            binding_revision,
            BindingLifecycle::Unbound,
            digest
        )
    );
    if let Some(index) = crate::mutation::admission_helpers::thread_parent_index(&thread) {
        change!(
            Parent,
            ThreadParentFamily,
            ThreadPairKey {
                first: index.parent_thread_id(),
                second: thread_id
            },
            index
        );
    }
    Ok((input, changes))
}

fn prepare_transcript_build(
    reader: &ParentRead<'_>,
    thread: &ThreadRecord,
    changes: &mut Vec<Change>,
) -> Result<(), SyndicMutationError> {
    use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};
    let page = reader
        .access
        .read_cursor::<SyndicDomain, TranscriptBuildsCodec>(
            &reader.storage.handle,
            &CursorRange::closed(
                ThreadTranscriptBuildKey::first_for_thread(thread.id()),
                ThreadTranscriptBuildKey::last_for_thread(thread.id()),
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 1024 * 1024).expect("fixed transcript-build bound"),
        )?;
    if let Some(record) = page.records().first() {
        let old = *record.value();
        if matches!(
            old.phase(),
            TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
        ) {
            if old.source_thread_revision() > thread.revision()
                || old.committed_tail() != thread.committed_tail()
                || old.selected_path_digest() != thread.selected_path_digest()
            {
                return Err(SyndicMutationError::TranscriptBuildConflict);
            }
            let new = TranscriptBuildRecord::new(
                old.thread_id(),
                old.generation(),
                old.revision().checked_next()?,
                old.source_thread_revision(),
                old.committed_tail(),
                old.selected_path_digest(),
                old.path_turn_count(),
                old.entry_count(),
                old.entry_digest(),
                old.history_complete(),
                TranscriptBuildPhase::Superseded,
            );
            changes.push(Change::TranscriptBuild {
                key: ThreadTranscriptBuildKey {
                    thread: thread.id(),
                    generation: old.generation(),
                },
                old: Some(old),
                new: Some(new),
            });
        }
    }
    Ok(())
}
