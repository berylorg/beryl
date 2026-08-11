use super::*;

enum LifecycleSettlementRecords {
    UserWork(SettlementRecords),
    Continuation(Box<ContinuationRecords>),
}

struct ContinuationRecords {
    operation: CompactionOperationRecord,
    receipt: CompactionSettlementReceiptRecord,
    thread: crate::ThreadRecord,
    draft_index: crate::DraftByThreadRecord,
    turn: TurnRecord,
    turn_state: TurnStateRecord,
    child: Option<crate::TurnChildIndexRecord>,
    item: crate::CanonicalItemRecord,
    item_index: crate::TurnItemIndexRecord,
    transcript_head: crate::TranscriptViewHeadRecord,
    transcript_build: Option<crate::TranscriptBuildRecord>,
    summary: crate::HistorySummaryRecord,
    gate: InputGateRecord,
    activity_head: crate::ActivityQueryHeadRecord,
    activity_source: crate::ActivityQuerySourceRecord,
    binding: crate::BindingRecord,
    binding_head: crate::BindingHeadRecord,
}

impl DomainMutation<SyndicDomain> for SettleLifecycleMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<CompactionOperationsCodec>(1)?;
        reservation.reserve_records::<CompactionSettlementReceiptsCodec>(1)?;
        reservation.reserve_records::<InputGatesCodec>(1)?;
        reservation.reserve_records::<SourceEventsCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        reservation.reserve_records::<CasThreadIndexCodec>(1)?;
        reservation.reserve_records::<CasThreadBindingIndexCodec>(1)?;
        reservation.reserve_records::<ThreadsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<TurnsCodec>(1)?;
        reservation.reserve_records::<TurnChildrenCodec>(1)?;
        reservation.reserve_records::<CanonicalItemsCodec>(1)?;
        reservation.reserve_records::<TurnItemsCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
        reservation.reserve_records::<ActivityQuerySourcesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.records(reader)? {
            LifecycleSettlementRecords::UserWork(records) => {
                mutations.put::<CompactionOperationsCodec>(
                    &records.operation.id(),
                    &records.operation,
                )?;
                mutations.put::<CompactionSettlementReceiptsCodec>(
                    &records.receipt.operation_id(),
                    &records.receipt,
                )?;
                mutations.put::<InputGatesCodec>(&records.gate.thread_id(), &records.gate)?;
            }
            LifecycleSettlementRecords::Continuation(records) => {
                records.contribute(mutations)?;
            }
        }
        Ok(())
    }
}

impl SettleLifecycleMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<LifecycleSettlementRecords, SyndicMutationError> {
        let request = &self.0;
        let (gate, operation) = live_operation(
            reader,
            request.operation_id,
            request.expected_operation_revision,
        )?;
        if !successful_compaction(&operation) {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        if gate.live_next_turn_count() > 0 {
            return Ok(LifecycleSettlementRecords::UserWork(
                SettleMutation(SettleCompactionOperation::new(
                    request.operation_id,
                    request.expected_operation_revision,
                    CompactionSettlement::LifecycleUserWorkWon,
                ))
                .records(reader)?,
            ));
        }
        if gate.live_logical_utf8_bytes() != 0 {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        self.continuation_records(reader, gate, operation)
            .map(|records| LifecycleSettlementRecords::Continuation(Box::new(records)))
    }

    fn continuation_records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        current_gate: InputGateRecord,
        current_operation: CompactionOperationRecord,
    ) -> Result<ContinuationRecords, SyndicMutationError> {
        let request = &self.0;
        if point::<CompactionSettlementReceiptsFamily>(reader, &current_operation.id())?.is_some() {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }
        let prepared = crate::prepare_lifecycle_continuation_content()?;
        let manifest = required::<ContentManifestsFamily>(reader, &request.content.id())?;
        if manifest.sealed_reference() != Some(request.content)
            || request.content.id() != prepared.id()
            || request.content.encoding() != prepared.encoding()
            || request.content.summary() != prepared.summary()
        {
            return Err(SyndicMutationError::ContentManifestConflict);
        }
        let expected_turn = crate::derive_lifecycle_continuation_turn_id(
            current_operation.home_id(),
            current_operation.id(),
            request.content.summary().digest(),
        );
        let expected_item = crate::derive_lifecycle_continuation_item_id(
            current_operation.home_id(),
            current_operation.id(),
            request.content.summary().digest(),
        );
        if request.turn_id != expected_turn
            || request.item_id != expected_item
            || point::<TurnsFamily>(reader, &request.turn_id)?.is_some()
            || point::<TurnStatesFamily>(reader, &request.turn_id)?.is_some()
            || point::<CanonicalItemsFamily>(reader, &request.item_id)?.is_some()
        {
            return Err(SyndicMutationError::AdmissionIdentityCollision);
        }
        let current_thread = required::<ThreadsFamily>(reader, &request.operation_id.thread_id())?;
        let admission_snapshot = required::<ExecutionSnapshotsFamily>(
            reader,
            &current_operation.target().snapshot_id(),
        )?;
        if admission_snapshot.selected_path() != current_thread.selected_path() {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        let current_draft = crate::mutation::current_draft(reader, current_thread.id())?;
        let current_summary = required::<HistorySummariesFamily>(reader, &current_thread.id())?;
        if request.settled_at < current_draft.updated_at()
            || request.settled_at < current_summary.last_activity_at()
        {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        let parent = ConversationParent::from_turn(admission_snapshot.selected_path().tail());
        let (depth, digest, ancestor_skip) =
            crate::mutation::admission::turn_shape(reader, request.turn_id, parent)?;
        let thread_revision = admission_snapshot
            .selected_path()
            .thread_revision()
            .checked_next()?;
        let selected =
            crate::SelectedPathProof::new(Some(request.turn_id), thread_revision, digest);
        let thread = crate::ThreadRecord::new(
            current_thread.id(),
            selected,
            current_thread.current_draft_id(),
            current_thread.lineage(),
            current_thread.image_label_frontiers(),
            current_thread.context_owner_id(),
        );
        let draft_index = crate::DraftByThreadRecord::new(
            thread.id(),
            current_draft.id(),
            current_draft.revision(),
            thread_revision,
        );
        let turn = TurnRecord::new(
            request.turn_id,
            thread.id(),
            TurnKind::BerylLifecycleContinuation,
            parent,
            ancestor_skip,
            depth,
            digest,
            request.settled_at,
        );
        let turn_state = TurnStateRecord::with_capture_frontiers(
            request.turn_id,
            TurnStateRevision::FIRST,
            TurnLifecycle::Pending,
            0,
            1,
            0,
            1,
            0,
            None,
            request.settled_at,
        )?;
        let child = parent.turn().map(|parent_id| {
            crate::TurnChildIndexRecord::new(parent_id, request.turn_id, depth, digest)
        });
        let item_revision = ProjectionRevision::new(1)?;
        let item = crate::CanonicalItemRecord::local_user_input(
            request.item_id,
            request.turn_id,
            crate::TurnItemOrdinal::FIRST,
            item_revision,
            request.content,
            None,
        );
        let item_index = crate::TurnItemIndexRecord::new(
            request.turn_id,
            crate::TurnItemOrdinal::FIRST,
            request.item_id,
            item_revision,
        );
        let current_transcript = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
        let transcript_build = crate::mutation::transcript::supersede_active_transcript_build(
            reader,
            &current_thread,
        )?;
        let transcript_head = crate::TranscriptViewHeadRecord::new(
            thread.id(),
            current_transcript.generation().checked_next()?,
            current_transcript.revision().checked_next()?,
            0,
            Some(request.turn_id),
            digest,
            crate::ProjectionLifecycle::Stale,
        );
        let summary = crate::HistorySummaryRecord::new(
            thread.id(),
            current_summary.revision().checked_next()?,
            thread_revision,
            Some(request.turn_id),
            digest,
            false,
            request.settled_at,
        );
        let gate_revision = current_gate.revision().checked_next()?;
        let gate = InputGateRecord::new(
            thread.id(),
            gate_revision,
            InputGateState::PendingTurn(request.turn_id),
            current_gate.accepted_high_water(),
            current_gate.route_generation_high_water(),
            None,
            0,
            0,
            0,
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
        let source = crate::ActivityQuerySource::new(thread.id(), request.turn_id);
        let activity_head = crate::ActivityQueryHeadRecord::new(
            thread.id(),
            work_period,
            Some(source),
            true,
            0,
            current_activity.revision().checked_next()?,
            1,
            0,
            0,
            0,
            0,
            None,
            crate::ProjectionLifecycle::Current,
        )?;
        let activity_source = crate::ActivityQuerySourceRecord::new(
            thread.id(),
            work_period,
            source,
            None,
            0,
            true,
            None,
        );
        let binding_head = required::<BindingHeadsFamily>(reader, &thread.id())?;
        let current_binding = required::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: thread.id(),
                revision: binding_head.revision(),
            },
        )?;
        let BindingState::Valid(_) = current_binding.state() else {
            return Err(SyndicMutationError::BindingStateConflict);
        };
        if current_binding.revision() != current_operation.target().binding_revision() {
            return Err(SyndicMutationError::BindingStateConflict);
        }
        if current_binding.selected_path() != admission_snapshot.selected_path() {
            return Err(SyndicMutationError::BindingPathConflict);
        }
        let binding_revision = current_binding.revision().checked_next()?;
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
        let binding = crate::BindingRecord::new(
            thread.id(),
            binding_revision,
            selected,
            BindingState::unbound("lifecycle continuation awaits an execution projection")?,
        );
        let binding_head = crate::BindingHeadRecord::new(
            thread.id(),
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        );
        let settlement = CompactionSettlement::LifecycleContinuation {
            turn_id: request.turn_id,
            item_id: request.item_id,
            content_id: request.content.id(),
        };
        let successor_operation_revision = current_operation.revision().checked_next()?;
        let receipt = CompactionSettlementReceiptRecord::new(
            current_operation.id(),
            current_operation.revision(),
            successor_operation_revision,
            current_gate,
            gate.clone(),
            settlement,
            Some(CompactionContinuationReceipt::new(
                parent,
                selected,
                binding_revision,
                request.content,
            )),
        )?;
        let receipt_commitment = compaction_settlement_receipt_commitment(&receipt)
            .map_err(|_| crate::SyndicRecordError::InvalidCompactionOperation)?;
        let operation = current_operation.consume(
            gate_revision,
            receipt.settlement().clone(),
            receipt_commitment,
        )?;
        Ok(ContinuationRecords {
            operation,
            receipt,
            thread,
            draft_index,
            turn,
            turn_state,
            child,
            item,
            item_index,
            transcript_head,
            transcript_build,
            summary,
            gate,
            activity_head,
            activity_source,
            binding,
            binding_head,
        })
    }
}

impl ContinuationRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<CompactionOperationsCodec>(&self.operation.id(), &self.operation)?;
        mutations.put::<CompactionSettlementReceiptsCodec>(
            &self.receipt.operation_id(),
            &self.receipt,
        )?;
        mutations.put::<ThreadsCodec>(&self.thread.id(), &self.thread)?;
        mutations.put::<DraftByThreadCodec>(&self.thread.id(), &self.draft_index)?;
        mutations.put::<TurnsCodec>(&self.turn.id(), &self.turn)?;
        mutations.put::<TurnStatesCodec>(&self.turn.id(), &self.turn_state)?;
        if let Some(child) = &self.child {
            mutations.put::<TurnChildrenCodec>(
                &TurnPairKey {
                    parent: child.parent_id(),
                    child: child.child_id(),
                },
                child,
            )?;
        }
        mutations.put::<CanonicalItemsCodec>(&self.item.id(), &self.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: self.turn.id(),
                ordinal: crate::TurnItemOrdinal::FIRST,
            },
            &self.item_index,
        )?;
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
        mutations.put::<ActivityQueryHeadsCodec>(&self.thread.id(), &self.activity_head)?;
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
        Ok(())
    }
}

fn successful_compaction(operation: &CompactionOperationRecord) -> bool {
    operation
        .terminal()
        .is_some_and(|terminal| terminal.status().outcome() == crate::TurnTerminalOutcome::Complete)
        && operation
            .marker()
            .is_some_and(|marker| marker.lifecycle() == CompactionMarkerLifecycle::Completed)
}
