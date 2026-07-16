use super::*;

impl AcceptedInputMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AcceptedInputRecords, SyndicMutationError> {
        let admission = &self.admission;
        let base = load_base(
            reader,
            admission.thread_id,
            admission.expected_thread_revision,
            admission.draft_id,
            admission.expected_draft_revision,
            admission.expected_content,
            admission.expected_gate_revision,
            admission.next_draft_id,
            admission.admitted_at,
        )?;
        let disposition = base
            .gate
            .state()
            .admitted_disposition()
            .ok_or(SyndicMutationError::InputGateStateConflict)?;
        if base.draft.context_owner_id().is_some() || base.draft.replacement_edit_intent().is_some()
        {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        admission.markers.validate_content(base.draft.content())?;
        let input_id = admission.accepted_input_id();
        if point::<AcceptedInputsFamily>(reader, &input_id)?.is_some()
            || point::<TurnsFamily>(reader, &admission.draft_id.submitted_turn_id())?.is_some()
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
        let logical_bytes = base.draft.content().summary().logical_utf8_bytes();
        let (steering_count, next_count) = match &disposition {
            AcceptedInputDisposition::AwaitingSteering(_)
            | AcceptedInputDisposition::SteerActiveTurn(_) => (
                base.gate.live_steering_count().checked_add(1).ok_or(
                    SyndicRecordError::LengthOverflow {
                        kind: "live steering count",
                    },
                )?,
                base.gate.live_next_turn_count(),
            ),
            AcceptedInputDisposition::NextTurn(_) => (
                base.gate.live_steering_count(),
                base.gate.live_next_turn_count().checked_add(1).ok_or(
                    SyndicRecordError::LengthOverflow {
                        kind: "live next-turn count",
                    },
                )?,
            ),
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
            steering_count,
            next_count,
            live_bytes,
        )?;
        let marker_count = u64::try_from(admission.markers.markers().len()).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "admission marker resolutions",
            }
        })?;
        let input = crate::AcceptedInputRecord::new(
            input_id,
            base.thread.id(),
            input_revision,
            ordinal,
            base.gate.revision(),
            disposition.clone(),
            AcceptedInputLifecycle::Admitted,
            base.draft.content(),
            marker_count,
            admission.admitted_at,
        );
        let order_index = crate::AcceptedOrderIndexRecord::new(
            base.thread.id(),
            ordinal,
            input_id,
            input_revision,
        );
        let (steering_index, next_index) = match &disposition {
            AcceptedInputDisposition::AwaitingSteering(target) => (
                Some(crate::AcceptedSteeringIndexRecord::new(
                    base.thread.id(),
                    target.active_turn_id(),
                    ordinal,
                    input_id,
                    input_revision,
                )),
                None,
            ),
            AcceptedInputDisposition::SteerActiveTurn(target) => (
                Some(crate::AcceptedSteeringIndexRecord::new(
                    base.thread.id(),
                    target.pending().active_turn_id(),
                    ordinal,
                    input_id,
                    input_revision,
                )),
                None,
            ),
            AcceptedInputDisposition::NextTurn(_) => (
                None,
                Some(crate::AcceptedNextTurnIndexRecord::new(
                    base.thread.id(),
                    ordinal,
                    input_id,
                    input_revision,
                )),
            ),
        };
        let marker_records = marker_records(
            InputMarkerOwner::AcceptedInput(input_id),
            &admission.markers,
        )?;

        let thread_revision = base.thread.revision().checked_next()?;
        let thread = ThreadRecord::new(
            base.thread.id(),
            thread_revision,
            base.thread.committed_tail(),
            admission.next_draft_id,
            base.thread.parent_thread_id(),
            base.thread.context_owner_id(),
            base.thread.selected_path_digest(),
        );
        let draft_revision = DraftRevision::new(1)?;
        let draft = DraftRecord::new(
            admission.next_draft_id,
            thread.id(),
            draft_revision,
            base.draft.parent(),
            None,
            None,
            base.empty_content,
            admission.admitted_at,
            admission.admitted_at,
        );
        let draft_index =
            DraftByThreadRecord::new(thread.id(), draft.id(), draft_revision, thread_revision);
        let summary = HistorySummaryRecord::new(
            thread.id(),
            thread_revision,
            thread.committed_tail(),
            thread.selected_path_digest(),
            false,
            admission.admitted_at,
        );
        let thread_parent_index = thread_parent_index(&thread);
        Ok(AcceptedInputRecords {
            old_draft_id: base.draft.id(),
            thread,
            draft,
            draft_index,
            input,
            order_index,
            steering_index,
            next_index,
            marker_records,
            summary,
            gate,
            thread_parent_index,
        })
    }
}

impl AcceptedInputRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.delete::<DraftsCodec>(&self.old_draft_id)?;
        mutations.put::<ThreadsCodec>(&self.thread.id(), &self.thread)?;
        mutations.put::<DraftsCodec>(&self.draft.id(), &self.draft)?;
        mutations.put::<DraftByThreadCodec>(&self.thread.id(), &self.draft_index)?;
        mutations.put::<AcceptedInputsCodec>(&self.input.id(), &self.input)?;
        mutations.put::<AcceptedOrderCodec>(
            &ThreadAcceptedKey {
                owner: self.thread.id(),
                ordinal: self.input.ordinal(),
            },
            &self.order_index,
        )?;
        if let Some(index) = &self.steering_index {
            mutations.put::<AcceptedSteeringCodec>(
                &SteeringKey {
                    thread: index.thread_id(),
                    turn: index.turn_id(),
                    ordinal: index.ordinal(),
                },
                index,
            )?;
        }
        if let Some(index) = &self.next_index {
            mutations.put::<AcceptedNextCodec>(
                &ThreadAcceptedKey {
                    owner: index.thread_id(),
                    ordinal: index.ordinal(),
                },
                index,
            )?;
        }
        for marker in &self.marker_records {
            mutations.put::<InputMarkerResolutionsCodec>(
                &InputMarkerKey {
                    owner: marker.owner(),
                    ordinal: marker.ordinal(),
                },
                marker,
            )?;
        }
        mutations.put::<HistorySummariesCodec>(&self.thread.id(), &self.summary)?;
        mutations.put::<InputGatesCodec>(&self.thread.id(), &self.gate)?;
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
