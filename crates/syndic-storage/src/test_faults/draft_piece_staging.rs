use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

use crate::{
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, domain::SyndicDomain, draft_piece::*,
};

#[derive(Clone)]
enum StagingCorruption {
    DeleteHead(DraftMutationStagingIdentityV1),
    DeletePage(DraftMutationStagingPageKeyV1),
    DeleteReceipt(DraftMutationStagingProgressReceiptKeyV1),
    PutPage(DraftMutationStagingPageKeyV1, DraftMutationStagingPageV1),
    PutReceipt(
        DraftMutationStagingProgressReceiptKeyV1,
        DraftMutationStagingProgressReceiptV1,
    ),
    PutHead(DraftMutationStagingIdentityV1, DraftMutationStagingHeadV1),
    PutSession(
        DraftEditorCandidateSessionRecordKeyV1,
        DraftEditorCandidateSessionRecordV1,
    ),
    PutBatchPrefix {
        targets: Box<
            [(
                DraftMutationStagingPageV1,
                DraftMutationStagingProgressReceiptV1,
            )],
        >,
        page_count: usize,
        receipt_count: usize,
    },
}

pub fn draft_mutation_staging_batch_target(
    prepared: &PreparedDraftMutationStagingBatchV1,
    index: usize,
) -> Option<(
    DraftMutationStagingPageV1,
    DraftMutationStagingProgressReceiptV1,
)> {
    prepared
        .targets()
        .nth(index)
        .map(|(page, receipt)| (page.clone(), receipt.clone()))
}

pub fn draft_mutation_staging_batch_target_records(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedDraftMutationStagingBatchV1,
    index: usize,
) -> Result<
    (
        Option<DraftMutationStagingPageV1>,
        Option<DraftMutationStagingProgressReceiptV1>,
    ),
    SyndicReadError,
> {
    let (page, receipt) = prepared
        .targets()
        .nth(index)
        .expect("fixture batch target index is in range");
    Ok((
        storage.point::<DraftMutationStagingPagesFamily>(store, page.key(), limit())?,
        storage.point::<DraftMutationStagingProgressFamily>(store, receipt.key(), limit())?,
    ))
}

pub fn draft_mutation_staging_locally_exact_source_head(
    head: &DraftMutationStagingHeadV1,
    next_cursor: u64,
    next_ordinal: u64,
    item_total: u64,
    canonical_byte_total: u64,
) -> DraftMutationStagingHeadV1 {
    let source = DraftMutationStagingLaneFrontierV1::new(
        next_cursor,
        next_ordinal,
        item_total,
        canonical_byte_total,
        head.source().cumulative_identity(),
    )
    .expect("fixture source frontier is nonzero");
    locally_exact_head(
        DraftMutationStagingHeadV1::from_parts(
            head.identity(),
            head.begin(),
            head.begin_digest(),
            source,
            head.proposal(),
            head.receipt(),
            head.lifecycle(),
            DraftPieceDigestV1::from_bytes([0; 32]),
        ),
        head.receipt(),
    )
}

pub fn inject_draft_mutation_staging_batch_prefix(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedDraftMutationStagingBatchV1,
    page_count: usize,
    receipt_count: usize,
) -> MutationContribution {
    assert!(page_count <= prepared.page_count());
    assert!(receipt_count <= prepared.page_count());
    let targets = prepared
        .targets()
        .map(|(page, receipt)| (page.clone(), receipt.clone()))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    contribution(
        store,
        storage,
        StagingCorruption::PutBatchPrefix {
            targets,
            page_count,
            receipt_count,
        },
    )
}

pub fn delete_draft_mutation_staging_head(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> MutationContribution {
    contribution(store, storage, StagingCorruption::DeleteHead(identity))
}

pub fn delete_draft_mutation_staging_page(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMutationStagingPageKeyV1,
) -> MutationContribution {
    contribution(store, storage, StagingCorruption::DeletePage(key))
}

pub fn inject_draft_mutation_staging_occupied_page(
    store: &HomeStore,
    storage: SyndicStorage,
    page: DraftMutationStagingPageV1,
) -> MutationContribution {
    contribution(store, storage, StagingCorruption::PutPage(page.key(), page))
}

pub fn delete_draft_mutation_staging_receipt(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMutationStagingProgressReceiptKeyV1,
) -> MutationContribution {
    contribution(store, storage, StagingCorruption::DeleteReceipt(key))
}

pub fn inject_draft_mutation_staging_page_digest_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMutationStagingPageKeyV1,
) -> MutationContribution {
    let page = storage
        .point::<DraftMutationStagingPagesFamily>(store, key, limit())
        .expect("fixture staging page reads")
        .expect("fixture staging page exists");
    let corrupted = DraftMutationStagingPageV1::from_parts(
        page.key(),
        page.transition_ordinal(),
        page.input_cursor(),
        page.successor_cursor(),
        page.item_ceiling(),
        page.byte_ceiling(),
        page.prior_cumulative_identity(),
        page.successor_cumulative_identity(),
        page.cumulative_item_total(),
        page.cumulative_byte_total(),
        page.items().to_vec().into_boxed_slice(),
        DraftPieceDigestV1::from_bytes([0xE1; 32]),
    );
    contribution(store, storage, StagingCorruption::PutPage(key, corrupted))
}

pub fn inject_draft_mutation_staging_page_ceiling_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMutationStagingPageKeyV1,
) -> MutationContribution {
    let page = storage
        .point::<DraftMutationStagingPagesFamily>(store, key, limit())
        .expect("fixture staging page reads")
        .expect("fixture staging page exists");
    let item_ceiling = if usize::from(page.item_ceiling()) < DRAFT_PIECE_PAGE_MAX_RECORDS {
        page.item_ceiling() + 1
    } else {
        page.item_ceiling() - 1
    };
    assert!(usize::from(item_ceiling) >= page.items().len());
    let corrupted = DraftMutationStagingPageV1::from_parts(
        page.key(),
        page.transition_ordinal(),
        page.input_cursor(),
        page.successor_cursor(),
        item_ceiling,
        page.byte_ceiling(),
        page.prior_cumulative_identity(),
        page.successor_cumulative_identity(),
        page.cumulative_item_total(),
        page.cumulative_byte_total(),
        page.items().to_vec().into_boxed_slice(),
        page.digest(),
    );
    contribution(store, storage, StagingCorruption::PutPage(key, corrupted))
}

pub fn inject_draft_mutation_terminal_same_operation_custody(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> MutationContribution {
    let head = stored_head(store, storage, identity);
    let key =
        DraftEditorCandidateSessionRecordKeyV1::head(identity.draft_id(), identity.session_id());
    let session = match storage
        .point::<DraftEditorCandidateSessionsFamily>(store, key, limit())
        .expect("fixture session reads")
        .expect("fixture session exists")
    {
        DraftEditorCandidateSessionRecordV1::Head(session) => session,
        DraftEditorCandidateSessionRecordV1::OpenReceipt(_) => {
            panic!("fixture session head exists")
        }
    };
    let custody = DraftEditorActiveOperationV1::staging(
        identity.operation_id().as_piece_operation(),
        head.begin_digest(),
        head.begin().predecessor_candidate_generation(),
        head.begin().predecessor_root(),
        head.begin().predecessor_history(),
        head.receipt(),
    );
    let corrupted = session
        .with_active_operation(custody)
        .expect("terminal fixture session accepts same-operation custody");
    contribution(
        store,
        storage,
        StagingCorruption::PutSession(key, DraftEditorCandidateSessionRecordV1::Head(corrupted)),
    )
}

pub fn inject_draft_mutation_staging_receipt_digest_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    key: DraftMutationStagingProgressReceiptKeyV1,
) -> MutationContribution {
    let receipt = storage
        .point::<DraftMutationStagingProgressFamily>(store, key, limit())
        .expect("fixture staging receipt reads")
        .expect("fixture staging receipt exists");
    let corrupted = DraftMutationStagingProgressReceiptV1::from_parts(
        receipt.key(),
        receipt.prior(),
        receipt.command(),
        receipt.page(),
        receipt.finish_digest(),
        receipt.before_source(),
        receipt.after_source(),
        receipt.before_proposal(),
        receipt.after_proposal(),
        receipt.before_head_digest(),
        receipt.after_head_digest(),
        receipt.before_lifecycle(),
        receipt.after_lifecycle(),
        receipt.custody_before(),
        receipt.custody_after(),
        receipt.build_endpoint(),
        receipt.terminal_evidence(),
        DraftPieceDigestV1::from_bytes([0xE2; 32]),
    );
    contribution(
        store,
        storage,
        StagingCorruption::PutReceipt(key, corrupted),
    )
}

pub fn inject_draft_mutation_staging_head_digest_corruption(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> MutationContribution {
    let head = storage
        .point::<DraftMutationStagingHeadsFamily>(store, identity, limit())
        .expect("fixture staging head reads")
        .expect("fixture staging head exists");
    let corrupted = DraftMutationStagingHeadV1::from_parts(
        head.identity(),
        head.begin(),
        head.begin_digest(),
        head.source(),
        head.proposal(),
        head.receipt(),
        head.lifecycle(),
        DraftPieceDigestV1::from_bytes([0xE3; 32]),
    );
    contribution(
        store,
        storage,
        StagingCorruption::PutHead(identity, corrupted),
    )
}

pub fn inject_draft_mutation_staging_head_ahead(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> MutationContribution {
    let head = stored_head(store, storage, identity);
    let receipt = DraftMutationStagingProgressReceiptReferenceV1::new(
        identity,
        head.receipt()
            .transition_ordinal()
            .checked_add(1)
            .expect("fixture transition advances"),
        DraftPieceDigestV1::from_bytes([0xE4; 32]),
    )
    .expect("fixture receipt reference is one-based");
    contribution(
        store,
        storage,
        StagingCorruption::PutHead(identity, locally_exact_head(head, receipt)),
    )
}

pub fn inject_draft_mutation_staging_head_fork(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> MutationContribution {
    let head = stored_head(store, storage, identity);
    let receipt = DraftMutationStagingProgressReceiptReferenceV1::new(
        identity,
        head.receipt().transition_ordinal(),
        DraftPieceDigestV1::from_bytes([0xE5; 32]),
    )
    .expect("fixture receipt reference is one-based");
    contribution(
        store,
        storage,
        StagingCorruption::PutHead(identity, locally_exact_head(head, receipt)),
    )
}

fn contribution(
    store: &HomeStore,
    storage: SyndicStorage,
    corruption: StagingCorruption,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).expect("fixture revision reads"),
        corruption,
    )
}

fn stored_head(
    store: &HomeStore,
    storage: SyndicStorage,
    identity: DraftMutationStagingIdentityV1,
) -> DraftMutationStagingHeadV1 {
    storage
        .point::<DraftMutationStagingHeadsFamily>(store, identity, limit())
        .expect("fixture staging head reads")
        .expect("fixture staging head exists")
}

fn locally_exact_head(
    head: DraftMutationStagingHeadV1,
    receipt: DraftMutationStagingProgressReceiptReferenceV1,
) -> DraftMutationStagingHeadV1 {
    let provisional = DraftMutationStagingHeadV1::from_parts(
        head.identity(),
        head.begin(),
        head.begin_digest(),
        head.source(),
        head.proposal(),
        receipt,
        head.lifecycle(),
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    let digest = crate::draft_piece::head_digest(provisional.clone())
        .expect("fixture head digest is canonical");
    DraftMutationStagingHeadV1::from_parts(
        provisional.identity(),
        provisional.begin(),
        provisional.begin_digest(),
        provisional.source(),
        provisional.proposal(),
        provisional.receipt(),
        provisional.lifecycle(),
        digest,
    )
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(75_000).expect("fixture point bound is nonzero")
}

impl DomainMutation<SyndicDomain> for StagingCorruption {
    type Error = super::FixtureMutationError;

    fn validate(&self, _: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self {
            Self::DeletePage(_) | Self::PutPage(..) => {
                reservation.reserve_records::<DraftMutationStagingPagesCodec>(1)?
            }
            Self::DeleteReceipt(_) | Self::PutReceipt(..) => {
                reservation.reserve_records::<DraftMutationStagingProgressCodec>(1)?
            }
            Self::DeleteHead(_) | Self::PutHead(..) => {
                reservation.reserve_records::<DraftMutationStagingHeadsCodec>(1)?
            }
            Self::PutSession(..) => {
                reservation.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?
            }
            Self::PutBatchPrefix {
                targets: _,
                page_count,
                receipt_count,
            } => {
                if *page_count > 0 {
                    reservation.reserve_records::<DraftMutationStagingPagesCodec>(*page_count)?;
                }
                if *receipt_count > 0 {
                    reservation
                        .reserve_records::<DraftMutationStagingProgressCodec>(*receipt_count)?;
                }
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self {
            Self::DeleteHead(key) => builder.delete::<DraftMutationStagingHeadsCodec>(key)?,
            Self::DeletePage(key) => builder.delete::<DraftMutationStagingPagesCodec>(key)?,
            Self::DeleteReceipt(key) => builder.delete::<DraftMutationStagingProgressCodec>(key)?,
            Self::PutPage(key, value) => {
                builder.put::<DraftMutationStagingPagesCodec>(key, value)?
            }
            Self::PutReceipt(key, value) => {
                builder.put::<DraftMutationStagingProgressCodec>(key, value)?
            }
            Self::PutHead(key, value) => {
                builder.put::<DraftMutationStagingHeadsCodec>(key, value)?
            }
            Self::PutSession(key, value) => {
                builder.put::<DraftEditorCandidateSessionsCodec>(key, value)?
            }
            Self::PutBatchPrefix {
                targets,
                page_count,
                receipt_count,
            } => {
                for (page, _) in targets.iter().take(*page_count) {
                    builder.put::<DraftMutationStagingPagesCodec>(&page.key(), page)?;
                }
                for (_, receipt) in targets.iter().take(*receipt_count) {
                    builder.put::<DraftMutationStagingProgressCodec>(&receipt.key(), receipt)?;
                }
            }
        }
        Ok(())
    }
}
