use super::*;
use beryl_app::catalog_projection::prepare_thread_catalog_projection;
use beryl_model::{WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
use beryl_state::{InitializeThreadlessWindow, RememberedTarget, ReplaceWindowClaim};

fn commit(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn catalog_collision_rejects_syndic_creation_without_overwriting_either_copy() {
    let fixture = Fixture::new(1);
    let command =
        prepare_thread_catalog_projection(&fixture.store, &fixture.syndic, &fixture.state, id(30))
            .unwrap()
            .into_command()
            .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            id(30),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let publication = fixture
        .state
        .catalog()
        .prepare_initial_publication(&fixture.store, id(210), row.sources(), row.facts().clone())
        .unwrap();
    let witness = publication.publication().clone();
    commit(&fixture.store, publication.contribution());
    let prepared = fixture.prepare(210, CommandCancellation::new());
    assert!(matches!(
        prepared.execute(),
        DiscussionCreationOutcome::NotCommitted { .. }
    ));
    assert!(
        fixture
            .syndic
            .thread(&fixture.store, id(210), limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .state
            .catalog()
            .initial_publication_status(&fixture.store, &witness)
            .unwrap(),
        beryl_state::CatalogInitialStatus::Exact
    );
    fixture.store.close().unwrap();
}

#[test]
fn new_claim_rejects_prepared_creation_and_already_claimed_identity_cannot_prepare() {
    let fixture = Fixture::new(1);
    let session = fixture.state.session();
    let window = WindowId::from_bytes([200; 16]);
    commit(
        &fixture.store,
        session.initialize_threadless(
            session.revision(&fixture.store).unwrap(),
            InitializeThreadlessWindow::new(
                window,
                WindowPlacement::new(
                    WindowBounds::new(0, 0, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
    let prepared = fixture.prepare(210, CommandCancellation::new());
    let initial = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    let binding = fixture
        .syndic
        .thread_catalog_summary(&fixture.store, id(30), limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    commit(
        &fixture.store,
        session.replace_claim(
            session.revision(&fixture.store).unwrap(),
            ReplaceWindowClaim::new(
                initial.header().revision(),
                window,
                initial.windows()[0].revision(),
                None,
                RememberedTarget::new(binding.runtime_id(), binding.root_id()),
                id(210),
            ),
        ),
    );
    assert!(matches!(
        prepared.execute(),
        DiscussionCreationOutcome::NotCommitted { .. }
    ));
    no_child(&fixture.store, &fixture.syndic, &fixture.state, 210);
    assert!(matches!(
        fixture.service().prepare(
            source(&fixture.store, &fixture.syndic),
            request(210),
            CommandCancellation::new()
        ),
        Err(DiscussionCreationError::AlreadyClaimed)
    ));
    fixture.store.close().unwrap();
}
