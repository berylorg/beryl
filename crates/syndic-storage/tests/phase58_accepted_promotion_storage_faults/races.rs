use beryl_home_store::HomeCommand;
use beryl_model::{ContentRevision, SyndicDraftId, SyndicItemId, SyndicTurnId};

use super::*;

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

#[test]
fn committed_autosave_invalidates_only_the_stale_promotion_basis() {
    let fixture = promotion_fixture(94, id(94));
    let (_home, store, storage) = seed("phase58-promotion-autosave-race", fixture.records.clone());
    let stale = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([150; 16]),
        SyndicItemId::from_bytes([151; 16]),
    );
    let current = storage
        .current_draft(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let saved_payload =
        ComposerPayload::new(vec![ComposerAtom::text("queued input").unwrap()]).unwrap();
    let saved = PreparedContent::composer(&saved_payload).unwrap();
    assert_eq!(
        saved.reference(ContentRevision::new(1).unwrap()),
        fixture.accepted_content,
    );
    let update = match DraftPayloadUpdate::prepare(&current, &saved, timestamp(21)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("autosave fixture must change the draft"),
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );

    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &stale, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision,
    );
    assert!(execute_promotion(&store, storage, stale.clone()).is_err());

    let fresh = PromoteAcceptedInput::new(
        candidate(&store, storage),
        stale.successor_turn_id(),
        stale.successor_item_id(),
        timestamp(22),
    );
    execute_promotion(&store, storage, fresh.clone()).unwrap();
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &fresh, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact,
    );
    let preserved = storage
        .draft(&store, fixture.current_draft, limit())
        .unwrap()
        .unwrap();
    assert_eq!(preserved.content(), fixture.accepted_content);
}

#[test]
fn ordinary_submit_cannot_overtake_live_next_turn_promotion() {
    let fixture = promotion_fixture(95, id(95));
    let (_home, store, storage) = seed(
        "phase58-promotion-ordinary-submit-race",
        fixture.records.clone(),
    );
    let request = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([152; 16]),
        SyndicItemId::from_bytes([153; 16]),
    );
    let current = storage
        .current_draft(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let ordinary = IdleSubmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([154; 16]),
        SyndicItemId::from_bytes([155; 16]),
        None,
        timestamp(21),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.submit_idle_draft(storage.revision(&store).unwrap(), ordinary))
        .unwrap();
    assert!(store.execute(command).is_err());
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior,
    );

    execute_promotion(&store, storage, request.clone()).unwrap();
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact,
    );
}

#[test]
fn gate_only_revision_advance_is_not_a_compatible_promotion_descendant() {
    let fixture = promotion_fixture(96, id(96));
    let (_home, store, storage) = seed(
        "phase60-promotion-incompatible-gate-descendant",
        fixture.records.clone(),
    );
    let request = promotion(
        &store,
        storage,
        SyndicTurnId::from_bytes([156; 16]),
        SyndicItemId::from_bytes([157; 16]),
    );
    execute_promotion(&store, storage, request.clone()).unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let impossible = InputGateRecord::new(
        gate.thread_id(),
        gate.revision().checked_next().unwrap(),
        gate.state().clone(),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(impossible)]),
    );

    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision,
    );
}
