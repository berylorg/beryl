use super::*;

impl SyndicStorage {
    pub fn transfer_draft_mutation_staging_to_builder(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMutationTransferV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, TransferMutation { prepared })
    }

    pub fn prepare_next_durable_draft_piece_page(
        &self,
        store: &beryl_home_store::HomeStore,
        identity: DraftMutationStagingIdentityV1,
    ) -> Result<Option<PreparedDraftPieceStagingPageV1>, DraftMutationStagingErrorV1> {
        let status = self.draft_mutation_staging_status(store, identity)?;
        let DraftMutationStagingStatusV1::Building { .. } = status else {
            return Err(DraftMutationStagingErrorV1::Invalid);
        };
        let head = self
            .draft_mutation_staging_head(store, identity)?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let key = DraftPieceSettlementKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        let (build, session) =
            super::mutation::authenticated_staging_build_from_store(self, store, key)
                .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if !matches!(
            build.frontier(),
            DraftPieceBuildFrontierV1::Receiving { .. }
        ) {
            return Ok(None);
        }
        let consumed = build
            .progress_receipt()
            .key()
            .transition_ordinal()
            .checked_sub(1)
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let source_pages = head
            .source()
            .next_ordinal()
            .checked_sub(1)
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let proposal_pages = head
            .proposal()
            .next_ordinal()
            .checked_sub(1)
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let (lane, ordinal) = if consumed < source_pages {
            (DraftMutationStagingLaneV1::Source, consumed + 1)
        } else {
            let proposal_consumed = consumed
                .checked_sub(source_pages)
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            if proposal_consumed >= proposal_pages {
                if build.staged_fragment_count() == build.fragment_count() {
                    return Ok(None);
                }
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            (DraftMutationStagingLaneV1::Proposal, proposal_consumed + 1)
        };
        let page_key = DraftMutationStagingPageKeyV1::new(identity, lane, ordinal)
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let page = self
            .draft_mutation_staging_page(store, page_key)?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if page.key() != page_key {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        authenticate_staging_page_closure(self, store, &head, &page)?;
        let replacements: Box<[DraftPieceReplacementV1]> = match lane {
            DraftMutationStagingLaneV1::Source => {
                for item in page.items() {
                    let DraftMutationStagingPageItemV1::SourcePosition(position) = item else {
                        return Err(DraftMutationStagingErrorV1::Invariant);
                    };
                    validate_position(self, store, build.predecessor_root(), *position)
                        .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
                }
                Box::new([])
            }
            DraftMutationStagingLaneV1::Proposal => page
                .items()
                .iter()
                .map(|item| match item {
                    DraftMutationStagingPageItemV1::Proposal(replacement) => {
                        Ok(replacement.clone())
                    }
                    _ => Err(DraftMutationStagingErrorV1::Invariant),
                })
                .collect::<Result<Box<[_]>, _>>()?,
        };
        let prepared_edit = super::mutation::prepared_edit_from_staging_build(&build, &session)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
        let (target_build, target_receipt, target_session, fragments) =
            super::mutation::staged_page_transition(
                &prepared_edit,
                &build,
                &session,
                &replacements,
            )
            .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
        Ok(Some(PreparedDraftPieceStagingPageV1 {
            staging_head: head,
            staging_page: page,
            expected_build: build,
            expected_session: session,
            target_build,
            target_receipt,
            target_session,
            fragments,
        }))
    }

    pub fn stage_next_durable_draft_piece_page(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceStagingPageV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StageDurablePageMutation { prepared },
        )
    }

    pub fn draft_mutation_staging_command(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingCommandV1,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, StagingMutation { prepared })
    }

    pub fn draft_mutation_staging_head(
        &self,
        store: &beryl_home_store::HomeStore,
        identity: DraftMutationStagingIdentityV1,
    ) -> Result<Option<DraftMutationStagingHeadV1>, SyndicReadError> {
        self.point::<DraftMutationStagingHeadsFamily>(
            store,
            identity,
            crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero"),
        )
    }

    pub fn draft_mutation_staging_page(
        &self,
        store: &beryl_home_store::HomeStore,
        key: DraftMutationStagingPageKeyV1,
    ) -> Result<Option<DraftMutationStagingPageV1>, SyndicReadError> {
        self.point::<DraftMutationStagingPagesFamily>(
            store,
            key,
            crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero"),
        )
    }
}
