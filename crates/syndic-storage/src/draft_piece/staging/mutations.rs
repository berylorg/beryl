use super::*;

impl DomainMutation<SyndicDomain> for StagingMutation {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let p = &self.prepared;
        let stored_head =
            point::<DraftMutationStagingHeadsFamily>(reader, &p.target_head.identity())?;
        let stored_receipt = point::<DraftMutationStagingProgressFamily>(reader, &p.receipt.key())?;
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            p.source_session.draft_id(),
            p.source_session.session_id(),
        );
        let stored_session =
            match point::<DraftEditorCandidateSessionsFamily>(reader, &session_key)? {
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
                if point::<DraftPieceBuildsFamily>(reader, &build_key)?.is_some()
                    || point::<DraftPieceSettlementsFamily>(reader, &build_key)?.is_some()
                    || point::<DraftPieceRootsFamily>(reader, &root_key)?.is_some()
                    || point::<DraftMutationStagingPagesFamily>(reader, &source_page)?.is_some()
                    || point::<DraftMutationStagingPagesFamily>(reader, &proposal_page)?.is_some()
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
            return Ok(());
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
            return Ok(());
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
        Ok(())
    }
    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if point::<DraftMutationStagingHeadsFamily>(reader, &self.prepared.target_head.identity())?
            .as_ref()
            != Some(&self.prepared.target_head)
        {
            mutations.put::<DraftMutationStagingProgressCodec>(
                &self.prepared.receipt.key(),
                &self.prepared.receipt,
            )?;
            mutations.put::<DraftMutationStagingHeadsCodec>(
                &self.prepared.target_head.identity(),
                &self.prepared.target_head,
            )?;
            if let Some(session) = &self.prepared.target_session {
                mutations.put::<DraftEditorCandidateSessionsCodec>(
                    &DraftEditorCandidateSessionRecordKeyV1::head(
                        session.draft_id(),
                        session.session_id(),
                    ),
                    &DraftEditorCandidateSessionRecordV1::Head(session.clone()),
                )?;
            }
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for StagingBatchMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let prepared = &self.prepared;
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
            return Ok(());
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
        Ok(())
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
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let prepared = &self.prepared;
        if point::<DraftMutationStagingHeadsFamily>(reader, &prepared.source_head.identity())?
            .as_ref()
            == Some(&prepared.source_head)
        {
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

impl DomainMutation<SyndicDomain> for TransferMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let p = &self.prepared;
        let identity = p.target_head.identity();
        let build_key = DraftPieceSettlementKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            identity.draft_id(),
            identity.session_id(),
        );
        let stored_head = point::<DraftMutationStagingHeadsFamily>(reader, &identity)?;
        let stored_transfer =
            point::<DraftMutationStagingProgressFamily>(reader, &p.receipt.key())?;
        let stored_build = point::<DraftPieceBuildsFamily>(reader, &build_key)?;
        let stored_build_receipt =
            point::<DraftPieceBuildProgressFamily>(reader, &p.build_receipt.key())?;
        let stored_session = point::<DraftEditorCandidateSessionsFamily>(reader, &session_key)?
            .and_then(|record| match record {
                DraftEditorCandidateSessionRecordV1::Head(head) => Some(head),
                _ => None,
            })
            .ok_or(SyndicMutationError::CurrentDraftConflict)?;
        if stored_head.as_ref() == Some(&p.source_head) {
            if stored_transfer.is_some()
                || stored_build.is_some()
                || stored_build_receipt.is_some()
                || stored_session != p.source_session
                || point::<DraftPieceSettlementsFamily>(reader, &build_key)?.is_some()
                || point::<DraftPieceRootsFamily>(
                    reader,
                    &DraftPieceRootKeyV1::editor_candidate(
                        identity.draft_id(),
                        identity.session_id(),
                        identity.operation_id().as_piece_operation(),
                    ),
                )?
                .is_some()
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let receipt = authenticate_staging_head_reader(reader, &p.source_head)?;
            if receipt.command() != DraftMutationStagingCommandKindV1::Finish
                || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return Ok(());
        }
        if stored_head.as_ref() == Some(&p.target_head)
            && stored_transfer.as_ref() == Some(&p.receipt)
            && stored_build.as_ref() == Some(&p.build)
            && stored_build_receipt.as_ref() == Some(&p.build_receipt)
            && stored_session == p.target_session
            && point::<DraftPieceSettlementsFamily>(reader, &build_key)?.is_none()
            && point::<DraftPieceRootsFamily>(
                reader,
                &DraftPieceRootKeyV1::editor_candidate(
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                ),
            )?
            .is_none()
        {
            let receipt = authenticate_staging_head_reader(reader, &p.target_head)?;
            if receipt != p.receipt {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return Ok(());
        }
        Err(SyndicMutationError::IdentityCollision)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftMutationStagingProgressCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftMutationStagingHeadsCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let p = &self.prepared;
        if point::<DraftMutationStagingHeadsFamily>(reader, &p.target_head.identity())?.as_ref()
            != Some(&p.target_head)
        {
            mutations.put::<DraftMutationStagingProgressCodec>(&p.receipt.key(), &p.receipt)?;
            mutations
                .put::<DraftPieceBuildProgressCodec>(&p.build_receipt.key(), &p.build_receipt)?;
            mutations
                .put::<DraftMutationStagingHeadsCodec>(&p.target_head.identity(), &p.target_head)?;
            let build_key = DraftPieceSettlementKeyV1::new(
                p.build.draft_id(),
                p.build.session_id(),
                p.build.operation_id(),
            );
            mutations.put::<DraftPieceBuildsCodec>(&build_key, &p.build)?;
            let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
                p.target_session.draft_id(),
                p.target_session.session_id(),
            );
            mutations.put::<DraftEditorCandidateSessionsCodec>(
                &session_key,
                &DraftEditorCandidateSessionRecordV1::Head(p.target_session.clone()),
            )?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for StageDurablePageMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let p = &self.prepared;
        if point::<DraftMutationStagingHeadsFamily>(reader, &p.staging_head.identity())?.as_ref()
            != Some(&p.staging_head)
            || point::<DraftMutationStagingPagesFamily>(reader, &p.staging_page.key())?.as_ref()
                != Some(&p.staging_page)
        {
            return Err(SyndicMutationError::IdentityCollision);
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
            if stored_target_receipt.is_some()
                || stored_session != p.expected_session
                || p.fragments.iter().any(|fragment| {
                    point::<DraftPieceBuildFragmentsFamily>(reader, &fragment.key())
                        .ok()
                        .flatten()
                        .is_some()
                })
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            return Ok(());
        }
        if stored_build.as_ref() == Some(&p.target_build)
            && stored_target_receipt.as_ref() == Some(&p.target_receipt)
            && stored_session == p.target_session
            && p.fragments.iter().all(|fragment| {
                point::<DraftPieceBuildFragmentsFamily>(reader, &fragment.key())
                    .ok()
                    .flatten()
                    .as_ref()
                    == Some(fragment)
            })
        {
            return Ok(());
        }
        Err(SyndicMutationError::IdentityCollision)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<DraftPieceBuildFragmentsCodec>(self.prepared.fragments.len())?;
        reservation.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        reservation.reserve_records::<DraftPieceBuildsCodec>(1)?;
        reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let p = &self.prepared;
        let build_key = DraftPieceSettlementKeyV1::new(
            p.target_build.draft_id(),
            p.target_build.session_id(),
            p.target_build.operation_id(),
        );
        if point::<DraftPieceBuildsFamily>(reader, &build_key)?.as_ref() != Some(&p.target_build) {
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
        }
        Ok(())
    }
}
