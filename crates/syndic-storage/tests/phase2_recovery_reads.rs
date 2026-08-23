#![cfg(feature = "test-faults")]

#[path = "phase2_recovery_reads/ordered.rs"]
mod ordered;
mod support;

use beryl_home_store::{
    CursorReadLimits, DomainCallbackSource, DomainRegistrationError, DomainValidationError,
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore, ReadError,
    WholeHomeScrubTrigger,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AcceptedInputRevision, BindingRevision, DiscussionContextOwnerId, DraftRevision,
    InputGateRevision, SyndicDraftId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureRecord, PhysicalCorruption, PhysicalFamily,
    RepresentativePhysicalCorruption, current_binding_read_metrics, inject_physical_corruption,
    inject_representative_physical_corruption, reset_current_binding_read_metrics,
};
use syndic_storage::{
    AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextSourceRecord, AcceptedOrderIndexRecord, AcceptedRouteEffectiveState,
    AcceptedRouteGeneration, AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord,
    AcceptedRouteLeafState, AcceptedRouteRevision, AcceptedRouteTarget, BindingLifecycle,
    DraftByThreadRecord, HistorySummaryRecord, InputGateRecord, InputGateState, NextTurnReason,
    SelectedPathProof, SourceEventSequence, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    ThreadRecord,
};

use support::populated::*;
use support::*;

const PHYSICAL_CORRUPTIONS: [PhysicalCorruption; 3] = [
    PhysicalCorruption::UnsupportedRecordVersion,
    PhysicalCorruption::MalformedStoredKey,
    PhysicalCorruption::MalformedCodecPayload,
];

fn exercise_physical_corruption_partition(partitions: usize, partition: usize) {
    assert_eq!(PhysicalFamily::ALL.len(), 61);
    assert_eq!(PhysicalFamily::ALL.len() * PHYSICAL_CORRUPTIONS.len(), 183);
    for (family, corruption) in PhysicalFamily::ALL
        .into_iter()
        .flat_map(|family| {
            PHYSICAL_CORRUPTIONS
                .into_iter()
                .map(move |corruption| (family, corruption))
        })
        .skip(partition)
        .step_by(partitions)
    {
        let home = TestHome::new(&format!("physical-{}-{corruption:?}", family.name()));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        inject_physical_corruption(&store, storage, family, corruption).unwrap();
        assert!(matches!(
            store.scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit),
            Err(error) if matches!(
                error.validation_error(),
                DomainValidationError::Access { domain: "syndic", .. }
            )
        ));
        let candidate = store.recover_same_home().unwrap();
        let recovered_storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let recovered = candidate.publish();
        assert!(storage.revision(&recovered).is_err());
        recovered_storage.revision(&recovered).unwrap();
        recovered.close().unwrap();

        let mut reopened = open(home.path());
        assert!(matches!(
            SyndicStorage::register_with_schema_validation(&mut reopened),
            Err(DomainRegistrationError::ValidationAccess {
                domain: "syndic",
                source: DomainCallbackSource::Read(_),
            })
        ));
        reopened.close().unwrap();

        let mut scrubbed = open(home.path());
        SyndicStorage::register(&mut scrubbed).unwrap();
        assert!(matches!(
            scrubbed.scrub_whole_home(WholeHomeScrubTrigger::Explicit),
            Err(error) if matches!(
                error.validation_error(),
                DomainValidationError::Access { domain: "syndic", .. }
            )
        ));
        scrubbed.close().unwrap();
    }
}

macro_rules! physical_corruption_partition_tests {
    ($($name:ident => $partition:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                exercise_physical_corruption_partition(16, $partition);
            }
        )+
    };
}

physical_corruption_partition_tests!(
    malformed_physical_records_partition_00 => 0,
    malformed_physical_records_partition_01 => 1,
    malformed_physical_records_partition_02 => 2,
    malformed_physical_records_partition_03 => 3,
    malformed_physical_records_partition_04 => 4,
    malformed_physical_records_partition_05 => 5,
    malformed_physical_records_partition_06 => 6,
    malformed_physical_records_partition_07 => 7,
    malformed_physical_records_partition_08 => 8,
    malformed_physical_records_partition_09 => 9,
    malformed_physical_records_partition_10 => 10,
    malformed_physical_records_partition_11 => 11,
    malformed_physical_records_partition_12 => 12,
    malformed_physical_records_partition_13 => 13,
    malformed_physical_records_partition_14 => 14,
    malformed_physical_records_partition_15 => 15,
);

#[test]
fn strict_decoders_reject_unknown_tags_trailing_bytes_and_noncanonical_options_on_explicit_paths() {
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
            store.scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit),
            Err(error) if matches!(
                error.validation_error(),
                DomainValidationError::Access { domain: "syndic", .. }
            )
        ));
        let recovered = store.recover_same_home().unwrap().publish();
        SyndicStorage::reacquire(&recovered).unwrap();
        recovered.close().unwrap();

        let mut reopened = open(home.path());
        assert!(matches!(
            SyndicStorage::register_with_schema_validation(&mut reopened),
            Err(DomainRegistrationError::ValidationAccess {
                domain: "syndic",
                source: DomainCallbackSource::Read(_),
            })
        ));
        reopened.close().unwrap();
    }
}

#[test]
fn routine_reopen_leaves_dormant_malformed_records_for_the_encountering_typed_read() {
    let home = TestHome::new("routine-reopen-dormant-threads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    inject_physical_corruption(
        &store,
        storage,
        PhysicalFamily::Threads,
        PhysicalCorruption::MalformedCodecPayload,
    )
    .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(
        storage
            .thread(&reopened, id(1), SyndicPointReadLimit::new(1_024).unwrap())
            .is_err()
    );
    assert_eq!(reopened.health().state(), HomeHealthState::Failed);
    reopened.close().unwrap();

    let mut validating = open(home.path());
    assert!(matches!(
        SyndicStorage::register_with_schema_validation(&mut validating),
        Err(DomainRegistrationError::ValidationAccess {
            domain: "syndic",
            source: DomainCallbackSource::Read(_),
        })
    ));
    validating.close().unwrap();
}

#[test]
fn primary_and_ordered_reads_enforce_caller_item_and_byte_bounds() {
    let home = TestHome::new("bounded-reads");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_canonical_empty_thread(&store, storage, id(1), draft_id(2));
    let mut records = empty_thread_records(id(1), draft_id(2));
    let final_thread_revision = ThreadRevision::new(3).unwrap();
    for record in &mut records {
        match record {
            FixtureRecord::Thread(thread) => {
                *thread = ThreadRecord::new(
                    thread.id(),
                    SelectedPathProof::new(
                        thread.committed_tail(),
                        final_thread_revision,
                        thread.selected_path_digest(),
                    ),
                    thread.current_draft_id(),
                    thread.lineage(),
                    thread.image_label_frontiers(),
                    thread.context_owner_id(),
                );
            }
            FixtureRecord::DraftByThread(index) => {
                *index = DraftByThreadRecord::new(
                    index.thread_id(),
                    index.draft_id(),
                    index.draft_revision(),
                    final_thread_revision,
                );
            }
            FixtureRecord::HistorySummary(summary) => {
                *summary = HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next().unwrap(),
                    final_thread_revision,
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    summary.complete(),
                    summary.last_activity_at(),
                );
            }
            _ => {}
        }
    }
    for number in 1..=2_u64 {
        let input_id = beryl_model::SyndicAcceptedInputId::from_bytes(
            [u8::try_from(20 + number).unwrap(); 16],
        );
        let ordinal = AcceptedInputOrdinal::new(number).unwrap();
        let revision = AcceptedInputRevision::new(1).unwrap();
        let generation = AcceptedRouteGeneration::new(number).unwrap();
        records.push(FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input_id,
                id(1),
                ordinal,
                AcceptedInputAdmissionProof::new(
                    ThreadRevision::new(number).unwrap(),
                    SyndicDraftId::from_bytes(*input_id.as_bytes()),
                    DraftRevision::new(1).unwrap(),
                    InputGateRevision::new(number).unwrap(),
                    if number == 1 {
                        SyndicDraftId::from_bytes([22; 16])
                    } else {
                        draft_id(2)
                    },
                )
                .unwrap(),
                generation,
                empty_composer_content(),
                None,
                timestamp(1),
            )
            .unwrap(),
        ));
        records.push(FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            id(1),
            ordinal,
            input_id,
            generation,
        )));
        records.extend([
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    id(1),
                    generation,
                    AcceptedRouteRevision::FIRST,
                    AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                    Some(ordinal),
                    Some(ordinal),
                    1,
                    0,
                    0,
                    1,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
                input_id,
                id(1),
                generation,
                ordinal,
                revision,
                AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
                AcceptedInputLifecycle::Admitted,
            )),
            FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
                id(1),
                generation,
                AcceptedRouteRevision::FIRST,
                ordinal,
                ordinal,
            )),
        ]);
    }
    records.retain(|record| !matches!(record, FixtureRecord::InputGate(_)));
    records.push(FixtureRecord::InputGate(
        InputGateRecord::new(
            id(1),
            InputGateRevision::new(3).unwrap(),
            InputGateState::Idle,
            2,
            Some(AcceptedRouteGeneration::new(2).unwrap()),
            None,
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
    assert_eq!(thread.id(), id(1));

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
    support::seed_populated(&store, storage);
    let limit = SyndicPointReadLimit::new(65_536).unwrap();

    assert_eq!(
        storage.thread(&store, id(30), limit).unwrap().unwrap().id(),
        id(30)
    );
    assert_eq!(
        storage
            .draft(&store, draft_id(31), limit)
            .unwrap()
            .unwrap()
            .thread_id(),
        id(30)
    );
    let owner = DiscussionContextOwnerId::Draft(draft_id(37));
    assert_eq!(
        storage
            .context_envelope(&store, owner, limit)
            .unwrap()
            .unwrap()
            .owner(),
        owner
    );
    assert_eq!(
        storage
            .turn(&store, source_turn(), limit)
            .unwrap()
            .unwrap()
            .id(),
        source_turn()
    );
    assert_eq!(
        storage
            .turn_state(&store, source_turn(), limit)
            .unwrap()
            .unwrap()
            .turn_id(),
        source_turn()
    );
    assert_eq!(
        storage
            .accepted_input(&store, next_input(), limit)
            .unwrap()
            .unwrap()
            .id(),
        next_input()
    );
    assert_eq!(
        storage
            .canonical_item(&store, source_item(), limit)
            .unwrap()
            .unwrap()
            .id(),
        source_item()
    );
    assert_eq!(
        storage
            .transcript_view_head(&store, id(30), limit)
            .unwrap()
            .unwrap()
            .entry_count(),
        1
    );
    assert_eq!(
        storage
            .projection(&store, source_projection(), limit)
            .unwrap()
            .unwrap()
            .id(),
        source_projection()
    );
    assert!(
        storage
            .resource(&store, source_resource(), limit)
            .unwrap()
            .is_none(),
        "the real plain-text provider projection must not recreate the former synthetic attachment"
    );
    assert!(
        storage
            .history_summary(&store, id(30), limit)
            .unwrap()
            .unwrap()
            .complete()
    );
    let binding_revision = BindingRevision::new(3).unwrap();
    let binding = storage
        .binding(&store, id(40), binding_revision, limit)
        .unwrap()
        .unwrap();
    assert_eq!(binding.revision(), binding_revision);
    assert_eq!(
        storage
            .execution_snapshot(&store, active_snapshot(), limit)
            .unwrap()
            .unwrap()
            .active_turn_id(),
        active_turn()
    );
    assert_eq!(
        storage
            .source_event(&store, active_turn(), SourceEventSequence::FIRST, limit)
            .unwrap()
            .unwrap()
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
    assert_eq!(current.binding(), &binding);
    let components = current_binding_read_metrics();
    assert_eq!(components.first_head_reads(), 1);
    assert_eq!(components.binding_reads(), 1);
    assert_eq!(components.second_head_reads(), 1);
    store.close().unwrap();
}
