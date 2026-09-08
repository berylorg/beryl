use super::*;

impl SyndicStorage {
    pub fn prepare_staged_draft_piece_transfer(
        &self,
        store: &HomeStore,
        identity: DraftMutationStagingIdentityV1,
        finished_receipt: DraftMutationStagingProgressReceiptReferenceV1,
    ) -> Result<PreparedStagedDraftPieceCommandV1, StagedDraftPiecePreparationErrorV1> {
        check_generation(self, store)?;
        let head = staging_head(self, store, identity)?;
        if head.receipt() != finished_receipt
            || !matches!(
                head.lifecycle(),
                DraftMutationStagingLifecycleV1::Finished(_)
            )
        {
            return Err(StagedDraftPiecePreparationErrorV1::StaleEndpoint);
        }
        let session_key = DraftEditorCandidateSessionRecordKeyV1::head(
            identity.draft_id(),
            identity.session_id(),
        );
        let session = match self.point::<DraftEditorCandidateSessionsFamily>(
            store,
            session_key,
            point_limit(),
        )? {
            Some(DraftEditorCandidateSessionRecordV1::Head(head)) => head,
            _ => return Err(StagedDraftPiecePreparationErrorV1::StaleEndpoint),
        };
        let transfer = self.prepare_draft_mutation_staging_transfer(&head, &session)?;
        Ok(PreparedStagedDraftPieceCommandV1 {
            storage: self.clone(),
            source: Box::new(CapturedState {
                staging: head,
                build: None,
                session,
                settlement: None,
                terminal_admission: None,
            }),
            command: CommandKind::Transfer(Box::new(transfer)),
        })
    }

    pub fn prepare_staged_draft_piece_window(
        &self,
        store: &HomeStore,
        identity: DraftMutationStagingIdentityV1,
        endpoint: DraftPieceBuildProgressReceiptReferenceV1,
        limits: DraftPieceDurableBuildWindowLimitsV1,
    ) -> Result<Option<PreparedStagedDraftPieceCommandV1>, StagedDraftPiecePreparationErrorV1> {
        check_generation(self, store)?;
        let head = staging_head(self, store, identity)?;
        let key = DraftPieceSettlementKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        let build = self
            .point::<DraftPieceBuildsFamily>(store, key, point_limit())?
            .ok_or(StagedDraftPiecePreparationErrorV1::StaleEndpoint)?;
        check_build(&head, &build, endpoint)?;
        let Some(window) =
            self.prepare_next_durable_draft_piece_window(store, identity, endpoint, limits)?
        else {
            return Ok(None);
        };
        Ok(Some(PreparedStagedDraftPieceCommandV1 {
            storage: self.clone(),
            source: Box::new(CapturedState {
                staging: window.staging_head.clone(),
                build: Some(window.expected_build.clone()),
                session: window.expected_session.clone(),
                settlement: None,
                terminal_admission: None,
            }),
            command: CommandKind::Window(Box::new(window)),
        }))
    }

    pub fn prepare_staged_draft_piece_advance(
        &self,
        store: &HomeStore,
        identity: DraftMutationStagingIdentityV1,
        endpoint: DraftPieceBuildProgressReceiptReferenceV1,
    ) -> Result<Option<PreparedStagedDraftPieceCommandV1>, StagedDraftPiecePreparationErrorV1> {
        check_generation(self, store)?;
        let Some(advance) = self.prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )?
        else {
            return Ok(None);
        };
        let head = match &advance.bounded {
            Some(source) => source
                .staging
                .clone()
                .ok_or(StagedDraftPiecePreparationErrorV1::StaleEndpoint)?,
            None => staging_head(self, store, identity)?,
        };
        check_build(&head, &advance.expected, endpoint)?;
        if self.revision(store).map_err(crate::SyndicReadError::from)? != advance.expected_revision
        {
            return Err(StagedDraftPiecePreparationErrorV1::StaleEndpoint);
        }
        Ok(Some(PreparedStagedDraftPieceCommandV1 {
            storage: self.clone(),
            source: Box::new(CapturedState {
                staging: head,
                build: Some(advance.expected.clone()),
                session: advance.expected_session.clone(),
                settlement: None,
                terminal_admission: None,
            }),
            command: CommandKind::Advance(Box::new(advance)),
        }))
    }

    pub fn prepare_staged_draft_piece_terminal(
        &self,
        store: &HomeStore,
        identity: DraftMutationStagingIdentityV1,
        endpoint: DraftPieceBuildProgressReceiptReferenceV1,
        election: StagedDraftPieceTerminalElectionV1,
    ) -> Result<PreparedStagedDraftPieceCommandV1, StagedDraftPiecePreparationErrorV1> {
        check_generation(self, store)?;
        let key = DraftPieceSettlementKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        let (build, session) = authenticated_build_from_store(self, store, key)?
            .ok_or(StagedDraftPiecePreparationErrorV1::StaleEndpoint)?;
        let head = staging_head(self, store, identity)?;
        check_build(&head, &build, endpoint)?;
        let edit = prepared_edit_from_staging_build(&build, &session)
            .map_err(|_| StagedDraftPiecePreparationErrorV1::StaleEndpoint)?;
        Ok(PreparedStagedDraftPieceCommandV1 {
            storage: self.clone(),
            source: Box::new(CapturedState {
                staging: head,
                build: Some(build),
                session,
                settlement: None,
                terminal_admission: None,
            }),
            command: CommandKind::Terminal(Box::new(edit), election),
        })
    }
}

fn check_generation(
    storage: &SyndicStorage,
    store: &HomeStore,
) -> Result<(), StagedDraftPiecePreparationErrorV1> {
    if store.health().generation() != Some(storage.home_generation)
        || storage.revision(store).is_err()
    {
        return Err(StagedDraftPiecePreparationErrorV1::Unavailable);
    }
    Ok(())
}

fn staging_head(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> Result<DraftMutationStagingHeadV1, StagedDraftPiecePreparationErrorV1> {
    storage
        .point::<DraftMutationStagingHeadsFamily>(store, identity, point_limit())?
        .filter(|head| head.identity() == identity)
        .ok_or(StagedDraftPiecePreparationErrorV1::StaleEndpoint)
}

fn check_build(
    head: &DraftMutationStagingHeadV1,
    build: &DraftPieceBuildRecordV1,
    endpoint: DraftPieceBuildProgressReceiptReferenceV1,
) -> Result<(), StagedDraftPiecePreparationErrorV1> {
    let Some(continuation) = build.durable_continuation() else {
        return Err(StagedDraftPiecePreparationErrorV1::StaleEndpoint);
    };
    if build.progress_receipt() != endpoint
        || continuation.finished().identity() != head.identity()
        || !matches!(
            head.lifecycle(),
            DraftMutationStagingLifecycleV1::Building(_)
        )
        || continuation.finished().source() != head.source()
        || continuation.finished().proposal() != head.proposal()
    {
        return Err(StagedDraftPiecePreparationErrorV1::StaleEndpoint);
    }
    Ok(())
}
