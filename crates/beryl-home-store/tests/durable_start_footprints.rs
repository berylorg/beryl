use beryl_home_store::{CheckedBatchFootprint, DurableStartFootprint, DurableStartFootprintError};
use beryl_state::{
    accepted_input_to_submitted_item_owner_transfer_max_footprint,
    draft_to_submitted_item_owner_transfer_max_footprint,
};
use syndic_storage::{accepted_input_promotion_max_footprint, idle_submission_max_footprint};

#[test]
fn owner_derived_maxima_compose_to_the_direct_and_queued_envelopes() {
    let direct = DurableStartFootprint::compose(
        idle_submission_max_footprint().expect("first-acceptance footprint"),
        Some(
            draft_to_submitted_item_owner_transfer_max_footprint()
                .expect("draft transfer footprint"),
        ),
    )
    .expect("direct composition");
    assert_eq!(26, direct.logical().records());
    assert_eq!(
        1_263_194,
        direct.logical().encoded_key_value_bytes().expect("total")
    );

    let queued = DurableStartFootprint::compose(
        accepted_input_promotion_max_footprint().expect("promotion footprint"),
        Some(
            accepted_input_to_submitted_item_owner_transfer_max_footprint()
                .expect("promotion transfer footprint"),
        ),
    )
    .expect("queued composition");
    assert_eq!(25, queued.logical().records());
    assert_eq!(
        1_328_212,
        queued.logical().encoded_key_value_bytes().expect("total")
    );
    assert_eq!(1_328_763, queued.journal_append_bytes());
}

#[test]
fn composition_rejects_wrong_participant_kind() {
    let error = DurableStartFootprint::compose(
        idle_submission_max_footprint().expect("first-acceptance footprint"),
        Some(
            accepted_input_to_submitted_item_owner_transfer_max_footprint()
                .expect("promotion transfer footprint"),
        ),
    )
    .expect_err("operation kinds must match");
    assert_eq!(
        DurableStartFootprintError::MismatchedParticipantKinds,
        error
    );
}

#[test]
fn no_image_start_omits_the_asset_participant() {
    let direct = DurableStartFootprint::compose(
        idle_submission_max_footprint().expect("first-acceptance footprint"),
        None,
    )
    .expect("marker-free direct composition");
    assert_eq!(23, direct.logical().records());

    let queued = DurableStartFootprint::compose(
        accepted_input_promotion_max_footprint().expect("promotion footprint"),
        None,
    )
    .expect("marker-free queued composition");
    assert_eq!(22, queued.logical().records());
}

#[test]
fn checked_batch_footprint_never_wraps() {
    let error = CheckedBatchFootprint::new(u64::MAX, 0, 0)
        .checked_add(CheckedBatchFootprint::new(1, 0, 0))
        .expect_err("record counts must not wrap");
    assert_eq!(DurableStartFootprintError::ArithmeticOverflow, error);
}

#[test]
fn queued_framing_stays_derived_from_fjall() {
    let queued = DurableStartFootprint::compose(
        accepted_input_promotion_max_footprint().expect("promotion footprint"),
        Some(
            accepted_input_to_submitted_item_owner_transfer_max_footprint()
                .expect("promotion transfer footprint"),
        ),
    )
    .expect("queued composition");
    let logical = queued.logical();
    let fjall = fjall::JournalAppendFootprint::try_from_batch_totals(
        logical.records(),
        logical.encoded_key_bytes(),
        logical.encoded_value_bytes(),
    )
    .expect("Fjall framing");
    assert_eq!(fjall.max_encoded_bytes(), queued.journal_append_bytes());
}
