use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InitialThreadRecords {
    pub(crate) thread: ThreadRecord,
    pub(crate) image_label_authority_head: ImageLabelAuthorityHeadV1,
    pub(crate) draft_image_label_protection_head: DraftImageLabelProtectionHeadV1,
    pub(crate) execution: ThreadExecutionRecord,
    pub(crate) attributes: ThreadAttributesRecord,
    pub(crate) usage: ThreadUsageRecord,
    pub(crate) catalog_summary: ThreadCatalogSummaryRecord,
    pub(crate) draft: DraftRecord,
    pub(crate) draft_piece_root: DraftPieceRootRecordV1,
    pub(crate) draft_edit_history: DraftEditHistoryFrontierV1,
    pub(crate) draft_index: DraftByThreadRecord,
    pub(crate) transcript_head: TranscriptViewHeadRecord,
    pub(crate) transcript_build: Option<crate::TranscriptBuildRecord>,
    pub(crate) summary: HistorySummaryRecord,
    pub(crate) input_gate: InputGateRecord,
    pub(crate) activity_head: crate::ActivityQueryHeadRecord,
    pub(crate) binding: BindingRecord,
    pub(crate) binding_head: BindingHeadRecord,
}

impl InitialThreadRecords {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        thread: ThreadRecord,
        execution_binding: ExecutionBinding,
        attributes: ThreadAttributesRecord,
        intent: DraftSubmissionIntent,
        inherited_labels: crate::ImageLabelFrontier,
        initial_title: Option<crate::ThreadCatalogTitle>,
        created_at: SyndicTimestamp,
        history_policy: DraftEditHistoryPolicyV1,
    ) -> Self {
        let thread_id = thread.id();
        let draft_id = thread.current_draft_id();
        let selected_path = thread.selected_path();
        let thread_revision = thread.revision();
        let draft_revision = DraftRevision::new(1).expect("initial revision is nonzero");
        let projection_revision = ProjectionRevision::new(1).expect("initial revision is nonzero");
        let binding_revision = BindingRevision::new(1).expect("initial revision is nonzero");
        let draft_piece_root = canonical_empty_draft_piece_root_v1(
            draft_id,
            draft_revision,
            canonical_empty_draft_root_operation_id_v1(draft_id),
        );
        let draft_edit_history =
            canonical_empty_draft_edit_history_v1(draft_piece_root.reference(), history_policy);
        let draft = DraftRecord::new(
            draft_id,
            thread_id,
            draft_revision,
            intent,
            DraftRootHistoryPairV1::new(
                draft_piece_root.reference(),
                draft_edit_history.reference(),
            ),
            created_at,
            created_at,
        );
        let stale = selected_path.tail().is_some();
        let binding_state = BindingState::unbound("new thread has no CAS projection")
            .expect("static unbound reason is valid");
        let transcript_build = (!stale).then(|| {
            crate::TranscriptBuildRecord::new(
                thread_id,
                TranscriptGeneration::FIRST,
                projection_revision,
                thread_revision,
                None,
                selected_path.digest(),
                0,
                0,
                crate::projection::transcript_entry_digest_seed(),
                true,
                crate::TranscriptBuildPhase::Complete,
            )
        });
        let execution = ThreadExecutionRecord::new(thread_id, execution_binding);
        let usage = ThreadUsageRecord::empty(thread_id);
        let summary = HistorySummaryRecord::new(
            thread_id,
            projection_revision,
            thread_revision,
            selected_path.tail(),
            selected_path.digest(),
            !stale,
            created_at,
        );
        let catalog_summary = ThreadCatalogSummaryRecord::initial_with_history_title(
            &thread,
            &execution,
            &attributes,
            &summary,
            initial_title,
        );
        InitialThreadRecords {
            thread,
            image_label_authority_head: ImageLabelAuthorityHeadV1::new(
                thread_id,
                1,
                inherited_labels,
                inherited_labels,
            )
            .expect("initial image-label authority head is valid"),
            draft_image_label_protection_head: DraftImageLabelProtectionHeadV1::new(
                thread_id,
                1,
                inherited_labels,
            )
            .expect("initial draft image-label protection head is valid"),
            execution,
            attributes,
            usage,
            catalog_summary,
            draft,
            draft_piece_root,
            draft_edit_history,
            draft_index: DraftByThreadRecord::new(
                thread_id,
                draft_id,
                draft_revision,
                thread_revision,
            ),
            transcript_head: TranscriptViewHeadRecord::new(
                thread_id,
                TranscriptGeneration::FIRST,
                projection_revision,
                0,
                selected_path.tail(),
                selected_path.digest(),
                if stale {
                    ProjectionLifecycle::Stale
                } else {
                    ProjectionLifecycle::Current
                },
            ),
            transcript_build,
            summary,
            input_gate: InputGateRecord::idle(thread_id),
            activity_head: crate::ActivityQueryHeadRecord::empty(thread_id),
            binding: BindingRecord::new(thread_id, binding_revision, selected_path, binding_state),
            binding_head: BindingHeadRecord::new(
                thread_id,
                binding_revision,
                BindingLifecycle::Unbound,
                selected_path.digest(),
            ),
        }
    }
}

impl InitialThreadRecords {
    pub(crate) fn ensure_absent(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        let records = self;
        let transcript_build_collision = match &records.transcript_build {
            Some(build) => point::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
            )?
            .is_some(),
            None => point::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: records.thread.id(),
                    generation: TranscriptGeneration::FIRST,
                },
            )?
            .is_some(),
        };
        if point::<ThreadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<DiscussionHandoffGatesFamily>(reader, &records.thread.id())?.is_some()
            || point::<ImageLabelAuthorityHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<DraftImageLabelProtectionHeadsFamily>(reader, &records.thread.id())?
                .is_some()
            || point::<ThreadExecutionsFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadAttributesFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadUsageFamily>(reader, &records.thread.id())?.is_some()
            || point::<ThreadCatalogSummariesFamily>(reader, &records.thread.id())?.is_some()
            || point::<DraftsFamily>(reader, &records.draft.id())?.is_some()
            || point::<DraftPieceRootsFamily>(reader, &records.draft_piece_root.reference().key())?
                .is_some()
            || point::<DraftEditHistoryFrontiersFamily>(
                reader,
                &records.draft_edit_history.reference().key(),
            )?
            .is_some()
            || point::<DraftByThreadFamily>(reader, &records.thread.id())?.is_some()
            || point::<TranscriptHeadsFamily>(reader, &records.thread.id())?.is_some()
            || transcript_build_collision
            || point::<HistorySummariesFamily>(reader, &records.thread.id())?.is_some()
            || current_input_gate(reader, &records.thread.id())?.is_some()
            || point::<ActivityQueryHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<BindingHeadsFamily>(reader, &records.thread.id())?.is_some()
            || point::<BindingsFamily>(
                reader,
                &BindingKey {
                    thread: records.thread.id(),
                    revision: records.binding.revision(),
                },
            )?
            .is_some()
            || point::<TurnsFamily>(
                reader,
                &SyndicTurnId::from_bytes(*records.draft.id().as_bytes()),
            )?
            .is_some()
            || point::<AcceptedInputsFamily>(reader, &records.draft.id().accepted_input_id())?
                .is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(())
    }

    pub(crate) fn reserve(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        self.reserve_with_catalog_summaries(reservation, 1)
    }

    pub(crate) fn reserve_with_catalog_summaries(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
        catalog_summaries: usize,
    ) -> Result<(), SyndicMutationError> {
        let records = self;
        reservation.reserve_records::<ThreadsCodec>(1)?;
        reservation.reserve_records::<ImageLabelAuthorityHeadsCodec>(1)?;
        reservation.reserve_records::<DraftImageLabelProtectionHeadsCodec>(1)?;
        reservation.reserve_records::<ThreadExecutionsCodec>(1)?;
        reservation.reserve_records::<ThreadAttributesCodec>(1)?;
        reservation.reserve_records::<ThreadUsageCodec>(1)?;
        reservation.reserve_records::<ThreadCatalogSummariesCodec>(catalog_summaries)?;
        reservation.reserve_records::<DraftsCodec>(1)?;
        reservation.reserve_records::<DraftPieceRootsCodec>(1)?;
        reservation.reserve_records::<DraftEditHistoryFrontiersCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        if records.transcript_build.is_some() {
            reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        }
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        reserve_input_gate(reservation)?;
        reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
        reservation.reserve_records::<BindingsCodec>(1)?;
        reservation.reserve_records::<BindingHeadsCodec>(1)?;
        Ok(())
    }

    pub(crate) fn put(
        &self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        let records = self;
        mutations.put::<ThreadsCodec>(&records.thread.id(), &records.thread)?;
        mutations.put::<ImageLabelAuthorityHeadsCodec>(
            &records.thread.id(),
            &records.image_label_authority_head,
        )?;
        mutations.put::<DraftImageLabelProtectionHeadsCodec>(
            &records.thread.id(),
            &records.draft_image_label_protection_head,
        )?;
        mutations.put::<ThreadExecutionsCodec>(&records.thread.id(), &records.execution)?;
        mutations.put::<ThreadAttributesCodec>(&records.thread.id(), &records.attributes)?;
        mutations.put::<ThreadUsageCodec>(&records.thread.id(), &records.usage)?;
        mutations
            .put::<ThreadCatalogSummariesCodec>(&records.thread.id(), &records.catalog_summary)?;
        mutations.put::<DraftsCodec>(&records.draft.id(), &records.draft)?;
        mutations.put::<DraftPieceRootsCodec>(
            &records.draft_piece_root.reference().key(),
            &records.draft_piece_root,
        )?;
        mutations.put::<DraftEditHistoryFrontiersCodec>(
            &records.draft_edit_history.reference().key(),
            &records.draft_edit_history,
        )?;
        mutations.put::<DraftByThreadCodec>(&records.thread.id(), &records.draft_index)?;
        mutations.put::<TranscriptHeadsCodec>(&records.thread.id(), &records.transcript_head)?;
        if let Some(build) = &records.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations.put::<HistorySummariesCodec>(&records.thread.id(), &records.summary)?;
        put_input_gate(mutations, &records.input_gate)?;
        mutations.put::<ActivityQueryHeadsCodec>(&records.thread.id(), &records.activity_head)?;
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: records.thread.id(),
                revision: records.binding.revision(),
            },
            &records.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&records.thread.id(), &records.binding_head)?;
        Ok(())
    }
}
