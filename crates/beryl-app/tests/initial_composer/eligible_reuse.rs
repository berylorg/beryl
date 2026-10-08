use super::*;

#[test]
fn typed_then_removed_reused_target_mounts_and_retires_with_original_history() {
    let fixture = Fixture::new(151);
    let thread = fixture.seed_pristine(161);
    empty_after_edit::publish_empty_after_typing(
        &fixture.store,
        &fixture.state,
        &fixture.storage,
        thread,
    );
    let limit = syndic_storage::SyndicPointReadLimit::new(65536).unwrap();
    let original = fixture
        .storage
        .current_draft(&fixture.store, thread, limit)
        .unwrap()
        .unwrap();
    for seed in [171, 181] {
        let mut custody = fixture.begin(seed);
        assert_eq!(custody.acquisition().thread_id(), thread);
        assert_eq!(
            custody.acquisition().disposition(),
            RuntimeBackedWindowAcquisitionDisposition::Reused
        );
        assert_eq!(
            custody.advance(&CommandCancellation::new()).unwrap(),
            MainWindowInitialComposerProgress::Activated
        );
        let prepared = custody
            .prepare(&mut config)
            .unwrap_or_else(|failure| panic!("{}", failure.error));
        let (editor, custody) = prepared.into_parts();
        let binding = editor.selection_identity().binding();
        assert_eq!(binding.candidate().draft_id(), original.draft().id());
        assert_eq!(binding.root(), original.draft().piece_root());
        assert!(binding.range_history_frontier().undo_available);
        drop(editor);
        fixture.retire_and_release(custody);
        assert_eq!(fixture.process.main_window_occupancy(), 0);
        let current = fixture
            .storage
            .current_draft(&fixture.store, thread, limit)
            .unwrap()
            .unwrap();
        assert_eq!(current.draft().piece_root(), original.draft().piece_root());
        assert_eq!(current.draft().history(), original.draft().history());
    }
}

#[test]
fn missing_catalog_reused_target_mounts_and_abandons_without_becoming_a_created_fallback() {
    let fixture = Fixture::new(152);
    let thread = SyndicThreadId::from_bytes([162; 16]);
    let draft = SyndicDraftId::from_bytes([163; 16]);
    let mut command = beryl_home_store::HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            syndic_storage::CreateThread::ordinary(
                thread,
                draft,
                fixture.execution(),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
    assert!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                thread,
                beryl_state::CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    let mut custody = fixture.begin(172);
    assert_eq!(custody.acquisition().thread_id(), thread);
    assert_eq!(
        custody.acquisition().disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(
        custody.advance(&CommandCancellation::new()).unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let (editor, custody) = custody
        .prepare(&mut config)
        .unwrap_or_else(|failure| panic!("{}", failure.error))
        .into_parts();
    assert_eq!(
        editor.selection_identity().binding().candidate().draft_id(),
        draft
    );
    drop(editor);
    fixture.retire_and_release(custody);
    assert_eq!(fixture.process.main_window_occupancy(), 0);
    assert!(
        fixture
            .storage
            .current_draft(
                &fixture.store,
                thread,
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .is_some()
    );
}
