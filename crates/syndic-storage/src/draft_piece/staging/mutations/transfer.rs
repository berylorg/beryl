use super::*;

impl DomainMutation<SyndicDomain> for TransferMutation {
    type Error = SyndicMutationError;
    type Prepared = Option<
        Box<(
            PreparedDraftMutationTransferV1,
            Option<DraftMarkerAdmissionHeadV1>,
        )>,
    >;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.writer_progress_allowed {
            return Err(SyndicMutationError::IdentityCollision);
        }
        match prepare_transfer(reader, &self.prepared)? {
            TransferPreparation::Apply(writer_head) => {
                Ok(Some(Box::new((self.prepared, writer_head))))
            }
            TransferPreparation::Replay => Ok(None),
        }
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
        if self.prepared.build.writer_admission().is_some() {
            reservation.reserve_records::<DraftMarkerAdmissionHeadsCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(prepared) = prepared {
            let (p, writer_head) = prepared.as_ref();
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
            if let Some(head) = writer_head {
                mutations.put::<DraftMarkerAdmissionHeadsCodec>(&head.owner(), head)?;
            }
        }
        Ok(())
    }
}

enum TransferPreparation {
    Apply(Option<DraftMarkerAdmissionHeadV1>),
    Replay,
}

#[inline(never)]
fn prepare_transfer(
    reader: &DomainReader<'_, SyndicDomain>,
    p: &PreparedDraftMutationTransferV1,
) -> Result<TransferPreparation, SyndicMutationError> {
    let identity = p.target_head.identity();
    let build_key = DraftPieceSettlementKeyV1::new(
        identity.draft_id(),
        identity.session_id(),
        identity.operation_id().as_piece_operation(),
    );
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(identity.draft_id(), identity.session_id());
    let stored_head = point::<DraftMutationStagingHeadsFamily>(reader, &identity)?;
    let stored_transfer = point::<DraftMutationStagingProgressFamily>(reader, &p.receipt.key())?;
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
        let writer_head = p
            .build
            .writer_admission()
            .map(|admission| prepare_draft_marker_writer_building_v1(reader, admission))
            .transpose()?;
        return Ok(TransferPreparation::Apply(writer_head));
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
        if let Some(admission) = p.build.writer_admission()
            && !draft_marker_writer_head_is_exact_v1(
                reader,
                admission,
                DraftMarkerAdmissionLifecycleV1::Building,
            )?
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        return Ok(TransferPreparation::Replay);
    }
    Err(SyndicMutationError::IdentityCollision)
}
