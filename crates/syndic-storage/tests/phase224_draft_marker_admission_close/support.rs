use super::*;

pub(super) fn assert_terminal_receipt_fault_remains_inert(
    name: &str,
    fault: DraftMarkerAdmissionTerminalReceiptFaultV1,
    seed: u8,
) {
    let (home, store, storage, thread) = fixture(name, seed);
    let (session, marker) = marked_session(&storage, &store, thread, seed.wrapping_add(1));
    let admission = owner(&session, seed.wrapping_add(2));
    submit_page(
        &storage,
        &store,
        admission,
        seed.wrapping_add(3),
        false,
        vec![association(
            seed.wrapping_add(4),
            &session,
            marker.marker_id(),
        )],
    );
    let terminal_command = command(seed.wrapping_add(5));
    assert_advanced(storage.cancel_draft_marker_admission(&store, admission, terminal_command));
    compact_terminal_without_retaining(&storage, &store, admission, seed.wrapping_add(6));
    drop(storage);
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let before = snapshot(&storage, &reopened, admission);
    let head_digest = before.head().unwrap().digest();
    let capacity_digest = before.capacity().unwrap().digest();
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        Some(admission),
        "{name}"
    );
    assert!(matches!(
        storage.inject_draft_marker_admission_terminal_receipt_fault_for_test(
            &reopened, admission, fault,
        ),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    assert!(matches!(
        storage.advance_draft_marker_admission_cleanup(
            &reopened,
            admission,
            command(seed.wrapping_add(9)),
        ),
        DraftMarkerAdmissionTerminalOutcomeV1::Collision
    ));
    let after = snapshot(&storage, &reopened, admission);
    assert_eq!(after.head().unwrap().digest(), head_digest, "{name}");
    assert_eq!(
        after.capacity().unwrap().digest(),
        capacity_digest,
        "{name}"
    );
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(&reopened)
            .unwrap(),
        Some(admission),
        "{name}"
    );
}

pub(super) fn assert_cancelled_terminal(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    terminal_command: u8,
) {
    assert_advanced(storage.cancel_draft_marker_admission(
        store,
        admission,
        command(terminal_command),
    ));
    let terminal = terminal_cleanup_snapshot(storage, store, admission);
    let head = terminal.head().unwrap();
    assert!(terminal.receipt().is_some());
    let aggregate = terminal.capacity().unwrap().charge();
    assert!(aggregate.heads() >= head.charge().heads());
    assert!(aggregate.associations() >= head.charge().associations());
    assert!(aggregate.encoded_bytes() >= head.charge().encoded_bytes());
}

pub(super) fn terminal_cleanup_snapshot(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
) -> syndic_storage::DraftMarkerAdmissionPublicationSnapshotV1 {
    let terminal = snapshot(storage, store, admission);
    let head = terminal.head().unwrap();
    assert_eq!(
        head.lifecycle(),
        DraftMarkerAdmissionLifecycleV1::TerminalCleanup
    );
    assert_eq!(head.selected_receipt(), None);
    terminal
}

pub(super) fn assert_inert_cleanup_refuses_without_mutation(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    cleanup_command: u8,
) {
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(store)
            .unwrap(),
        Some(admission)
    );
    let before = snapshot(storage, store, admission);
    let head_digest = before.head().unwrap().digest();
    let capacity_digest = before.capacity().unwrap().digest();
    assert!(matches!(
        storage.advance_draft_marker_admission_cleanup(store, admission, command(cleanup_command)),
        DraftMarkerAdmissionTerminalOutcomeV1::Collision
    ));
    let after = snapshot(storage, store, admission);
    assert_eq!(after.head().unwrap().digest(), head_digest);
    assert_eq!(after.capacity().unwrap().digest(), capacity_digest);
    assert_eq!(
        storage
            .next_inert_draft_marker_admission_cleanup(store)
            .unwrap(),
        Some(admission)
    );
}

pub(super) fn compact_terminal_without_retaining(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    first_cleanup_command: u8,
) {
    for cleanup_command in [
        first_cleanup_command,
        first_cleanup_command.wrapping_add(1),
        first_cleanup_command.wrapping_add(2),
        first_cleanup_command.wrapping_add(3),
    ] {
        match storage.advance_draft_marker_admission_cleanup(
            store,
            admission,
            command(cleanup_command),
        ) {
            DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. } => {}
            DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure => {
                panic!("compact terminal closure was retained before receipt fault injection")
            }
            _ => panic!("terminal cleanup did not progress toward compact closure"),
        }
        let compact = snapshot(storage, store, admission);
        if compact.head().unwrap().source_root().count() == 0
            && compact.head().unwrap().target_root().count() == 0
        {
            assert_eq!(compact.head().unwrap().charge().associations(), 0);
            return;
        }
    }
    panic!("bounded one-association cleanup did not reach compact closure");
}

pub(super) fn submit_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    page_command: u8,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) {
    let flight = submission_flight(storage, store, admission, page_command, eof, associations);
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
    ));
}

pub(super) fn submission_flight(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
    page_command: u8,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessPageSubmissionFlightV1 {
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            request(admission, page_command, eof, associations),
        )
        .unwrap();
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.into_submission_flight(store, receipt).unwrap()
}

pub(super) fn request(
    admission: DraftMarkerAdmissionOwnerV1,
    page_command: u8,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
) -> DraftMarkerLabelReadinessPageRequestV1 {
    DraftMarkerLabelReadinessPageRequestV1::new(
        admission,
        command(page_command),
        NonZeroU64::MIN,
        eof,
        DraftMarkerLabelReadinessDispositionV1::Reuse,
        associations.into_boxed_slice(),
        None,
    )
}

pub(super) fn snapshot(
    storage: &SyndicStorage,
    store: &HomeStore,
    admission: DraftMarkerAdmissionOwnerV1,
) -> syndic_storage::DraftMarkerAdmissionPublicationSnapshotV1 {
    storage
        .draft_marker_admission_publication_snapshot_for_test(store, admission, &[])
        .unwrap()
}

pub(super) fn command(seed: u8) -> DraftMarkerAdmissionCommandIdV1 {
    DraftMarkerAdmissionCommandIdV1::from_bytes([seed; 16])
}

pub(super) fn assert_advanced(outcome: DraftMarkerAdmissionTerminalOutcomeV1) {
    assert!(matches!(
        outcome,
        DraftMarkerAdmissionTerminalOutcomeV1::Advanced { .. }
    ));
}
