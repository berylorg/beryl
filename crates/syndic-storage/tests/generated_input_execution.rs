#![cfg(feature = "test-faults")]

mod support;
use beryl_home_store::WholeHomeScrubTrigger;
use syndic_storage::*;

#[test]
fn generated_input_dispatches_correlates_and_settles_without_losing_provenance() {
    let home = support::TestHome::new("generated-execution");
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let (input, receipt, _) =
        support::generated_input::seed(&store, storage.clone(), "result [image:A] stays literal");
    let thread = input.thread_id();
    let turn = receipt.parent_turn_id;
    let item_id = receipt.canonical_item_id;
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let pending = storage
        .pending_dispatch_evidence(&store, thread, limit)
        .unwrap()
        .unwrap();
    assert_eq!(pending.turn_kind(), TurnKind::BerylDiscussionHandoff);
    assert_eq!(pending.item_id(), item_id);
    assert_eq!(pending.input(), input.content());
    assert_eq!(pending.asset_reference_set(), None);
    let corrupt = AcceptedInputRecord::new(
        input.id(),
        thread,
        input.ordinal(),
        AcceptedInputSource::DiscussionHandoff(DiscussionHandoffReceipt {
            canonical_item_id: beryl_model::SyndicItemId::from_bytes([239; 16]),
            ..receipt
        }),
        input.content(),
        None,
        input.admitted_at(),
    )
    .unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([test_faults::FixtureRecord::AcceptedInput(corrupt)]),
    );
    assert!(
        storage
            .pending_dispatch_evidence(&store, thread, limit)
            .is_err()
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([test_faults::FixtureRecord::AcceptedInput(input.clone())]),
    );
    storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                thread,
                pending.selected_path(),
                support::exact_cas::execution_binding(),
                support::test_tool_profile(),
            ),
            limit,
        )
        .unwrap();
    let source = support::exact_cas::establish_turn(
        &store,
        storage.clone(),
        thread,
        turn,
        support::timestamp(101),
    );
    support::exact_cas::admit_event(
        &store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        support::timestamp(102),
    );
    support::exact_cas::correlate_user_item(
        &store,
        storage.clone(),
        thread,
        turn,
        item_id,
        &source,
        support::timestamp(103),
    );
    let correlated = storage
        .canonical_item(&store, item_id, limit)
        .unwrap()
        .unwrap();
    assert_eq!(correlated.kind(), CanonicalItemKind::DiscussionHandoff);
    assert_eq!(
        correlated.presentation(),
        &CanonicalItemPresentation::DiscussionHandoff {
            content: input.content(),
            accepted_input_id: input.id()
        }
    );
    assert_eq!(
        correlated.provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    assert_eq!(
        storage
            .turn_state(&store, turn, limit)
            .unwrap()
            .unwrap()
            .item_count(),
        1
    );
    let StopAdmissionRead::Admissible(stop) =
        storage.stop_admission_read(&store, thread, limit).unwrap()
    else {
        panic!("generated turn should admit ordinary stop");
    };
    assert_eq!(stop.target().turn_kind(), TurnKind::BerylDiscussionHandoff);
    let admission = stop.admission(
        StopOperationNonce::from_bytes([238; 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    let operation = admission.operation_id();
    support::discussion_input::committed(
        &store,
        storage.admit_stop_operation(storage.revision(&store).unwrap(), admission),
    );
    support::exact_cas::admit_event(
        &store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        support::timestamp(104),
    );
    support::converge_and_release_terminal_history(&store, storage.clone(), thread, turn);
    assert!(matches!(
        storage
            .stop_operation(&store, operation, limit)
            .unwrap()
            .unwrap()
            .state(),
        StopOperationState::MatchingTerminal(_)
    ));
    assert_eq!(
        storage
            .input_gate(&store, thread, limit)
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    assert_eq!(
        storage.accepted_input(&store, input.id(), limit).unwrap(),
        Some(input.clone())
    );
    let selected = storage
        .thread(&store, thread, limit)
        .unwrap()
        .unwrap()
        .selected_path();
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(2_000_000)),
        )
        .unwrap()
    else {
        panic!("generated history must be replayable");
    };
    let mut cursor = storage.open_recovery_cursor(&store, projection).unwrap();
    let pool = beryl_stream::PagePool::new(
        std::num::NonZeroUsize::new(256).unwrap(),
        std::num::NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    let mut lease = pool.try_lease().unwrap();
    let mut items = Vec::<(RecoveryItemSequenceRole, String)>::new();
    while let Some(page) = storage
        .read_recovery_cursor_page(&store, &mut cursor, lease, 256)
        .unwrap()
    {
        if page.item_offset() == 0 {
            items.push((page.role(), String::new()));
        }
        items.last_mut().unwrap().1.push_str(page.text());
        lease = page.into_page_lease();
    }
    assert_eq!(
        items
            .iter()
            .filter(|(_, text)| text == "Discussion resolution:\n\nresult [image:A] stays literal")
            .count(),
        1
    );
    assert_eq!(
        items.last().unwrap().0,
        RecoveryItemSequenceRole::UserInputText
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
