#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CursorReadLimits, DomainCallbackSource, DomainRegistrationError, DomainValidationError,
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeRecoveryError, HomeSchemaVersion, HomeStore,
    ReadError,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AcceptedInputRevision, BindingRevision, DiscussionContextOwnerId, InputGateRevision,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, PhysicalCorruption, PhysicalFamily,
    RepresentativePhysicalCorruption, current_binding_read_metrics, inject_physical_corruption,
    inject_representative_physical_corruption, reset_current_binding_read_metrics,
};
use syndic_storage::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextTurnIndexRecord, AcceptedOrderIndexRecord, BindingLifecycle, HistorySummaryRecord,
    InputGateRecord, InputGateState, ItemProjectionGeneration, NextTurnReason, SourceEventSequence,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage, TranscriptGeneration,
};

use support::populated::*;
use support::*;

#[test]
fn version_key_and_codec_corruption_fail_registration_verification_and_recovery() {
    assert_eq!(PhysicalFamily::ALL.len(), 44);
    for family in PhysicalFamily::ALL {
        for corruption in [
            PhysicalCorruption::UnsupportedRecordVersion,
            PhysicalCorruption::MalformedStoredKey,
            PhysicalCorruption::MalformedCodecPayload,
        ] {
            let home = TestHome::new(&format!("physical-{}-{corruption:?}", family.name()));
            let mut store = open(home.path());
            let storage = SyndicStorage::register(&mut store).unwrap();
            inject_physical_corruption(&store, storage, family, corruption).unwrap();
            assert!(matches!(
                store.validate_registered_domains(),
                Err(DomainValidationError::Access {
                    domain: "syndic",
                    ..
                })
            ));
            assert!(matches!(
                store.recover_same_home(),
                Err(HomeRecoveryError::DomainValidation(
                    DomainValidationError::Access {
                        domain: "syndic",
                        ..
                    }
                ))
            ));
            store.close().unwrap();

            let mut reopened = open(home.path());
            assert!(matches!(
                SyndicStorage::register(&mut reopened),
                Err(DomainRegistrationError::ValidationAccess {
                    domain: "syndic",
                    source: DomainCallbackSource::Read(_),
                })
            ));
            reopened.close().unwrap();
        }
    }
}

#[test]
fn strict_decoders_reject_unknown_tags_trailing_bytes_and_noncanonical_options() {
    for corruption in [
        RepresentativePhysicalCorruption::UnknownTag,
        RepresentativePhysicalCorruption::TrailingBytes,
        RepresentativePhysicalCorruption::NoncanonicalOption,
    ] {
        let home = TestHome::new(&format!("strict-{corruption:?}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        inject_representative_physical_corruption(&store, storage, corruption).unwrap();
        assert!(matches!(
            store.validate_registered_domains(),
            Err(DomainValidationError::Access {
                domain: "syndic",
                ..
            })
        ));
        assert!(matches!(
            store.recover_same_home(),
            Err(HomeRecoveryError::DomainValidation(
                DomainValidationError::Access {
                    domain: "syndic",
                    ..
                }
            ))
        ));
        store.close().unwrap();

        let mut reopened = open(home.path());
        assert!(matches!(
            SyndicStorage::register(&mut reopened),
            Err(DomainRegistrationError::ValidationAccess {
                domain: "syndic",
                source: DomainCallbackSource::Read(_),
            })
        ));
        reopened.close().unwrap();
    }
}

#[test]
fn primary_and_ordered_reads_enforce_caller_item_and_byte_bounds() {
    let home = TestHome::new("bounded-reads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records = empty_thread_records(id(1), draft_id(2));
    for number in 1..=2_u64 {
        let input_id = beryl_model::SyndicAcceptedInputId::from_bytes(
            [u8::try_from(20 + number).unwrap(); 16],
        );
        let ordinal = AcceptedInputOrdinal::new(number).unwrap();
        let revision = AcceptedInputRevision::new(1).unwrap();
        records.push(FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
            input_id,
            id(1),
            revision,
            ordinal,
            InputGateRevision::new(number + 1).unwrap(),
            AcceptedInputDisposition::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
            empty_composer_content(),
            0,
            timestamp(number),
        )));
        records.push(FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            id(1),
            ordinal,
            input_id,
            revision,
        )));
        records.push(FixtureRecord::AcceptedNextTurn(
            AcceptedNextTurnIndexRecord::new(id(1), ordinal, input_id, revision),
        ));
    }
    records.retain(|record| !matches!(record, FixtureRecord::InputGate(_)));
    records.push(FixtureRecord::InputGate(
        InputGateRecord::new(
            id(1),
            InputGateRevision::new(3).unwrap(),
            InputGateState::Idle,
            2,
            0,
            2,
            0,
        )
        .unwrap(),
    ));
    commit(&store, storage, batch(records));

    assert!(matches!(
        storage.thread(&store, id(1), SyndicPointReadLimit::new(1).unwrap()),
        Err(SyndicReadError::Read(ReadError::BoundExceeded { .. }))
    ));
    let thread = storage
        .thread(&store, id(1), SyndicPointReadLimit::new(1_024).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(thread.record().id(), id(1));
    assert!(thread.stored_bytes() > 1);
    let thread_bytes = thread.stored_bytes();
    assert_eq!(
        storage
            .thread(
                &store,
                id(1),
                SyndicPointReadLimit::new(thread_bytes).unwrap(),
            )
            .unwrap()
            .unwrap()
            .stored_bytes(),
        thread_bytes
    );
    assert!(matches!(
        storage.thread(
            &store,
            id(1),
            SyndicPointReadLimit::new(thread_bytes - 1).unwrap(),
        ),
        Err(SyndicReadError::Read(ReadError::BoundExceeded { .. }))
    ));

    let page = storage
        .accepted_order(
            &store,
            id(1),
            None,
            CursorReadLimits::new(1, 4_096).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(page.has_more());
    let page_bytes = page.stored_bytes();
    let exact_page = storage
        .accepted_order(
            &store,
            id(1),
            None,
            CursorReadLimits::new(1, page_bytes).unwrap(),
        )
        .unwrap();
    assert_eq!(exact_page.records(), page.records());
    assert_eq!(exact_page.stored_bytes(), page_bytes);
    assert!(exact_page.has_more());
    assert!(matches!(
        storage.accepted_order(
            &store,
            id(1),
            None,
            CursorReadLimits::new(1, page_bytes - 1).unwrap(),
        ),
        Err(SyndicReadError::Read(ReadError::BoundExceeded { .. }))
    ));
    let next = storage
        .accepted_order(
            &store,
            id(1),
            Some(page.records()[0].ordinal()),
            CursorReadLimits::new(2, 4_096).unwrap(),
        )
        .unwrap();
    assert_eq!(next.records().len(), 1);
    assert!(!next.has_more());
    store.close().unwrap();
}

#[test]
fn populated_point_and_current_binding_reads_expose_exact_public_records() {
    let home = TestHome::new("populated-point-reads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let limit = SyndicPointReadLimit::new(65_536).unwrap();

    assert_eq!(
        storage
            .thread(&store, id(30), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        id(30)
    );
    assert_eq!(
        storage
            .draft(&store, draft_id(31), limit)
            .unwrap()
            .unwrap()
            .record()
            .thread_id(),
        id(30)
    );
    let owner = DiscussionContextOwnerId::Draft(draft_id(37));
    assert_eq!(
        storage
            .context_envelope(&store, owner, limit)
            .unwrap()
            .unwrap()
            .record()
            .owner(),
        owner
    );
    assert_eq!(
        storage
            .turn(&store, source_turn(), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        source_turn()
    );
    assert_eq!(
        storage
            .turn_state(&store, source_turn(), limit)
            .unwrap()
            .unwrap()
            .record()
            .turn_id(),
        source_turn()
    );
    assert_eq!(
        storage
            .accepted_input(&store, next_input(), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        next_input()
    );
    assert_eq!(
        storage
            .canonical_item(&store, source_item(), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        source_item()
    );
    assert_eq!(
        storage
            .transcript_view_head(&store, id(30), limit)
            .unwrap()
            .unwrap()
            .record()
            .entry_count(),
        1
    );
    assert_eq!(
        storage
            .projection(&store, source_projection(), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        source_projection()
    );
    assert_eq!(
        storage
            .resource(&store, source_resource(), limit)
            .unwrap()
            .unwrap()
            .record()
            .id(),
        source_resource()
    );
    assert!(
        storage
            .history_summary(&store, id(30), limit)
            .unwrap()
            .unwrap()
            .record()
            .complete()
    );
    let binding_revision = BindingRevision::new(3).unwrap();
    let binding = storage
        .binding(&store, id(40), binding_revision, limit)
        .unwrap()
        .unwrap();
    assert_eq!(binding.record().revision(), binding_revision);
    assert_eq!(
        storage
            .execution_snapshot(&store, active_snapshot(), limit)
            .unwrap()
            .unwrap()
            .record()
            .active_turn_id(),
        active_turn()
    );
    assert_eq!(
        storage
            .source_event(&store, active_turn(), SourceEventSequence::FIRST, limit)
            .unwrap()
            .unwrap()
            .record()
            .sequence(),
        SourceEventSequence::FIRST
    );

    reset_current_binding_read_metrics();
    let current = storage
        .current_binding(&store, id(40), limit)
        .unwrap()
        .unwrap();
    assert_eq!(current.head().revision(), binding_revision);
    assert_eq!(current.head().lifecycle(), BindingLifecycle::Active);
    assert_eq!(current.binding(), binding.record());
    assert!(current.stored_bytes() > binding.stored_bytes());
    let components = current_binding_read_metrics();
    assert_eq!(components.binding_bytes(), binding.stored_bytes());
    assert_eq!(
        components.first_head_bytes(),
        components.second_head_bytes()
    );
    assert_eq!(
        current.stored_bytes(),
        components
            .first_head_bytes()
            .checked_add(components.binding_bytes())
            .and_then(|bytes| bytes.checked_add(components.second_head_bytes()))
            .unwrap()
    );
    store.close().unwrap();
}

#[test]
fn populated_ordered_pages_preserve_cursor_continuation_and_index_getters() {
    let home = TestHome::new("populated-ordered-reads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let one = CursorReadLimits::new(1, 65_536).unwrap();

    let accepted = storage.accepted_order(&store, id(40), None, one).unwrap();
    assert_eq!(accepted.records()[0].input_id(), steering_input());
    assert!(accepted.has_more());
    assert!(accepted.stored_bytes() > 0);
    let accepted_tail = storage
        .accepted_order(&store, id(40), Some(accepted.records()[0].ordinal()), one)
        .unwrap();
    assert_eq!(accepted_tail.records()[0].input_id(), next_input());
    assert!(!accepted_tail.has_more());

    let next = storage
        .accepted_next_turn(&store, id(40), None, one)
        .unwrap();
    assert_eq!(next.records()[0].input_id(), next_input());
    let events = storage
        .source_events(&store, active_turn(), None, one)
        .unwrap();
    assert_eq!(events.records()[0].sequence(), SourceEventSequence::FIRST);
    let items = storage
        .turn_items(&store, source_turn(), None, one)
        .unwrap();
    assert_eq!(items.records()[0].item_id(), source_item());
    let transcript = storage
        .transcript_entries(&store, id(30), TranscriptGeneration::FIRST, None, one)
        .unwrap();
    assert_eq!(transcript.records()[0].projection_id(), source_projection());
    let projections = storage
        .item_projections(
            &store,
            source_item(),
            ItemProjectionGeneration::FIRST,
            None,
            one,
        )
        .unwrap();
    assert_eq!(
        projections.records()[0].projection_id(),
        source_projection()
    );
    let resources = storage
        .projection_resources(&store, source_resource_projection(), None, one)
        .unwrap();
    assert_eq!(resources.records()[0].resource_id(), source_resource());

    let binding_one = BindingRevision::new(1).unwrap();
    let history = storage.binding_history(&store, id(40), None, one).unwrap();
    assert_eq!(history.records()[0].revision(), binding_one);
    assert!(history.has_more());
    let history_tail = storage
        .binding_history(&store, id(40), Some(binding_one), one)
        .unwrap();
    assert_eq!(history_tail.records()[0].revision().get(), 2);
    assert!(history_tail.has_more());
    let history_end = storage
        .binding_history(&store, id(40), Some(BindingRevision::new(2).unwrap()), one)
        .unwrap();
    assert_eq!(history_end.records()[0].revision().get(), 3);
    assert!(!history_end.has_more());

    let children = storage
        .turn_children(&store, SyndicTurnId::from_bytes([29; 16]), None, one)
        .unwrap();
    assert_eq!(children.records()[0].child_id(), source_turn());
    store.close().unwrap();
}

#[test]
fn successful_recovery_requires_old_handle_reacquisition() {
    let home = TestHome::new("reacquire");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let old = SyndicStorage::register(&mut store).unwrap();
    commit(&store, old, batch(empty_thread_records(id(1), draft_id(2))));

    let replacement = HistorySummaryRecord::new(
        id(1),
        ThreadRevision::new(1).unwrap(),
        None,
        syndic_storage::empty_selected_path_digest(),
        true,
        timestamp(1),
    );
    let mut fixture = FixtureBatch::new();
    fixture
        .put(FixtureRecord::HistorySummary(replacement))
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(old.fixture_contribution(old.revision(&store).unwrap(), fixture))
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(store.execute(command).is_err());
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    store.recover_same_home().unwrap();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    assert!(matches!(
        old.thread(&store, id(1), SyndicPointReadLimit::new(1_024).unwrap()),
        Err(SyndicReadError::Read(ReadError::ForeignDomain {
            domain: "syndic"
        }))
    ));
    let current = SyndicStorage::reacquire(&store).unwrap();
    assert!(
        current
            .thread(&store, id(1), SyndicPointReadLimit::new(1_024).unwrap())
            .unwrap()
            .is_some()
    );
    store.close().unwrap();
}
