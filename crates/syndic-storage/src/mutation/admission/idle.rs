use super::*;

impl IdleSubmissionMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<IdleSubmissionRecords, SyndicMutationError> {
        let submission = &self.submission;
        let base = load_base(
            reader,
            submission.thread_id,
            submission.expected_thread_revision,
            submission.draft_id,
            submission.expected_draft_revision,
            submission.expected_content,
            submission.expected_gate_revision,
            submission.next_draft_id,
            submission.admitted_at,
        )?;
        if !matches!(base.gate.state(), InputGateState::Idle) || base.gate.live_count() != 0 {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        submission.markers.validate_content(base.draft.content())?;
        let turn_id = submission.submitted_turn_id();
        if point::<TurnsFamily>(reader, &turn_id)?.is_some()
            || point::<AcceptedInputsFamily>(reader, &submission.draft_id.accepted_input_id())?
                .is_some()
            || point::<CanonicalItemsFamily>(reader, &submission.user_item_id)?.is_some()
        {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }

        let parent = match base.draft.replacement_edit_intent() {
            Some(intent) => validate_replacement_intent(reader, &base.thread, intent)?
                .0
                .parent(),
            None => base.draft.parent(),
        };
        let (depth, digest, ancestor_skip) = turn_shape(reader, turn_id, parent)?;
        let (context_move, moved_context_owner) = context_move(reader, &base.draft, turn_id)?;
        let context_owner = moved_context_owner.or(base.thread.context_owner_id());
        let thread_revision = base.thread.revision().checked_next()?;
        let thread = ThreadRecord::new(
            base.thread.id(),
            thread_revision,
            Some(turn_id),
            submission.next_draft_id,
            base.thread.parent_thread_id(),
            context_owner,
            digest,
        );
        let draft_revision = DraftRevision::new(1)?;
        let draft = DraftRecord::new(
            submission.next_draft_id,
            thread.id(),
            draft_revision,
            ConversationParent::Turn(turn_id),
            None,
            None,
            base.empty_content,
            submission.admitted_at,
            submission.admitted_at,
        );
        let draft_index =
            DraftByThreadRecord::new(thread.id(), draft.id(), draft_revision, thread_revision);
        let turn = TurnRecord::new(
            turn_id,
            thread.id(),
            TurnKind::OrdinaryUser,
            parent,
            ancestor_skip,
            depth,
            digest,
            submission.admitted_at,
        );
        let turn_state = TurnStateRecord::with_capture_frontiers(
            turn_id,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            1,
            0,
            1,
            0,
            None,
            submission.admitted_at,
        )?;
        let child_index = parent
            .turn()
            .map(|parent_id| TurnChildIndexRecord::new(parent_id, turn_id, depth, digest));
        let item_revision = ProjectionRevision::new(1)?;
        let marker_count = u64::try_from(submission.markers.markers().len()).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "admission marker resolutions",
            }
        })?;
        let item = CanonicalItemRecord::local_user_input(
            submission.user_item_id,
            turn_id,
            TurnItemOrdinal::FIRST,
            item_revision,
            base.draft.content(),
            marker_count,
        );
        let item_index = TurnItemIndexRecord::new(
            turn_id,
            TurnItemOrdinal::FIRST,
            submission.user_item_id,
            item_revision,
        );
        let marker_records = marker_records(
            InputMarkerOwner::CanonicalItem(submission.user_item_id),
            &submission.markers,
        )?;

        let current_head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
        let transcript_build =
            crate::mutation::transcript::supersede_active_transcript_build(reader, &base.thread)?;
        let transcript_head = TranscriptViewHeadRecord::new(
            thread.id(),
            current_head.generation().checked_next()?,
            current_head.revision().checked_next()?,
            0,
            Some(turn_id),
            digest,
            ProjectionLifecycle::Stale,
        );
        let summary = HistorySummaryRecord::new(
            thread.id(),
            thread_revision,
            Some(turn_id),
            digest,
            false,
            submission.admitted_at,
        );
        let gate = InputGateRecord::new(
            thread.id(),
            base.gate.revision().checked_next()?,
            InputGateState::PendingTurn(turn_id),
            base.gate.accepted_high_water(),
            base.gate.live_steering_count(),
            base.gate.live_next_turn_count(),
            base.gate.live_logical_utf8_bytes(),
        )?;

        let binding_head = required::<BindingHeadsFamily>(reader, &thread.id())?;
        let binding_revision = binding_head.revision().checked_next()?;
        if point::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: thread.id(),
                revision: binding_revision,
            },
        )?
        .is_some()
        {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }
        let selected = SelectedPathProof::new(Some(turn_id), thread_revision, digest);
        let binding = BindingRecord::new(
            thread.id(),
            binding_revision,
            selected,
            BindingState::unbound("submitted turn awaits an execution projection")?,
        );
        let binding_head = BindingHeadRecord::new(
            thread.id(),
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        );
        let thread_parent_index = thread_parent_index(&thread);

        Ok(IdleSubmissionRecords {
            old_draft_id: base.draft.id(),
            thread,
            draft,
            draft_index,
            turn,
            turn_state,
            child_index,
            item,
            item_index,
            marker_records,
            transcript_head,
            transcript_build,
            summary,
            gate,
            binding,
            binding_head,
            context_move,
            thread_parent_index,
        })
    }
}

impl IdleSubmissionRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.delete::<DraftsCodec>(&self.old_draft_id)?;
        mutations.put::<ThreadsCodec>(&self.thread.id(), &self.thread)?;
        mutations.put::<DraftsCodec>(&self.draft.id(), &self.draft)?;
        mutations.put::<DraftByThreadCodec>(&self.thread.id(), &self.draft_index)?;
        mutations.put::<TurnsCodec>(&self.turn.id(), &self.turn)?;
        mutations.put::<TurnStatesCodec>(&self.turn.id(), &self.turn_state)?;
        if let Some(index) = &self.child_index {
            mutations.put::<TurnChildrenCodec>(
                &TurnPairKey {
                    parent: index.parent_id(),
                    child: index.child_id(),
                },
                index,
            )?;
        }
        mutations.put::<CanonicalItemsCodec>(&self.item.id(), &self.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: self.turn.id(),
                ordinal: TurnItemOrdinal::FIRST,
            },
            &self.item_index,
        )?;
        for marker in &self.marker_records {
            mutations.put::<InputMarkerResolutionsCodec>(
                &InputMarkerKey {
                    owner: marker.owner(),
                    ordinal: marker.ordinal(),
                },
                marker,
            )?;
        }
        mutations.put::<TranscriptHeadsCodec>(&self.thread.id(), &self.transcript_head)?;
        if let Some(build) = &self.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations.put::<HistorySummariesCodec>(&self.thread.id(), &self.summary)?;
        mutations.put::<InputGatesCodec>(&self.thread.id(), &self.gate)?;
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: self.thread.id(),
                revision: self.binding.revision(),
            },
            &self.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&self.thread.id(), &self.binding_head)?;
        if let Some(context) = &self.context_move {
            mutations.delete::<ContextEnvelopesCodec>(&ContextOwnerKey::from(context.old_owner))?;
            mutations.put::<ContextEnvelopesCodec>(
                &ContextOwnerKey::from(context.new_record.owner()),
                &context.new_record,
            )?;
        }
        if let Some(index) = &self.thread_parent_index {
            mutations.put::<ThreadParentCodec>(
                &ThreadPairKey {
                    first: index.parent_thread_id(),
                    second: index.child_thread_id(),
                },
                index,
            )?;
        }
        Ok(())
    }
}
