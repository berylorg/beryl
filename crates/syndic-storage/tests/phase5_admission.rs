#![cfg(feature = "test-faults")]

mod support;

use std::num::NonZeroU64;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, AssetId, DraftRevision, InputGateRevision, SyndicDraftMarkerId,
    SyndicItemId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::populated::{active_turn, populated_records, steering_input};
use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    _storage: SyndicStorage,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn create_thread(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    draft: beryl_model::SyndicDraftId,
) {
    execute(
        store,
        storage,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(thread, draft, timestamp(1)),
        ),
    );
}

fn publish_payload(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    payload: &ComposerPayload,
    updated_at: SyndicTimestamp,
) {
    let content = PreparedContent::composer(payload).unwrap();
    execute(
        store,
        storage,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(&content),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, &content).unwrap() {
        manifest = append.next_manifest().clone();
        execute(
            store,
            storage,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, updated_at).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("test payload must be new"),
    };
    execute(
        store,
        storage,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
}

#[test]
fn idle_submission_consumes_the_draft_and_publishes_one_pending_turn() {
    let home = TestHome::new("phase5-idle-submission");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    let next_draft = draft_id(3);
    let item = SyndicItemId::from_bytes([4; 16]);
    create_thread(&store, storage, thread, draft);

    let marker_id = SyndicDraftMarkerId::from_bytes([5; 16]);
    let label = ImageLabelOrdinal::FIRST;
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("hello").unwrap(),
        ComposerAtom::image_marker(marker_id, label),
    ])
    .unwrap();
    publish_payload(&store, storage, thread, &payload, timestamp(2));
    let asset = AssetId::sha256_v1([6; 32], NonZeroU64::new(64).unwrap());
    let markers =
        AdmissionMarkers::new(vec![ResolvedImageMarker::new(marker_id, label, asset)]).unwrap();
    let expected_content = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let submission = IdleSubmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(1).unwrap(),
        next_draft,
        item,
        markers,
        timestamp(3),
    );
    assert_eq!(
        storage
            .idle_submission_status(&store, &submission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    execute(
        &store,
        storage,
        storage.submit_idle_draft(storage.revision(&store).unwrap(), submission.clone()),
    );
    store.validate_registered_domains().unwrap();

    assert!(
        storage
            .draft(&store, draft, point_limit())
            .unwrap()
            .is_none()
    );
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().revision().get(), 2);
    assert_eq!(current.draft().id(), next_draft);
    let turn_id = draft.submitted_turn_id();
    let turn = storage
        .turn(&store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(turn.record().parent(), ConversationParent::Root);
    let state = storage
        .turn_state(&store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.record().item_count(), 1);
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision().get(), 2);
    assert_eq!(gate.record().state(), &InputGateState::PendingTurn(turn_id));
    let markers = storage
        .input_marker_resolutions(
            &store,
            InputMarkerOwner::CanonicalItem(item),
            None,
            CursorReadLimits::new(2, 4_096).unwrap(),
        )
        .unwrap();
    assert_eq!(markers.records().len(), 1);
    assert_eq!(markers.records()[0].marker().asset_id(), asset);
    assert_eq!(
        storage
            .idle_submission_status(&store, &submission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactSubmitted
    );
    let colliding = IdleSubmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(1).unwrap(),
        draft_id(99),
        item,
        submission.markers().clone(),
        timestamp(3),
    );
    assert_eq!(
        storage
            .idle_submission_status(&store, &colliding, point_limit())
            .unwrap(),
        InputAdmissionStatus::Collision
    );
    let mut duplicate = HomeCommand::new(store.home_revision().unwrap());
    duplicate
        .add(storage.submit_idle_draft(storage.revision(&store).unwrap(), submission.clone()))
        .unwrap();
    assert!(store.execute(duplicate).is_err());
    assert_eq!(
        storage
            .idle_submission_status(&store, &submission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactSubmitted
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn active_submission_rotates_the_draft_into_permanent_and_live_steering_order() {
    let home = TestHome::new("phase5-active-admission");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let thread = id(40);
    let draft = draft_id(41);
    let next_draft = draft_id(60);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("steer me").unwrap()]).unwrap();
    publish_payload(&store, storage, thread, &payload, timestamp(9));
    let expected_content = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();

    let admission = AcceptedInputAdmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(3).unwrap(),
        next_draft,
        AdmissionMarkers::default(),
        timestamp(10),
    );
    let accepted_id = admission.accepted_input_id();
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::Absent
    );
    execute(
        &store,
        storage,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission.clone()),
    );
    store.validate_registered_domains().unwrap();

    assert!(
        storage
            .draft(&store, draft, point_limit())
            .unwrap()
            .is_none()
    );
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().revision().get(), 2);
    assert_eq!(current.thread().committed_tail(), Some(active_turn()));
    assert_eq!(current.draft().id(), next_draft);
    let input = storage
        .accepted_input(&store, accepted_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(input.record().ordinal().get(), 3);
    assert_eq!(input.record().gate_revision().get(), 3);
    assert!(matches!(
        input.record().disposition(),
        AcceptedInputDisposition::SteerActiveTurn(_)
    ));
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision().get(), 4);
    assert_eq!(gate.record().accepted_high_water(), 3);
    assert_eq!(gate.record().live_steering_count(), 2);
    assert_eq!(gate.record().live_next_turn_count(), 1);
    assert_eq!(
        gate.record().live_logical_utf8_bytes(),
        payload.utf8_bytes() as u64
    );
    let order = storage
        .accepted_order(
            &store,
            thread,
            Some(AcceptedInputOrdinal::new(2).unwrap()),
            CursorReadLimits::new(2, 4_096).unwrap(),
        )
        .unwrap();
    assert_eq!(order.records().len(), 1);
    assert_eq!(order.records()[0].input_id(), accepted_id);
    assert!(
        storage
            .turn(&store, draft.submitted_turn_id(), point_limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(input.record().thread_id(), admission.thread_id());
    assert_eq!(
        input.record().gate_revision(),
        admission.expected_gate_revision()
    );
    assert_eq!(input.record().content(), admission.expected_content());
    assert_eq!(input.record().admitted_at(), admission.admitted_at());
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    let colliding = AcceptedInputAdmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        expected_content,
        InputGateRevision::new(3).unwrap(),
        draft_id(99),
        AdmissionMarkers::default(),
        timestamp(10),
    );
    assert_eq!(
        storage
            .accepted_input_status(&store, &colliding, point_limit())
            .unwrap(),
        InputAdmissionStatus::Collision
    );
    let mut duplicate = HomeCommand::new(store.home_revision().unwrap());
    duplicate
        .add(storage.admit_accepted_input(storage.revision(&store).unwrap(), admission.clone()))
        .unwrap();
    assert!(store.execute(duplicate).is_err());
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn steering_rejection_preserves_identity_and_swaps_only_the_live_route() {
    let home = TestHome::new("phase5-steering-rejection");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records = populated_records();
    for record in &mut records {
        if let FixtureRecord::AcceptedInput(input) = record
            && input.id() == steering_input()
        {
            *input = AcceptedInputRecord::new(
                input.id(),
                input.thread_id(),
                input.revision(),
                input.ordinal(),
                input.gate_revision(),
                input.disposition().clone(),
                AcceptedInputLifecycle::Delivering,
                input.content(),
                input.marker_count(),
                input.admitted_at(),
            );
        }
    }
    commit(&store, storage, batch(records));

    execute(
        &store,
        storage,
        storage.record_steering_rejection(
            storage.revision(&store).unwrap(),
            SteeringRejection::new(
                id(40),
                InputGateRevision::new(3).unwrap(),
                steering_input(),
                AcceptedInputRevision::new(1).unwrap(),
            ),
        ),
    );
    store.validate_registered_domains().unwrap();

    let input = storage
        .accepted_input(&store, steering_input(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(input.record().revision().get(), 2);
    assert_eq!(input.record().gate_revision().get(), 2);
    assert_eq!(
        input.record().lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert_eq!(
        input.record().disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::SteeringRejected)
    );
    let steering = storage
        .accepted_steering(
            &store,
            id(40),
            active_turn(),
            None,
            CursorReadLimits::new(4, 4_096).unwrap(),
        )
        .unwrap();
    assert!(steering.records().is_empty());
    let next = storage
        .accepted_next_turn(
            &store,
            id(40),
            None,
            CursorReadLimits::new(4, 4_096).unwrap(),
        )
        .unwrap();
    assert_eq!(next.records().len(), 2);
    assert_eq!(next.records()[0].input_id(), steering_input());
    let gate = storage
        .input_gate(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision().get(), 4);
    assert_eq!(gate.record().live_steering_count(), 0);
    assert_eq!(gate.record().live_next_turn_count(), 2);
}

#[test]
fn stale_draft_revision_rejects_admission_without_consuming_the_current_draft() {
    let home = TestHome::new("phase5-stale-admission");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(70);
    let draft = draft_id(71);
    create_thread(&store, storage, thread, draft);
    let payload = ComposerPayload::new(vec![ComposerAtom::text("keep me").unwrap()]).unwrap();
    publish_payload(&store, storage, thread, &payload, timestamp(2));
    let expected_content = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    let submission = IdleSubmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(1).unwrap(),
        expected_content,
        InputGateRevision::new(1).unwrap(),
        draft_id(72),
        SyndicItemId::from_bytes([73; 16]),
        AdmissionMarkers::default(),
        timestamp(3),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.submit_idle_draft(storage.revision(&store).unwrap(), submission))
        .unwrap();
    assert!(store.execute(command).is_err());
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), draft);
    assert_eq!(current.draft().revision().get(), 2);
    assert_eq!(read_composer_payload(&store, storage, &current), payload);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
