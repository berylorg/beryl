use super::support::slot_close::retired;
use super::widget_support::fixture::Fixture;
use beryl_app::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushPurpose, ComposerHostFlushState,
};
use beryl_home_store::test_faults::FaultPoint;
use beryl_state::{BerylState, ReplaceWindowClaim};
use gpui::{AppContext, TestAppContext};
use syndic_storage::SyndicStorage;

#[gpui::test]
fn recovered_slot_keeps_fresh_close_gates_and_rejects_stale_handles(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rebind-retired-slot", 201);
    let owner = cx.new(|_| ()).entity_id();
    let mut facts = retired(&fixture, owner);
    let selection = facts.selection();
    let close = facts.close_ticket();
    let old_state = BerylState::reacquire(&fixture.store).unwrap();
    let foreign = Fixture::new("foreign-rebind-slot", 202);
    let foreign_state = BerylState::reacquire(&foreign.store).unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut recovery = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&recovery).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let revision = access.home_revision().unwrap();
    for stale in [&old_state, &foreign_state] {
        facts = facts
            .rebind_candidate(&access, storage.clone(), stale)
            .err()
            .unwrap()
            .0;
        assert_eq!(facts.selection(), selection);
        assert_eq!(facts.close_ticket(), close);
    }
    facts = facts
        .rebind_candidate(&access, fixture.storage.clone(), &state)
        .err()
        .unwrap()
        .0;
    assert_eq!(facts.selection(), selection);
    let (mut slot, fresh_close) = facts
        .rebind_candidate(&access, storage, &state)
        .unwrap_or_else(|(_, error)| panic!("slot reconstruction failed: {error}"));
    let fresh = slot.selected_identity().unwrap();
    assert_eq!(fresh.window_id(), selection.window_id());
    assert_eq!(fresh.claim(), selection.claim());
    assert_eq!(fresh.binding().candidate(), selection.binding().candidate());
    assert_eq!(fresh.binding().home_generation(), access.generation());
    assert_ne!(fresh.binding(), selection.binding());
    assert_ne!(fresh_close, close);
    assert!(!slot.test_release_window_close_gate(close, None).unwrap());
    assert!(
        slot.begin_selected_flush(fresh, ComposerHostFlushPurpose::ApplicationExit)
            .is_err()
    );
    let ComposerHostFlushAdmission::Joined { ticket, state } = slot
        .begin_selected_flush(fresh, ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    else {
        panic!("reconstruction must retain its host gate");
    };
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert!(
        slot.begin_selected_flush(selection, ComposerHostFlushPurpose::WindowClose)
            .is_err()
    );
    assert_eq!(access.home_revision().unwrap(), revision);
    assert!(slot.retire_clean_window_close(fresh_close, ticket).is_err());
    recovery.abort().close().unwrap();
}

#[gpui::test]
fn changed_claim_or_failed_candidate_read_preserves_retirement_facts(cx: &mut TestAppContext) {
    for scenario in 0..3 {
        let fixture = Fixture::new("refuse-slot-rebind", 211 + scenario);
        let owner = cx.new(|_| ()).entity_id();
        let facts = retired(&fixture, owner);
        let selection = facts.selection();
        let close = facts.close_ticket();
        if scenario != 0 {
            let state = BerylState::reacquire(&fixture.store).unwrap();
            let session = state.session();
            let bootstrap = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
            let window = bootstrap
                .windows()
                .iter()
                .find(|window| window.window_id() == selection.window_id())
                .unwrap();
            let mut command =
                beryl_home_store::HomeCommand::new(fixture.store.home_revision().unwrap());
            let contribution = if scenario == 2 {
                session.delete_thread_claim_copy_for_test(
                    session.revision(&fixture.store).unwrap(),
                    selection.window_id(),
                    selection.claim().thread_id(),
                )
            } else {
                session.replace_claim(
                    session.revision(&fixture.store).unwrap(),
                    ReplaceWindowClaim::new(
                        bootstrap.header().revision(),
                        window.window_id(),
                        window.revision(),
                        Some(selection.claim()),
                        window.remembered_target().unwrap(),
                        fixture.target_thread,
                    ),
                )
            };
            command.add(contribution).unwrap();
            assert!(matches!(
                fixture.store.execute(command),
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
        }
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let mut recovery = fixture.store.recover_same_home().unwrap();
        let state = BerylState::reacquire_candidate(&recovery).unwrap();
        let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        if scenario == 0 {
            fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let (facts, _) = facts
            .rebind_candidate(&access, storage, &state)
            .err()
            .unwrap();
        assert_eq!(facts.selection(), selection);
        assert_eq!(facts.close_ticket(), close);
        assert_eq!(facts.host().binding(), selection.binding());
        recovery.abort().close().unwrap();
    }
}
