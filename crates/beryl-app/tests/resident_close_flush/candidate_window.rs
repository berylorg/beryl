use super::*;

#[gpui::test]
fn candidate_source_refuses_changed_window_with_unchanged_claim(cx: &mut TestAppContext) {
    let fixture = Fixture::new("candidate-window-changed", 173);
    let facts = slot_close::retired(&fixture, cx.new(|_| ()).entity_id());
    let old_seed = seed(&facts, composer::position(0), composer::position(0));
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let source = Source::new(&mut candidate, facts, storage, &state, old_seed)
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    let record = source.window().clone();
    let access = candidate.recovery_access().unwrap();
    let session = state.session();
    let snapshot = session
        .minimal_bootstrap_candidate(&access)
        .unwrap()
        .unwrap();
    let changed = beryl_model::WindowPlacement::new(
        beryl_model::WindowBounds::new(123, 456, 812, 613).unwrap(),
        beryl_model::WindowDisplayState::Normal,
        None,
        None,
    );
    assert_ne!(&changed, record.placement());
    let mut command = beryl_home_store::HomeCommand::new(access.home_revision().unwrap());
    command
        .add(session.update_placement(
            session.revision_candidate(&access).unwrap(),
            beryl_state::UpdateWindowPlacement::new(
                snapshot.header().revision(),
                record.window_id(),
                record.revision(),
                changed,
            ),
        ))
        .unwrap();
    assert!(matches!(
        access.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let snapshot = session
        .minimal_bootstrap_candidate(&access)
        .unwrap()
        .unwrap();
    let current = snapshot
        .windows()
        .iter()
        .find(|window| window.window_id() == record.window_id())
        .unwrap();
    assert_eq!(current.selected_thread(), record.selected_thread());
    assert_ne!(current.revision(), record.revision());
    let request = page(source.seed().binding, 42, 0, PageDirection::Forward, 4);
    assert!(
        source
            .text_page(&access, request)
            .unwrap_err()
            .contains("window record changed")
    );
    assert!(
        source
            .object_page(&access, objects(source.seed().binding, 1, None, 2))
            .unwrap_err()
            .contains("window record changed")
    );
    assert_eq!(source.window(), &record);
    drop(access);
    drop(source);
    candidate.abort().close().unwrap();
}
