use super::*;
use beryl_app::main_window::MainWindowComposerClipboardFeedbackKind;

#[gpui::test]
fn stale_copy_page_releases_process_capacity_without_writing(cx: &mut gpui::TestAppContext) {
    let first = source(241);
    let first_writes = Arc::new(AtomicUsize::new(0));
    let observed = first_writes.clone();
    let (_composer, cx) = mount(
        cx,
        &first,
        1024,
        Box::new(move |_, _, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            ClipboardWriteOutcome::Written
        }),
    );
    let release = first.service.test_block_next_selected_page_dispatch();
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 16);
    first.service.test_with_selected_host(|host| {
        let binding = host.binding().unwrap();
        composer_support::commit_text(host, &first.store, binding, 9001, 2, 2, "C", 3, 1);
    });
    release.release();
    drive_owner(cx, 32);
    assert_eq!(first_writes.load(Ordering::SeqCst), 0);
    let second = source(251);
    let second_writes = Arc::new(AtomicUsize::new(0));
    let observed = second_writes.clone();
    let (composer, cx) = mount(
        cx,
        &second,
        1024,
        Box::new(move |_, _, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            ClipboardWriteOutcome::Written
        }),
    );
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 64);
    assert_eq!(second_writes.load(Ordering::SeqCst), 1);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
}

#[gpui::test]
fn private_cut_capacity_is_shared_across_mounted_composers(cx: &mut gpui::TestAppContext) {
    let first = source(211);
    let first_writes = Arc::new(Mutex::new(Vec::new()));
    let captured = first_writes.clone();
    let (_first_composer, cx) = mount(
        cx,
        &first,
        1024,
        Box::new(move |_, metadata, _| {
            captured.lock().unwrap().push(metadata.unwrap().to_owned());
            ClipboardWriteOutcome::Written
        }),
    );
    let release = first.service.test_block_next_cut_preparation();
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 32);
    assert_eq!(first_writes.lock().unwrap().len(), 1);
    let second = source(221);
    let second_writes = Arc::new(AtomicUsize::new(0));
    let observed = second_writes.clone();
    let (composer, cx) = mount(
        cx,
        &second,
        1024,
        Box::new(move |_, _, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            ClipboardWriteOutcome::Written
        }),
    );
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 32);
    assert_eq!(second_writes.load(Ordering::SeqCst), 0);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::CapacityUnavailable
    );
    assert_eq!(second.service.selected_identity(), Some(second.selection));
    release.release();
    drive_owner(cx, 64);
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 64);
    assert_eq!(second_writes.load(Ordering::SeqCst), 1);
    assert!(
        first
            .service
            .private_clipboard_owner()
            .unwrap()
            .descriptor(&first_writes.lock().unwrap()[0])
            .is_none()
    );
}

#[gpui::test]
fn private_clipboard_limit_never_writes_or_cuts(cx: &mut gpui::TestAppContext) {
    let fixture = source(231);
    let writes = Arc::new(AtomicUsize::new(0));
    let captured = writes.clone();
    let (composer, cx) = mount(
        cx,
        &fixture,
        1,
        Box::new(move |_, _, _| {
            captured.fetch_add(1, Ordering::SeqCst);
            ClipboardWriteOutcome::Written
        }),
    );
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 64);
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.service.selected_identity(), Some(fixture.selection));
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::TooLarge
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
}

struct FixtureSource {
    service: Arc<MainWindowConversationComposerService>,
    store: HomeStore,
    storage: syndic_storage::SyndicStorage,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    marker_before: SourcePosition,
    _directory: tempfile::TempDir,
}

fn source(seed: u8) -> FixtureSource {
    let fixture = Fixture::new("private-composer-clipboard", seed);
    let assets = fixture.assets();
    let marker_asset = publication_support::publish_image_asset(
        &fixture.store,
        assets.clone(),
        b"private-composer-image",
    );
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let (claim, _) = fixture.claims();
    let (directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    host.test_activate(
        &store,
        ComposerHostActivationRequest::new(
            thread,
            syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([seed.wrapping_add(1); 16]),
            support::operation_id(seed.wrapping_add(2)),
            NonZeroU64::new(1).unwrap(),
            None,
            Box::new([]),
        ),
        &CommandCancellation::new(),
    )
    .unwrap();
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 304, 0, 0, "AB", 2, 1);
    let binding = insert_marker_at(&mut host, &store, &assets, binding, 305, marker_asset, 0);
    let marker_before = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(
            InlineObjectId::new(0x1001),
            InlineObjectOrder::new(1),
        )),
    );
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage.clone());
    let rebound = activate_with_initial_pages(
        &mut host,
        &store,
        thread,
        seed.wrapping_add(1),
        seed.wrapping_add(2),
        2,
        Some(DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(0x1001_u128.to_be_bytes()),
        }),
    );
    assert_eq!(binding.root(), rebound.root());
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker_authority)
            .unwrap();
    let selection = slot.selected_identity().unwrap();
    let protection = storage
        .draft_image_label_protection_head(
            &store,
            thread,
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(
        protection.protected_maximum().contains(
            selection
                .binding()
                .root()
                .marker_commitment()
                .maximum_image_label()
                .unwrap()
        )
    );
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.service_reference(),
        slot,
    ));
    FixtureSource {
        _directory: directory,
        store,
        storage,
        service,
        selection,
        marker_before,
    }
}

fn mount<'a>(
    cx: &'a mut gpui::TestAppContext,
    fixture: &FixtureSource,
    maximum: usize,
    writer: beryl_app::main_window::ComposerCheckedClipboardWriter,
) -> (
    gpui::Entity<MainWindowConversationComposer>,
    &'a mut gpui::VisualTestContext,
) {
    cx.update(ensure_text_input_bindings);
    let configuration = MainWindowConversationComposerConfig::new(
        fixture.selection,
        widget_config(fixture.selection.binding().range_binding(), maximum),
    )
    .unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        let mut composer = MainWindowConversationComposer::new(
            configuration,
            fixture.service.clone(),
            Box::new(|_, _| ClipboardWriteOutcome::Failed),
            window,
            cx,
        )
        .unwrap();
        composer.test_set_checked_clipboard_writer(writer);
        composer
            .gpui_input()
            .update(cx, |input, _| input.focus(window));
        composer
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input
                .rebind(
                    fixture.selection.binding().range_binding(),
                    Some(RangeSourceSelection {
                        anchor: fixture.marker_before,
                        head: composer_support::position(1),
                    }),
                    window,
                    cx,
                )
                .unwrap();
        })
    });
    drive_owner(cx, 12);
    (composer, cx)
}

#[gpui::test]
fn private_copy_replacement_and_metadata_failure_preserve_draft(cx: &mut gpui::TestAppContext) {
    let fixture = source(171);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let (composer, cx) = mount(
        cx,
        &fixture,
        1024,
        Box::new(move |text, metadata, _| {
            captured
                .lock()
                .unwrap()
                .push((text.to_owned(), metadata.unwrap().to_owned()));
            ClipboardWriteOutcome::Written
        }),
    );
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 64);
    let owner = fixture.service.private_clipboard_owner().unwrap();
    let token = writes.lock().unwrap()[0].1.clone();
    let copied = owner.descriptor(&token).unwrap();
    assert_eq!(copied.origin, fixture.selection);
    assert_eq!(copied.content_origin, fixture.selection);
    assert_eq!(copied.closure.item_count(), 1);
    assert_eq!(copied.closure.output_bytes(), 10);
    assert_eq!(writes.lock().unwrap()[0].0, "[Image A]A");
    let retained_service = Arc::strong_count(&fixture.service);
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 64);
    assert_eq!(Arc::strong_count(&fixture.service), retained_service);
    let newer_token = writes.lock().unwrap()[1].1.clone();
    assert_ne!(token, newer_token);
    assert!(owner.descriptor(&token).is_none());
    owner.observe_clipboard_metadata(&token, None);
    assert!(owner.descriptor(&newer_token).is_some());
    let token = newer_token;
    let failures = Arc::new(Mutex::new(Vec::new()));
    let observed = failures.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_writer(Box::new(move |_, metadata, _| {
            observed.lock().unwrap().push(metadata.unwrap().to_owned());
            ClipboardWriteOutcome::Failed
        }))
    });
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 64);
    assert_eq!(fixture.service.selected_identity(), Some(fixture.selection));
    assert_eq!(failures.lock().unwrap().len(), 1);
    assert!(owner.descriptor(&token).is_none());
    assert!(owner.descriptor(&failures.lock().unwrap()[0]).is_none());
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::Failed
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
}

#[gpui::test]
fn private_cut_is_unavailable_until_exact_commit_and_undo_expires_it(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = source(181);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let (composer, cx) = mount(
        cx,
        &fixture,
        1024,
        Box::new(move |text, metadata, _| {
            captured
                .lock()
                .unwrap()
                .push((text.to_owned(), metadata.unwrap().to_owned()));
            ClipboardWriteOutcome::Written
        }),
    );
    let release = fixture.service.test_block_next_cut_preparation();
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 32);
    let token = writes.lock().unwrap()[0].1.clone();
    let owner = fixture.service.private_clipboard_owner().unwrap();
    assert!(owner.descriptor(&token).is_none());
    assert_eq!(fixture.service.selected_identity(), Some(fixture.selection));
    release.release();
    drive_owner(cx, 64);
    let cut = fixture.service.selected_identity().unwrap();
    let descriptor = owner.descriptor(&token).unwrap();
    assert_eq!(descriptor.origin, cut);
    assert_eq!(descriptor.content_origin, fixture.selection);
    assert_eq!(
        descriptor.source.content_root(),
        fixture.selection.binding().root()
    );
    assert!(
        fixture
            .storage
            .draft_private_clipboard_source_is_current(&fixture.store, descriptor.source)
            .unwrap()
    );
    assert_eq!(cut.binding().root().summary().marker_count(), 0);
    assert_eq!(cut.binding().logical_extent().logical_utf8_bytes(), 1);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    drive_current_surface(&composer, &input, cx, 256);
    for _ in 0..256 {
        if input.read_with(cx, |input, _| input.is_semantically_quiescent()) {
            break;
        }
        drive_owner(cx, 1);
    }
    assert!(input.read_with(cx, |input, _| input.is_semantically_quiescent()));
    assert!(input.read_with(cx, |input, _| input.history_frontier().undo_available));
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    assert_eq!(
        input.read_with(cx, |input, _| input.history_frontier().binding()),
        cut.binding().range_binding()
    );
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        assert_eq!(
            window.focused(app),
            Some(gpui::Focusable::focus_handle(input.read(app), app))
        );
    });
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_selected_surface(&composer, &input, &fixture.service, cut, cx, 256);
    assert_eq!(undone.binding().root(), fixture.selection.binding().root());
    assert!(owner.descriptor(&token).is_none());
    composer.read_with(cx, |composer, _| assert_eq!(composer.last_error(), None));
}

#[gpui::test]
fn private_cut_noncommit_restores_candidate_then_retirement_releases_source(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = source(191);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let (composer, cx) = mount(
        cx,
        &fixture,
        1024,
        Box::new(move |_, metadata, _| {
            captured.lock().unwrap().push(metadata.unwrap().to_owned());
            ClipboardWriteOutcome::Written
        }),
    );
    fixture.service.test_cancel_next_mutation_commit();
    cx.simulate_keystrokes("ctrl-x");
    drive_owner(cx, 64);
    let owner = fixture.service.private_clipboard_owner().unwrap();
    let token = writes.lock().unwrap()[0].clone();
    let descriptor = owner.descriptor(&token).unwrap();
    assert_eq!(
        descriptor.origin.binding().root(),
        fixture.selection.binding().root()
    );
    assert!(
        fixture
            .storage
            .draft_private_clipboard_source_is_current(&fixture.store, descriptor.source)
            .unwrap()
    );
    cx.update(|window, app| {
        composer.update(app, |composer, cx| {
            composer.begin_widget_release_fence(window, cx).unwrap()
        })
    });
    assert!(owner.descriptor(&token).is_none());
    owner.retire();
    assert!(owner.is_retired());
    assert!(owner.descriptor(&token).is_none());
}
