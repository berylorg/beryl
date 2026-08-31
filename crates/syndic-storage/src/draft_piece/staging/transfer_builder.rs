use super::*;

impl SyndicStorage {
    pub fn transfer_draft_mutation_staging_to_builder(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMutationTransferV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            TransferMutation {
                writer_progress_allowed: admitted_writer_is_current_generation(
                    self,
                    &prepared.source_head,
                ),
                prepared,
            },
        )
    }

    pub fn prepare_next_durable_draft_piece_window(
        &self,
        store: &beryl_home_store::HomeStore,
        identity: DraftMutationStagingIdentityV1,
        current_endpoint: DraftPieceBuildProgressReceiptReferenceV1,
        limits: DraftPieceDurableBuildWindowLimitsV1,
    ) -> Result<Option<PreparedDraftPieceStagingWindowV1>, DraftMutationStagingErrorV1> {
        let mut acquisition = StagingWindowAcquisitionReader::new(self, store);
        let head = acquisition
            .point::<DraftMutationStagingHeadsFamily>(identity)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if head.identity() != identity || !draft_mutation_staging_head_is_locally_exact(&head) {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        if !admitted_writer_is_current_generation(self, &head) {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let DraftMutationStagingLifecycleV1::Building(staging_build_endpoint) = head.lifecycle()
        else {
            return Err(DraftMutationStagingErrorV1::Invalid);
        };
        let receipt_key = DraftMutationStagingProgressReceiptKeyV1::new(
            identity,
            head.receipt().transition_ordinal(),
        )
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let staging_receipt = acquisition
            .point::<DraftMutationStagingProgressFamily>(receipt_key)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if !draft_mutation_staging_receipt_is_locally_exact(&staging_receipt)
            || !receipt_finish_digest_is_exact(&staging_receipt)
            || staging_receipt.digest() != head.receipt().digest()
            || staging_receipt.after_head_digest() != head.digest()
            || staging_receipt.after_source() != head.source()
            || staging_receipt.after_proposal() != head.proposal()
            || staging_receipt.after_lifecycle() != head.lifecycle()
            || staging_receipt.command() != DraftMutationStagingCommandKindV1::Transfer
            || staging_receipt.custody_after() != DraftMutationStagingCustodyTagV1::Building
            || staging_receipt.build_endpoint() != Some(staging_build_endpoint)
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let key = DraftPieceSettlementKeyV1::new(
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        );
        let (build, session) =
            super::mutation::authenticated_staging_window_build_from_store(&mut acquisition, key)
                .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        let current_key = build.progress_receipt().key();
        let staging_key = staging_build_endpoint.key();
        if build.progress_receipt() != current_endpoint
            || current_key.draft_id() != staging_key.draft_id()
            || current_key.session_id() != staging_key.session_id()
            || current_key.operation_id() != staging_key.operation_id()
            || current_key.transition_ordinal() < staging_key.transition_ordinal()
        {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        if !matches!(
            build.frontier(),
            DraftPieceBuildFrontierV1::Receiving { .. }
        ) {
            return Ok(None);
        }
        let continuation = build
            .durable_continuation()
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
        if continuation.finished().identity() != identity
            || head.source() != continuation.finished().source()
            || head.proposal() != continuation.finished().proposal()
        {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let Some(lane) = continuation.lane() else {
            return Ok(None);
        };
        let final_frontier = match lane {
            DraftMutationStagingLaneV1::Source => continuation.finished().source(),
            DraftMutationStagingLaneV1::Proposal => continuation.finished().proposal(),
        };
        let mut frontier = match lane {
            DraftMutationStagingLaneV1::Source => continuation.source(),
            DraftMutationStagingLaneV1::Proposal => continuation.proposal(),
        };
        let mut pages = Vec::new();
        let mut replacements = Vec::new();
        let mut inserted_utf8_bytes = 0usize;
        let mut acquired_pages = 0usize;
        while frontier != final_frontier && acquired_pages < limits.page_limit() {
            if lane == DraftMutationStagingLaneV1::Proposal
                && replacements.len() == limits.fragment_limit()
            {
                break;
            }
            let page_key =
                DraftMutationStagingPageKeyV1::new(identity, lane, frontier.next_ordinal())
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            let page = acquisition
                .point::<DraftMutationStagingPagesFamily>(page_key)
                .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            acquired_pages = acquired_pages
                .checked_add(1)
                .ok_or(DraftMutationStagingErrorV1::Overflow)?;
            if page.key() != page_key
                || page.items().len() != 1
                || page.input_cursor() != frontier.next_cursor()
                || page.prior_cumulative_identity() != frontier.cumulative_identity()
            {
                return Err(DraftMutationStagingErrorV1::Invariant);
            }
            let receipt_key =
                DraftMutationStagingProgressReceiptKeyV1::new(identity, page.transition_ordinal())
                    .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            let receipt = acquisition
                .point::<DraftMutationStagingProgressFamily>(receipt_key)
                .map_err(|_| DraftMutationStagingErrorV1::Invariant)?
                .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            authenticate_staging_page_receipt_for_window(&head, &page, &receipt)?;
            let replacement = match (lane, &page.items()[0]) {
                (
                    DraftMutationStagingLaneV1::Source,
                    DraftMutationStagingPageItemV1::SourcePosition(position),
                ) => {
                    validate_position(self, store, build.predecessor_root(), *position)
                        .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
                    None
                }
                (
                    DraftMutationStagingLaneV1::Proposal,
                    DraftMutationStagingPageItemV1::Proposal(replacement),
                ) => Some(replacement.clone()),
                _ => return Err(DraftMutationStagingErrorV1::Invariant),
            };
            if let Some(replacement) = replacement {
                let replacement_utf8_bytes = replacement_inserted_utf8_bytes(&replacement)?;
                let next_utf8_bytes = inserted_utf8_bytes
                    .checked_add(replacement_utf8_bytes)
                    .ok_or(DraftMutationStagingErrorV1::Overflow)?;
                if next_utf8_bytes > limits.inserted_utf8_byte_limit() {
                    if pages.is_empty() {
                        return Err(DraftMutationStagingErrorV1::Invalid);
                    }
                    break;
                }
                inserted_utf8_bytes = next_utf8_bytes;
                replacements.push(replacement);
            }
            frontier = DraftMutationStagingLaneFrontierV1::new(
                page.successor_cursor(),
                page.key()
                    .ordinal()
                    .checked_add(1)
                    .ok_or(DraftMutationStagingErrorV1::Overflow)?,
                page.cumulative_item_total(),
                page.cumulative_byte_total(),
                page.successor_cumulative_identity(),
            )
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
            pages.push(page);
        }
        if pages.is_empty() {
            return Err(DraftMutationStagingErrorV1::Invalid);
        }
        let (source, proposal) = match lane {
            DraftMutationStagingLaneV1::Source => (frontier, continuation.proposal()),
            DraftMutationStagingLaneV1::Proposal => (continuation.source(), frontier),
        };
        let phase = if source != continuation.finished().source() {
            DraftPieceBuildStagingPhaseV1::Source
        } else if proposal != continuation.finished().proposal() {
            DraftPieceBuildStagingPhaseV1::Proposal
        } else {
            DraftPieceBuildStagingPhaseV1::Structure
        };
        let target_continuation = DraftPieceDurableBuildContinuationV1::new(
            continuation.finished(),
            source,
            proposal,
            phase,
        );
        if !target_continuation.is_locally_exact() {
            return Err(DraftMutationStagingErrorV1::Invariant);
        }
        let prepared_edit = super::mutation::prepared_edit_from_staging_build(&build, &session)
            .map_err(|_| DraftMutationStagingErrorV1::Invariant)?;
        let (target_build, target_receipt, target_session, fragments) =
            super::mutation::staged_page_transition(
                &prepared_edit,
                &build,
                &session,
                &replacements,
                target_continuation,
            )
            .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
        Ok(Some(PreparedDraftPieceStagingWindowV1 {
            staging_head: head,
            staging_pages: pages.into_boxed_slice(),
            expected_build: build,
            expected_session: session,
            target_build,
            target_receipt,
            target_session,
            fragments,
            inserted_utf8_bytes,
            acquisition_read_count: acquisition.reads(),
            acquisition_encoded_value_bytes: acquisition.encoded_value_bytes(),
        }))
    }

    pub fn stage_next_durable_draft_piece_window(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftPieceStagingWindowV1,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StageDurableWindowMutation {
                writer_progress_allowed: admitted_writer_is_current_generation(
                    self,
                    &prepared.staging_head,
                ),
                prepared,
            },
        )
    }

    pub fn draft_mutation_staging_command(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingCommandV1,
    ) -> MutationContribution {
        let writer_progress_allowed =
            matches!(
                prepared.target_head.lifecycle(),
                DraftMutationStagingLifecycleV1::Cancelled
                    | DraftMutationStagingLifecycleV1::Rejected
                    | DraftMutationStagingLifecycleV1::Conflict
                    | DraftMutationStagingLifecycleV1::Error
            ) || admitted_writer_is_current_generation(self, &prepared.target_head);
        self.handle.contribution(
            expected_domain_revision,
            StagingMutation {
                prepared,
                writer_progress_allowed,
            },
        )
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

fn replacement_inserted_utf8_bytes(
    replacement: &DraftPieceReplacementV1,
) -> Result<usize, DraftMutationStagingErrorV1> {
    replacement
        .inserted()
        .iter()
        .try_fold(0usize, |total, piece| {
            let bytes = match piece {
                DraftPieceV1::Text(text) => text.len(),
                DraftPieceV1::Marker(_) => 0,
            };
            total
                .checked_add(bytes)
                .ok_or(DraftMutationStagingErrorV1::Overflow)
        })
}
