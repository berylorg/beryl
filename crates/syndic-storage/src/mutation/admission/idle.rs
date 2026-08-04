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
        let (image_label_frontiers, origin_span) = advance_image_label_authority(
            reader,
            &base.thread,
            crate::ImageLabelOriginOwner::CanonicalItem(submission.user_item_id),
            base.draft.content(),
            submission.asset_reference_set,
        )?;
        let turn_id = submission.submitted_turn_id();
        if point::<TurnsFamily>(reader, &turn_id)?.is_some()
            || point::<AcceptedInputsFamily>(reader, &submission.draft_id.accepted_input_id())?
                .is_some()
            || point::<CanonicalItemsFamily>(reader, &submission.user_item_id)?.is_some()
        {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }

        let (context_move, moved_context_owner, context_parent) =
            context_move(reader, &base.draft, turn_id)?;
        let parent = match base.draft.submission_intent() {
            DraftSubmissionIntent::Ordinary => {
                ConversationParent::from_turn(base.thread.committed_tail())
            }
            DraftSubmissionIntent::DiscussionContext(_) => {
                context_parent.ok_or(SyndicMutationError::CurrentDraftConflict)?
            }
            DraftSubmissionIntent::Replacement(intent) => {
                validate_replacement_intent(reader, &base.thread, intent)?
                    .0
                    .parent()
            }
        };
        let (depth, digest, ancestor_skip) = turn_shape(reader, turn_id, parent)?;
        let context_owner = moved_context_owner.or(base.thread.context_owner_id());
        let thread_revision = base.thread.revision().checked_next()?;
        let selected = SelectedPathProof::new(Some(turn_id), thread_revision, digest);
        let thread = ThreadRecord::new(
            base.thread.id(),
            selected,
            submission.next_draft_id,
            base.thread.lineage(),
            image_label_frontiers,
            context_owner,
        );
        let draft_revision = DraftRevision::new(1)?;
        let draft = DraftRecord::new(
            submission.next_draft_id,
            thread.id(),
            draft_revision,
            DraftSubmissionIntent::Ordinary,
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
        let item = CanonicalItemRecord::local_user_input(
            submission.user_item_id,
            turn_id,
            TurnItemOrdinal::FIRST,
            item_revision,
            base.draft.content(),
            submission.asset_reference_set,
        );
        let item_index = TurnItemIndexRecord::new(
            turn_id,
            TurnItemOrdinal::FIRST,
            submission.user_item_id,
            item_revision,
        );
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
            base.summary.revision().checked_next()?,
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
            base.gate.route_generation_high_water(),
            None,
            base.gate.live_steering_count(),
            base.gate.live_next_turn_count(),
            base.gate.live_logical_utf8_bytes(),
        )?;
        let current_activity = required::<ActivityQueryHeadsFamily>(reader, &thread.id())?;
        if current_activity.source_active()
            || current_activity.logical_row_count() != current_activity.completed_row_count()
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let work_period = if current_activity.source().is_none() {
            current_activity.work_period()
        } else {
            current_activity.work_period().checked_next()?
        };
        let activity_head = crate::ActivityQueryHeadRecord::new(
            thread.id(),
            work_period,
            Some(crate::ActivityQuerySource::new(thread.id(), turn_id)),
            true,
            0,
            current_activity.revision().checked_next()?,
            1,
            0,
            0,
            0,
            0,
            None,
            ProjectionLifecycle::Current,
        )?;
        let activity_source = crate::ActivityQuerySourceRecord::new(
            thread.id(),
            work_period,
            crate::ActivityQuerySource::new(thread.id(), turn_id),
            None,
            0,
            true,
            None,
        );

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
            origin_span,
            transcript_head,
            transcript_build,
            summary,
            gate,
            activity_head,
            activity_source,
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
        if let Some(span) = &self.origin_span {
            mutations.put::<ImageLabelOriginSpansCodec>(
                &ImageLabelOriginSpanKey {
                    thread: span.thread_id(),
                    end_label: span.end_label(),
                },
                span,
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
        mutations
            .put::<ActivityQueryHeadsCodec>(&self.activity_head.thread_id(), &self.activity_head)?;
        mutations.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: self.activity_source.thread_id(),
                work_period: self.activity_source.work_period(),
                source_thread: self.activity_source.source().thread_id(),
                source_turn: self.activity_source.source().turn_id(),
            },
            &self.activity_source,
        )?;
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
