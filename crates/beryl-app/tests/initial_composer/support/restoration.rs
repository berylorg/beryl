use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_state::{BeginSessionRestore, MinimalSessionBootstrap};

pub(super) fn snapshot(fixture: &Fixture) -> MinimalSessionBootstrap {
    fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap()
}

pub(super) fn begin_restore(fixture: &Fixture) {
    let session = fixture.state.session();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(session.begin_restore(
            session.revision(&fixture.store).unwrap(),
            BeginSessionRestore::new(snapshot(fixture).header().revision()),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub(super) fn attempt(
    fixture: &Fixture,
) -> (
    RestoredWindowPreparationAttempt,
    RestoredWindowServiceTestLifetime,
) {
    RestoredWindowPreparationAttempt::new_for_test(
        fixture.store.clone(),
        fixture.state.session(),
        fixture.storage.clone(),
    )
    .unwrap()
}

pub(super) fn begin(
    fixture: &Fixture,
    attempt: &RestoredWindowPreparationAttempt,
    seed: u8,
) -> RestoredWindowComposer {
    let snapshot = snapshot(fixture);
    let window = &snapshot.windows()[0];
    begin_window(fixture, attempt, window.window_id(), seed)
}

pub(super) fn begin_window(
    fixture: &Fixture,
    attempt: &RestoredWindowPreparationAttempt,
    window_id: WindowId,
    seed: u8,
) -> RestoredWindowComposer {
    let snapshot = snapshot(fixture);
    let window = snapshot
        .windows()
        .iter()
        .find(|w| w.window_id() == window_id)
        .unwrap();
    attempt
        .begin(
            snapshot.header().revision(),
            window.window_id(),
            composer_support::activation(
                window.selected_thread().unwrap().thread_id(),
                seed,
                seed.wrapping_add(1),
                1,
                0,
            ),
            composer_support::fixture::operation_id(seed.wrapping_add(2)),
            MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets()),
        )
        .unwrap()
}

pub(super) fn open(
    custody: &mut RestoredWindowComposer,
    attempt: &RestoredWindowPreparationAttempt,
) {
    assert_eq!(
        custody
            .advance(attempt, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
}

pub(super) fn paired_claim(fixture: &Fixture, window: WindowId) -> beryl_state::ThreadClaimRecord {
    fixture
        .state
        .session()
        .thread_claim_catalog_source(&fixture.store, fixture.claim(window).thread_id())
        .unwrap()
        .claim()
        .unwrap()
}

pub(super) fn retire(custody: RestoredWindowComposer) {
    match custody.retire(CommandCancellation::new()) {
        RestoredWindowComposerRetirement::Retired => {}
        RestoredWindowComposerRetirement::Pending(failure) => panic!("{}", failure.error),
    }
}
