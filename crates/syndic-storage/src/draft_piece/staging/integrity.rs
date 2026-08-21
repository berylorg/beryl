use super::*;

pub(super) fn authenticate_staging_head_reader(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMutationStagingHeadV1,
) -> Result<DraftMutationStagingProgressReceiptV1, SyndicMutationError> {
    if !draft_mutation_staging_head_is_locally_exact(head) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let selected_key = DraftMutationStagingProgressReceiptKeyV1::new(
        head.identity(),
        head.receipt().transition_ordinal(),
    )
    .ok_or(SyndicMutationError::IdentityCollision)?;
    let selected = point::<DraftMutationStagingProgressFamily>(reader, &selected_key)?
        .ok_or(SyndicMutationError::IdentityCollision)?;
    if !draft_mutation_staging_receipt_is_locally_exact(&selected)
        || selected.digest() != head.receipt().digest()
        || selected.after_head_digest() != head.digest()
        || selected.after_source() != head.source()
        || selected.after_proposal() != head.proposal()
        || selected.after_lifecycle() != head.lifecycle()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_receipt_page_reader(reader, head, &selected)?;
    if !receipt_finish_digest_is_exact(&selected) {
        return Err(SyndicMutationError::IdentityCollision);
    }
    match (selected.key().transition_ordinal(), selected.prior()) {
        (1, None) if selected.before_head_digest().is_none() => {}
        (ordinal, Some(prior))
            if ordinal > 1
                && prior.identity() == head.identity()
                && prior.transition_ordinal().checked_add(1) == Some(ordinal) =>
        {
            let prior_key = DraftMutationStagingProgressReceiptKeyV1::new(
                head.identity(),
                prior.transition_ordinal(),
            )
            .ok_or(SyndicMutationError::IdentityCollision)?;
            let prior_record = point::<DraftMutationStagingProgressFamily>(reader, &prior_key)?
                .ok_or(SyndicMutationError::IdentityCollision)?;
            if !draft_mutation_staging_receipt_is_locally_exact(&prior_record)
                || prior_record.digest() != prior.digest()
                || selected.before_head_digest() != Some(prior_record.after_head_digest())
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
            authenticate_receipt_page_reader(reader, head, &prior_record)?;
            if !receipt_finish_digest_is_exact(&prior_record) {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        _ => return Err(SyndicMutationError::IdentityCollision),
    }
    Ok(selected)
}

pub(super) fn receipt_finish_digest_is_exact(
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> bool {
    match receipt.command() {
        DraftMutationStagingCommandKindV1::Finish => {
            let DraftMutationStagingLifecycleV1::Finished(finish) = receipt.after_lifecycle()
            else {
                return false;
            };
            receipt.finish_digest() == Some(finish_digest(finish))
        }
        DraftMutationStagingCommandKindV1::Transfer => {
            let Some(DraftMutationStagingLifecycleV1::Finished(finish)) =
                receipt.before_lifecycle()
            else {
                return false;
            };
            receipt.finish_digest() == Some(finish_digest(finish))
        }
        _ => true,
    }
}

fn authenticate_receipt_page_reader(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMutationStagingHeadV1,
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> Result<(), SyndicMutationError> {
    let Some((page_key, page_digest)) = receipt.page() else {
        return Ok(());
    };
    let page = point::<DraftMutationStagingPagesFamily>(reader, &page_key)?
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let (before, after) = match page_key.lane() {
        DraftMutationStagingLaneV1::Source => (receipt.before_source(), receipt.after_source()),
        DraftMutationStagingLaneV1::Proposal => {
            (receipt.before_proposal(), receipt.after_proposal())
        }
    };
    let within_head = match page_key.lane() {
        DraftMutationStagingLaneV1::Source => page_key.ordinal() < head.source().next_ordinal(),
        DraftMutationStagingLaneV1::Proposal => page_key.ordinal() < head.proposal().next_ordinal(),
    };
    if !within_head
        || !draft_mutation_staging_page_is_locally_exact(&page)
        || page.digest() != page_digest
        || page.transition_ordinal() != receipt.key().transition_ordinal()
        || before.next_cursor() != page.input_cursor()
        || before.next_ordinal() != page.key().ordinal()
        || before.cumulative_identity() != page.prior_cumulative_identity()
        || after.next_cursor() != page.successor_cursor()
        || after.next_ordinal() != page.key().ordinal().checked_add(1).unwrap_or(0)
        || after.item_total() != page.cumulative_item_total()
        || after.canonical_byte_total() != page.cumulative_byte_total()
        || after.cumulative_identity() != page.successor_cumulative_identity()
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(super) fn make_receipt(
    key: DraftMutationStagingProgressReceiptKeyV1,
    prior: Option<DraftMutationStagingProgressReceiptReferenceV1>,
    command: DraftMutationStagingCommandKindV1,
    page: Option<(DraftMutationStagingPageKeyV1, DraftPieceDigestV1)>,
    finish: Option<DraftPieceDigestV1>,
    before_source: DraftMutationStagingLaneFrontierV1,
    after_source: DraftMutationStagingLaneFrontierV1,
    before_proposal: DraftMutationStagingLaneFrontierV1,
    after_proposal: DraftMutationStagingLaneFrontierV1,
    before_head_digest: Option<DraftPieceDigestV1>,
    after_head_digest: DraftPieceDigestV1,
    before_lifecycle: Option<DraftMutationStagingLifecycleV1>,
    after_lifecycle: DraftMutationStagingLifecycleV1,
    custody_before: DraftMutationStagingCustodyTagV1,
    custody_after: DraftMutationStagingCustodyTagV1,
    build_endpoint: Option<DraftPieceBuildProgressReceiptReferenceV1>,
    terminal_evidence: Option<DraftMutationStagingTerminalEvidenceV1>,
) -> Result<DraftMutationStagingProgressReceiptV1, DraftMutationStagingErrorV1> {
    let mut receipt = DraftMutationStagingProgressReceiptV1::from_parts(
        key,
        prior,
        command,
        page,
        finish,
        before_source,
        after_source,
        before_proposal,
        after_proposal,
        before_head_digest,
        after_head_digest,
        before_lifecycle,
        after_lifecycle,
        custody_before,
        custody_after,
        build_endpoint,
        terminal_evidence,
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    let digest = receipt_digest(receipt.clone())?;
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
        digest,
    );
    Ok(receipt)
}

fn authenticate_staging_page_receipt_values(
    head: &DraftMutationStagingHeadV1,
    page: &DraftMutationStagingPageV1,
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> Result<(), DraftMutationStagingErrorV1> {
    if page.key().identity() != head.identity()
        || !draft_mutation_staging_page_is_locally_exact(page)
        || page.key().ordinal()
            >= match page.key().lane() {
                DraftMutationStagingLaneV1::Source => head.source().next_ordinal(),
                DraftMutationStagingLaneV1::Proposal => head.proposal().next_ordinal(),
            }
    {
        return Err(DraftMutationStagingErrorV1::Invariant);
    }
    let command = match page.key().lane() {
        DraftMutationStagingLaneV1::Source => DraftMutationStagingCommandKindV1::SourcePage,
        DraftMutationStagingLaneV1::Proposal => DraftMutationStagingCommandKindV1::ProposalPage,
    };
    if receipt.key()
        != DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), page.transition_ordinal())
            .ok_or(DraftMutationStagingErrorV1::Invariant)?
        || receipt.digest() != receipt_digest(receipt.clone())?
        || receipt.command() != command
        || receipt.page() != Some((page.key(), page.digest()))
        || receipt.after_lifecycle() != DraftMutationStagingLifecycleV1::Receiving
        || receipt.custody_before() != DraftMutationStagingCustodyTagV1::Staging
        || receipt.custody_after() != DraftMutationStagingCustodyTagV1::Staging
    {
        return Err(DraftMutationStagingErrorV1::Invariant);
    }
    let (before, after) = match page.key().lane() {
        DraftMutationStagingLaneV1::Source => (receipt.before_source(), receipt.after_source()),
        DraftMutationStagingLaneV1::Proposal => {
            (receipt.before_proposal(), receipt.after_proposal())
        }
    };
    if before.next_cursor() != page.input_cursor()
        || before.next_ordinal() != page.key().ordinal()
        || before.cumulative_identity() != page.prior_cumulative_identity()
        || after.next_cursor() != page.successor_cursor()
        || after.next_ordinal()
            != page
                .key()
                .ordinal()
                .checked_add(1)
                .ok_or(DraftMutationStagingErrorV1::Invariant)?
        || after.item_total() != page.cumulative_item_total()
        || after.canonical_byte_total() != page.cumulative_byte_total()
        || after.cumulative_identity() != page.successor_cumulative_identity()
    {
        return Err(DraftMutationStagingErrorV1::Invariant);
    }
    let prior = receipt
        .prior()
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    if prior.transition_ordinal().checked_add(1) != Some(page.transition_ordinal()) {
        return Err(DraftMutationStagingErrorV1::Invariant);
    }
    Ok(())
}

pub(super) fn authenticate_staging_page_closure(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    head: &DraftMutationStagingHeadV1,
    page: &DraftMutationStagingPageV1,
) -> Result<(), DraftMutationStagingErrorV1> {
    let limit = crate::SyndicPointReadLimit::new(65_536).expect("staging point limit is nonzero");
    let key =
        DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), page.transition_ordinal())
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    let receipt = storage
        .point::<DraftMutationStagingProgressFamily>(store, key, limit)?
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    authenticate_staging_page_receipt_values(head, page, &receipt)?;
    let prior = receipt
        .prior()
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    let prior_key =
        DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), prior.transition_ordinal())
            .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    let prior_receipt = storage
        .point::<DraftMutationStagingProgressFamily>(store, prior_key, limit)?
        .ok_or(DraftMutationStagingErrorV1::Invariant)?;
    if prior_receipt.digest() != prior.digest()
        || receipt.before_head_digest() != Some(prior_receipt.after_head_digest())
    {
        return Err(DraftMutationStagingErrorV1::Invariant);
    }
    Ok(())
}

pub(super) fn authenticate_staging_page_receipt_for_window(
    head: &DraftMutationStagingHeadV1,
    page: &DraftMutationStagingPageV1,
    receipt: &DraftMutationStagingProgressReceiptV1,
) -> Result<(), DraftMutationStagingErrorV1> {
    authenticate_staging_page_receipt_values(head, page, receipt)
}

pub(super) fn authenticate_staging_page_reader(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftMutationStagingHeadV1,
    page: &DraftMutationStagingPageV1,
) -> Result<(), SyndicMutationError> {
    let key =
        DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), page.transition_ordinal())
            .ok_or(SyndicMutationError::IdentityCollision)?;
    let receipt = point::<DraftMutationStagingProgressFamily>(reader, &key)?
        .ok_or(SyndicMutationError::IdentityCollision)?;
    authenticate_staging_page_receipt_values(head, page, &receipt)
        .map_err(|_| SyndicMutationError::IdentityCollision)?;
    let prior = receipt
        .prior()
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let prior_key =
        DraftMutationStagingProgressReceiptKeyV1::new(head.identity(), prior.transition_ordinal())
            .ok_or(SyndicMutationError::IdentityCollision)?;
    let prior_receipt = point::<DraftMutationStagingProgressFamily>(reader, &prior_key)?
        .ok_or(SyndicMutationError::IdentityCollision)?;
    if prior_receipt.digest() != prior.digest()
        || !draft_mutation_staging_receipt_is_locally_exact(&prior_receipt)
        || receipt.before_head_digest() != Some(prior_receipt.after_head_digest())
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}
