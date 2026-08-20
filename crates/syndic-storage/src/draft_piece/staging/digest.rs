use super::*;

fn hash(domain: &[u8], parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_be_bytes());
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

pub(super) fn empty_lane_identity(lane: DraftMutationStagingLaneV1) -> DraftPieceDigestV1 {
    hash(
        b"syndic/draft-mutation-staging-lane/v1/empty",
        &[&[match lane {
            DraftMutationStagingLaneV1::Source => 0,
            DraftMutationStagingLaneV1::Proposal => 1,
        }]],
    )
}

pub(super) fn begin_digest(begin: DraftMutationBeginV1) -> DraftPieceDigestV1 {
    let bytes = canonical_staging_begin_bytes(begin);
    hash(b"syndic/draft-mutation-begin/v1", &[&bytes])
}

pub(super) fn finish_digest(finish: DraftMutationFinishInputV1) -> DraftPieceDigestV1 {
    let bytes = canonical_staging_finish_bytes(finish);
    hash(b"syndic/draft-mutation-finish/v1", &[&bytes])
}

pub(super) fn page_digest(
    key: DraftMutationStagingPageKeyV1,
    transition_ordinal: u64,
    input_cursor: u64,
    successor_cursor: u64,
    item_ceiling: u16,
    byte_ceiling: u32,
    prior: DraftPieceDigestV1,
    item_total: u64,
    byte_total: u64,
    item_bytes: &[u8],
) -> DraftPieceDigestV1 {
    let mut key_bytes = Vec::with_capacity(57);
    key_bytes.extend_from_slice(key.identity().draft_id().as_bytes());
    key_bytes.extend_from_slice(key.identity().session_id().as_bytes());
    key_bytes.extend_from_slice(key.identity().operation_id().as_bytes());
    key_bytes.push(match key.lane() {
        DraftMutationStagingLaneV1::Source => 0,
        DraftMutationStagingLaneV1::Proposal => 1,
    });
    key_bytes.extend_from_slice(&key.ordinal().to_be_bytes());
    hash(
        b"syndic/draft-mutation-staging-page/v1",
        &[
            &key_bytes,
            &transition_ordinal.to_be_bytes(),
            &input_cursor.to_be_bytes(),
            &successor_cursor.to_be_bytes(),
            &item_ceiling.to_be_bytes(),
            &byte_ceiling.to_be_bytes(),
            prior.as_bytes(),
            &item_total.to_be_bytes(),
            &byte_total.to_be_bytes(),
            item_bytes,
        ],
    )
}

pub(super) fn cumulative_identity(
    prior: DraftPieceDigestV1,
    page: DraftPieceDigestV1,
) -> DraftPieceDigestV1 {
    hash(
        b"syndic/draft-mutation-staging-lane/v1/link",
        &[prior.as_bytes(), page.as_bytes()],
    )
}

pub(super) fn head_digest(
    head: DraftMutationStagingHeadV1,
) -> Result<DraftPieceDigestV1, DraftMutationStagingErrorV1> {
    let bytes = canonical_staging_head_digest_bytes(&head);
    Ok(hash(b"syndic/draft-mutation-staging-head/v1", &[&bytes]))
}

pub(super) fn receipt_digest(
    mut receipt: DraftMutationStagingProgressReceiptV1,
) -> Result<DraftPieceDigestV1, DraftMutationStagingErrorV1> {
    receipt = DraftMutationStagingProgressReceiptV1::from_parts(
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
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    let bytes = canonical_staging_progress_bytes(&receipt)
        .map_err(|_| DraftMutationStagingErrorV1::Invalid)?;
    Ok(hash(
        b"syndic/draft-mutation-staging-progress-receipt/v1",
        &[&bytes[..bytes.len() - 32]],
    ))
}

pub(super) fn draft_mutation_staging_head_is_locally_exact(
    head: &DraftMutationStagingHeadV1,
) -> bool {
    head.identity() == head.begin().identity()
        && head.begin_digest() == begin_digest(head.begin())
        && head.receipt().identity() == head.identity()
        && head.receipt().transition_ordinal() > 0
        && !matches!(head.lifecycle(), DraftMutationStagingLifecycleV1::Finished(finish)
            if finish.source() != head.source() || finish.proposal() != head.proposal())
        && head_digest(head.clone()).ok() == Some(head.digest())
}

pub(super) fn draft_mutation_staging_page_is_locally_exact(
    page: &DraftMutationStagingPageV1,
) -> bool {
    if page.transition_ordinal() <= 1
        || page.input_cursor() >= page.successor_cursor()
        || page.items().is_empty()
        || page.items().len() > DRAFT_PIECE_PAGE_MAX_RECORDS
        || page.item_ceiling() == 0
        || page.items().len() > usize::from(page.item_ceiling())
        || page.byte_ceiling() == 0
        || usize::try_from(page.byte_ceiling())
            .ok()
            .is_none_or(|limit| limit > DRAFT_PIECE_PAGE_MAX_BYTES)
        || page.items().iter().any(|item| {
            !matches!(
                (page.key().lane(), item),
                (
                    DraftMutationStagingLaneV1::Source,
                    DraftMutationStagingPageItemV1::SourcePosition(_)
                ) | (
                    DraftMutationStagingLaneV1::Proposal,
                    DraftMutationStagingPageItemV1::Proposal(_)
                )
            )
        })
    {
        return false;
    }
    let Ok(item_bytes) = canonical_staging_items_bytes(page.items()) else {
        return false;
    };
    if item_bytes.len() > usize::try_from(page.byte_ceiling()).unwrap_or(0) {
        return false;
    }
    let expected_digest = page_digest(
        page.key(),
        page.transition_ordinal(),
        page.input_cursor(),
        page.successor_cursor(),
        page.item_ceiling(),
        page.byte_ceiling(),
        page.prior_cumulative_identity(),
        page.cumulative_item_total(),
        page.cumulative_byte_total(),
        &item_bytes,
    );
    expected_digest == page.digest()
        && cumulative_identity(page.prior_cumulative_identity(), page.digest())
            == page.successor_cumulative_identity()
}

pub(super) fn draft_mutation_staging_receipt_is_locally_exact(
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> bool {
    let ordinal = receipt.key().transition_ordinal();
    let predecessor_is_exact = match (ordinal, receipt.prior(), receipt.before_head_digest()) {
        (1, None, None) => true,
        (value, Some(prior), Some(_)) if value > 1 => {
            prior.identity() == receipt.key().identity()
                && prior.transition_ordinal().checked_add(1) == Some(value)
        }
        _ => false,
    };
    let frontiers_unchanged = receipt.before_source() == receipt.after_source()
        && receipt.before_proposal() == receipt.after_proposal();
    let shape_is_exact = match receipt.command() {
        DraftMutationStagingCommandKindV1::Begin => {
            ordinal == 1
                && receipt.page().is_none()
                && receipt.finish_digest().is_none()
                && receipt.before_lifecycle().is_none()
                && receipt.after_lifecycle() == DraftMutationStagingLifecycleV1::Receiving
                && receipt.custody_before() == DraftMutationStagingCustodyTagV1::None
                && receipt.custody_after() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.build_endpoint().is_none()
                && receipt.terminal_evidence().is_none()
                && frontiers_unchanged
        }
        DraftMutationStagingCommandKindV1::SourcePage
        | DraftMutationStagingCommandKindV1::ProposalPage => {
            let expected_lane = match receipt.command() {
                DraftMutationStagingCommandKindV1::SourcePage => DraftMutationStagingLaneV1::Source,
                DraftMutationStagingCommandKindV1::ProposalPage => {
                    DraftMutationStagingLaneV1::Proposal
                }
                _ => unreachable!(),
            };
            ordinal > 1
                && receipt.page().is_some_and(|(key, _)| {
                    key.identity() == receipt.key().identity() && key.lane() == expected_lane
                })
                && receipt.finish_digest().is_none()
                && receipt.before_lifecycle() == Some(DraftMutationStagingLifecycleV1::Receiving)
                && receipt.after_lifecycle() == DraftMutationStagingLifecycleV1::Receiving
                && receipt.custody_before() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.custody_after() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.build_endpoint().is_none()
                && receipt.terminal_evidence().is_none()
                && match expected_lane {
                    DraftMutationStagingLaneV1::Source => {
                        receipt.before_source() != receipt.after_source()
                            && receipt.before_proposal() == receipt.after_proposal()
                    }
                    DraftMutationStagingLaneV1::Proposal => {
                        receipt.before_source() == receipt.after_source()
                            && receipt.before_proposal() != receipt.after_proposal()
                    }
                }
        }
        DraftMutationStagingCommandKindV1::Finish => {
            ordinal > 1
                && receipt.page().is_none()
                && receipt.finish_digest().is_some()
                && receipt.before_lifecycle() == Some(DraftMutationStagingLifecycleV1::Receiving)
                && matches!(
                    receipt.after_lifecycle(),
                    DraftMutationStagingLifecycleV1::Finished(_)
                )
                && receipt.custody_before() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.custody_after() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.build_endpoint().is_none()
                && receipt.terminal_evidence().is_none()
                && frontiers_unchanged
        }
        DraftMutationStagingCommandKindV1::Transfer => {
            ordinal > 1
                && receipt.page().is_none()
                && receipt.finish_digest().is_some()
                && matches!(
                    receipt.before_lifecycle(),
                    Some(DraftMutationStagingLifecycleV1::Finished(_))
                )
                && matches!(
                    receipt.after_lifecycle(),
                    DraftMutationStagingLifecycleV1::Building(_)
                )
                && receipt.custody_before() == DraftMutationStagingCustodyTagV1::Staging
                && receipt.custody_after() == DraftMutationStagingCustodyTagV1::Building
                && receipt.build_endpoint().is_some()
                && receipt.terminal_evidence().is_none()
                && frontiers_unchanged
        }
        DraftMutationStagingCommandKindV1::Terminal => {
            let evidence_matches = receipt
                .terminal_evidence()
                .is_some_and(|evidence| terminal_lifecycle(evidence) == receipt.after_lifecycle());
            receipt.page().is_none()
                && receipt.finish_digest().is_none()
                && receipt.build_endpoint().is_none()
                && evidence_matches
                && frontiers_unchanged
                && if ordinal == 1 {
                    receipt.before_lifecycle().is_none()
                        && receipt.custody_before() == DraftMutationStagingCustodyTagV1::None
                        && receipt.custody_after() == DraftMutationStagingCustodyTagV1::None
                } else {
                    matches!(
                        receipt.before_lifecycle(),
                        Some(
                            DraftMutationStagingLifecycleV1::Receiving
                                | DraftMutationStagingLifecycleV1::Finished(_)
                        )
                    ) && receipt.custody_before() == DraftMutationStagingCustodyTagV1::Staging
                        && receipt.custody_after() == DraftMutationStagingCustodyTagV1::None
                }
        }
    };
    predecessor_is_exact
        && shape_is_exact
        && receipt_digest(receipt.clone()).ok() == Some(receipt.digest())
}
