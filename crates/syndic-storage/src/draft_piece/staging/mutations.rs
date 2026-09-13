mod transfer;

use super::*;

impl DomainMutation<SyndicDomain> for StagingMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<(
        PreparedDraftMutationStagingCommandV1,
        Option<PreparedDraftMarkerWriterBeginV1>,
        Option<PreparedDraftMarkerWriterTerminalV1>,
    )>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.writer_progress_allowed {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let p = &self.prepared;
        let begin_work = (p.source_head.is_none()
            && p.target_head.begin().writer_admission().is_some())
        .then(|| AdmissionWorkLedger::new(DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES));
        let stored_head = draft_marker_writer_point::<DraftMutationStagingHeadsFamily>(
            reader,
            &p.target_head.identity(),
            begin_work.as_ref(),
        )?;
        let stored_receipt = draft_marker_writer_point::<DraftMutationStagingProgressFamily>(
            reader,
            &p.receipt.key(),
            begin_work.as_ref(),
        )?;
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            p.source_session.draft_id(),
            p.source_session.session_id(),
        );
        let stored_session = match draft_marker_writer_point::<DraftEditorCandidateSessionsFamily>(
            reader,
            &session_key,
            begin_work.as_ref(),
        )? {
            Some(DraftEditorCandidateSessionRecordV1::Head(head)) => Some(head),
            Some(DraftEditorCandidateSessionRecordV1::OpenReceipt(_)) => {
                return Err(SyndicMutationError::IdentityCollision);
            }
            None => None,
        };
        if stored_head.as_ref() == p.source_head.as_ref() {
            if stored_receipt.is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            if p.source_head.is_none() {
                let identity = p.target_head.identity();
                let operation = identity.operation_id().as_piece_operation();
                let build_key = DraftPieceSettlementKeyV1::new(
                    identity.draft_id(),
                    identity.session_id(),
                    operation,
                );
                let root_key = DraftPieceRootKeyV1::editor_candidate(
                    identity.draft_id(),
                    identity.session_id(),
                    operation,
                );
                let source_page = DraftMutationStagingPageKeyV1::new(
                    identity,
                    DraftMutationStagingLaneV1::Source,
                    1,
                )
                .ok_or(SyndicMutationError::IdentityCollision)?;
                let proposal_page = DraftMutationStagingPageKeyV1::new(
                    identity,
                    DraftMutationStagingLaneV1::Proposal,
                    1,
                )
                .ok_or(SyndicMutationError::IdentityCollision)?;
                if draft_marker_writer_point::<DraftPieceBuildsFamily>(
                    reader,
                    &build_key,
                    begin_work.as_ref(),
                )?
                .is_some()
                    || draft_marker_writer_point::<DraftPieceSettlementsFamily>(
                        reader,
                        &build_key,
                        begin_work.as_ref(),
                    )?
                    .is_some()
                    || draft_marker_writer_point::<DraftPieceRootsFamily>(
                        reader,
                        &root_key,
                        begin_work.as_ref(),
                    )?
                    .is_some()
                    || draft_marker_writer_point::<DraftMutationStagingPagesFamily>(
                        reader,
                        &source_page,
                        begin_work.as_ref(),
                    )?
                    .is_some()
                    || draft_marker_writer_point::<DraftMutationStagingPagesFamily>(
                        reader,
                        &proposal_page,
                        begin_work.as_ref(),
                    )?
                    .is_some()
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            } else {
                authenticate_staging_head_reader(
                    reader,
                    p.source_head
                        .as_ref()
                        .ok_or(SyndicMutationError::IdentityCollision)?,
                )?;
            }
            if stored_session.as_ref() != Some(&p.source_session) {
                return Err(SyndicMutationError::CurrentDraftConflict);
            }
            if let Some(work) = begin_work.as_ref() {
                charge_draft_marker_writer_emit::<DraftMutationStagingHeadsFamily>(
                    work,
                    &p.target_head.identity(),
                    &p.target_head,
                )?;
                charge_draft_marker_writer_emit::<DraftMutationStagingProgressFamily>(
                    work,
                    &p.receipt.key(),
                    &p.receipt,
                )?;
                if let Some(session) = p.target_session.as_ref() {
                    charge_draft_marker_writer_emit::<DraftEditorCandidateSessionsFamily>(
                        work,
                        &session_key,
                        &DraftEditorCandidateSessionRecordV1::Head(session.clone()),
                    )?;
                }
            }
            let writer = if p.source_head.is_none() {
                p.target_head
                    .begin()
                    .writer_admission()
                    .map(|admission| {
                        prepare_draft_marker_writer_begin_v1(
                            reader,
                            admission,
                            begin_work
                                .as_ref()
                                .ok_or(SyndicMutationError::IdentityCollision)?,
                        )
                    })
                    .transpose()?
            } else {
                None
            };
            let writer_terminal = if p.source_head.is_some()
                && matches!(
                    p.target_head.lifecycle(),
                    DraftMutationStagingLifecycleV1::Cancelled
                        | DraftMutationStagingLifecycleV1::Rejected
                        | DraftMutationStagingLifecycleV1::Conflict
                        | DraftMutationStagingLifecycleV1::Error
                ) {
                p.target_head
                    .begin()
                    .writer_admission()
                    .map(|admission| {
                        prepare_draft_marker_writer_terminal_v1(
                            reader,
                            admission,
                            p.receipt.digest(),
                        )
                    })
                    .transpose()?
            } else {
                None
            };
            return Ok(Some((self.prepared, writer, writer_terminal)));
        }
        if stored_head.as_ref() == Some(&p.target_head)
            && stored_receipt.as_ref() == Some(&p.receipt)
            && (matches!(
                p.target_head.lifecycle(),
                DraftMutationStagingLifecycleV1::Cancelled
                    | DraftMutationStagingLifecycleV1::Rejected
                    | DraftMutationStagingLifecycleV1::Conflict
                    | DraftMutationStagingLifecycleV1::Error
            ) || p
                .target_session
                .as_ref()
                .is_none_or(|target| stored_session.as_ref() == Some(target)))
        {
            authenticate_staging_head_reader(reader, &p.target_head)?;
            if matches!(
                p.target_head.lifecycle(),
                DraftMutationStagingLifecycleV1::Cancelled
                    | DraftMutationStagingLifecycleV1::Rejected
                    | DraftMutationStagingLifecycleV1::Conflict
                    | DraftMutationStagingLifecycleV1::Error
            ) && stored_session.as_ref().is_some_and(|session| {
                terminal_session_has_same_operation_custody(session, p.target_head.identity())
            }) {
                return Err(SyndicMutationError::IdentityCollision);
            }
            if let Some(admission) = p.target_head.begin().writer_admission() {
                let exact = if matches!(
                    p.target_head.lifecycle(),
                    DraftMutationStagingLifecycleV1::Cancelled
                        | DraftMutationStagingLifecycleV1::Rejected
                        | DraftMutationStagingLifecycleV1::Conflict
                        | DraftMutationStagingLifecycleV1::Error
                ) {
                    draft_marker_writer_terminal_is_exact_v1(reader, admission, p.receipt.digest())?
                } else {
                    draft_marker_writer_head_is_exact_v1(
                        reader,
                        admission,
                        DraftMarkerAdmissionLifecycleV1::Staging,
                    )?
                };
                if !exact {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            return Ok(None);
        }
        Err(SyndicMutationError::IdentityCollision)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMutationStagingHeadsCodec>(1)?;
        reservation.reserve_records::<DraftMutationStagingProgressCodec>(1)?;
        if self.prepared.target_session.is_some() {
            reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        }
        if self.prepared.source_head.is_none()
            && self
                .prepared
                .target_head
                .begin()
                .writer_admission()
                .is_some()
        {
            reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
            reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
            reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
            reservation.reserve_records::<DraftMarkerAdmissionNodesCodec>(
                crate::draft_piece::admission::DRAFT_MARKER_ADMISSION_MAX_RETAINED_PREDECESSORS,
            )?;
        }
        if self.prepared.source_head.is_some()
            && self
                .prepared
                .target_head
                .begin()
                .writer_admission()
                .is_some()
            && matches!(
                self.prepared.target_head.lifecycle(),
                DraftMutationStagingLifecycleV1::Cancelled
                    | DraftMutationStagingLifecycleV1::Rejected
                    | DraftMutationStagingLifecycleV1::Conflict
                    | DraftMutationStagingLifecycleV1::Error
            )
        {
            reservation.reserve_records::<DraftMarkerAdmissionCapacityCodec>(1)?;
            reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
            reservation.reserve_records::<DraftMarkerAdmissionReceiptsCodec>(1)?;
        }
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some((prepared, writer, writer_terminal)) = prepared {
            mutations.put::<DraftMutationStagingProgressCodec>(
                &prepared.receipt.key(),
                &prepared.receipt,
            )?;
            mutations.put::<DraftMutationStagingHeadsCodec>(
                &prepared.target_head.identity(),
                &prepared.target_head,
            )?;
            if let Some(session) = &prepared.target_session {
                mutations.put::<DraftEditorCandidateSessionsCodec>(
                    &DraftEditorCandidateSessionRecordKeyV1::head(
                        session.draft_id(),
                        session.session_id(),
                    ),
                    &DraftEditorCandidateSessionRecordV1::Head(session.clone()),
                )?;
            }
            if let Some(writer) = writer {
                contribute_draft_marker_writer_begin_v1(writer, mutations)?;
            }
            if let Some(writer_terminal) = writer_terminal {
                contribute_draft_marker_writer_terminal_v1(writer_terminal, mutations)?;
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for StagingBatchMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftMutationStagingBatchV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.writer_progress_allowed {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let prepared = &self.prepared;
        if prepared.source_head.begin().writer_admission().is_none()
            && !(prepared.permits_unadmitted_marker_builder()
                && unadmitted_marker_builder_is_authorized_for_test(
                    DraftPieceSettlementKeyV1::new(
                        prepared.source_head.identity().draft_id(),
                        prepared.source_head.identity().session_id(),
                        prepared
                            .source_head
                            .identity()
                            .operation_id()
                            .as_piece_operation(),
                    ),
                ))
            && prepared.targets.iter().any(|target| {
                target.page.items().iter().any(|item| {
                    matches!(
                        item,
                        DraftMutationStagingPageItemV1::Proposal(replacement)
                            if replacement.marker_effect().is_some()
                    )
                })
            })
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let identity = prepared.source_head.identity();
        let stored_head = point::<DraftMutationStagingHeadsFamily>(reader, &identity)?;
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            prepared.source_session.draft_id(),
            prepared.source_session.session_id(),
        );
        let stored_session =
            match point::<DraftEditorCandidateSessionsFamily>(reader, &session_key)? {
                Some(DraftEditorCandidateSessionRecordV1::Head(head)) => head,
                _ => return Err(SyndicMutationError::IdentityCollision),
            };

        if stored_head.as_ref() == Some(&prepared.source_head) {
            authenticate_staging_head_reader(reader, &prepared.source_head)?;
            if stored_session != prepared.source_session {
                return Err(SyndicMutationError::IdentityCollision);
            }
            for target in prepared.targets.iter() {
                if point::<DraftMutationStagingPagesFamily>(reader, &target.page.key())?.is_some()
                    || point::<DraftMutationStagingProgressFamily>(reader, &target.receipt.key())?
                        .is_some()
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            return Ok(Some(self.prepared));
        }

        if stored_head.as_ref() != Some(&prepared.target_head)
            || stored_session != prepared.target_session
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        for target in prepared.targets.iter() {
            if point::<DraftMutationStagingPagesFamily>(reader, &target.page.key())?.as_ref()
                != Some(&target.page)
                || point::<DraftMutationStagingProgressFamily>(reader, &target.receipt.key())?
                    .as_ref()
                    != Some(&target.receipt)
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        authenticate_staging_head_reader(reader, &prepared.target_head)?;
        Ok(None)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<DraftMutationStagingPagesCodec>(self.prepared.targets.len())?;
        reservation
            .reserve_records::<DraftMutationStagingProgressCodec>(self.prepared.targets.len())?;
        reservation.reserve_records::<DraftMutationStagingHeadsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            let prepared = &prepared;
            for target in prepared.targets.iter() {
                mutations
                    .put::<DraftMutationStagingPagesCodec>(&target.page.key(), &target.page)?;
                mutations.put::<DraftMutationStagingProgressCodec>(
                    &target.receipt.key(),
                    &target.receipt,
                )?;
            }
            mutations.put::<DraftMutationStagingHeadsCodec>(
                &prepared.target_head.identity(),
                &prepared.target_head,
            )?;
            mutations.put::<DraftEditorCandidateSessionsCodec>(
                &DraftEditorCandidateSessionRecordKeyV1::head(
                    prepared.target_session.draft_id(),
                    prepared.target_session.session_id(),
                ),
                &DraftEditorCandidateSessionRecordV1::Head(prepared.target_session.clone()),
            )?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for StageDurableWindowMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<PreparedDraftPieceStagingWindowV1>;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.writer_progress_allowed {
            return Err(SyndicMutationError::IdentityCollision);
        }
        let p = &self.prepared;
        if point::<DraftMutationStagingHeadsFamily>(reader, &p.staging_head.identity())?.as_ref()
            != Some(&p.staging_head)
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_staging_head_reader(reader, &p.staging_head)?;
        for page in p.staging_pages.iter() {
            if point::<DraftMutationStagingPagesFamily>(reader, &page.key())?.as_ref() != Some(page)
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            authenticate_staging_page_reader(reader, &p.staging_head, page)?;
        }
        let build_key = DraftPieceSettlementKeyV1::new(
            p.expected_build.draft_id(),
            p.expected_build.session_id(),
            p.expected_build.operation_id(),
        );
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            p.expected_session.draft_id(),
            p.expected_session.session_id(),
        );
        let stored_build = point::<DraftPieceBuildsFamily>(reader, &build_key)?;
        let stored_target_receipt =
            point::<DraftPieceBuildProgressFamily>(reader, &p.target_receipt.key())?;
        let stored_session = point::<DraftEditorCandidateSessionsFamily>(reader, &session_key)?
            .and_then(|record| match record {
                DraftEditorCandidateSessionRecordV1::Head(head) => Some(head),
                _ => None,
            })
            .ok_or(SyndicMutationError::CurrentDraftConflict)?;
        if stored_build.as_ref() == Some(&p.expected_build) {
            super::mutation::authenticate_build(reader, &p.expected_build)?;
            if stored_target_receipt.is_some() || stored_session != p.expected_session {
                return Err(SyndicMutationError::IdentityCollision);
            }
            for fragment in p.fragments.iter() {
                if point::<DraftPieceBuildFragmentsFamily>(reader, &fragment.key())?.is_some() {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            return Ok(Some(self.prepared));
        }
        if stored_build.as_ref() == Some(&p.target_build)
            && stored_target_receipt.as_ref() == Some(&p.target_receipt)
            && stored_session == p.target_session
        {
            super::mutation::authenticate_build(reader, &p.target_build)?;
            for fragment in p.fragments.iter() {
                if point::<DraftPieceBuildFragmentsFamily>(reader, &fragment.key())?.as_ref()
                    != Some(fragment)
                {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
            return Ok(None);
        }
        Err(SyndicMutationError::IdentityCollision)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if !self.prepared.fragments.is_empty() {
            reservation
                .reserve_records::<DraftPieceBuildFragmentsCodec>(self.prepared.fragments.len())?;
        }
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let Some(prepared) = prepared else {
            return Ok(());
        };
        let p = &prepared;
        let build_key = DraftPieceSettlementKeyV1::new(
            p.target_build.draft_id(),
            p.target_build.session_id(),
            p.target_build.operation_id(),
        );
        for fragment in p.fragments.iter() {
            mutations.put::<DraftPieceBuildFragmentsCodec>(&fragment.key(), fragment)?;
        }
        mutations
            .put::<DraftPieceBuildProgressCodec>(&p.target_receipt.key(), &p.target_receipt)?;
        mutations.put::<DraftPieceBuildsCodec>(&build_key, &p.target_build)?;
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            p.target_session.draft_id(),
            p.target_session.session_id(),
        );
        mutations.put::<DraftEditorCandidateSessionsCodec>(
            &session_key,
            &DraftEditorCandidateSessionRecordV1::Head(p.target_session.clone()),
        )?;
        Ok(())
    }
}
