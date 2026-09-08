use super::*;

#[gpui::test]
fn cancelled_mounted_staging_dispatch_drains_before_the_next_edit(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let MountedMarkerFixture {
        directory: _directory,
        store,
        service,
        marker_seals,
        image_asset,
        ..
    } = mounted_marker_fixture("mounted-staging-cancel", 71, 72, 73, b"cancelled-marker");
    let settlements = RangeSettlementCoordinator::new(256).unwrap();
    let widget_settlements = settlements.clone();
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(move |selection| {
                    let mut config = widget_config(
                        selection.binding().range_binding(),
                        selection.binding().presentation_generation(),
                    );
                    config.settlement_coordinator = widget_settlements.clone();
                    MainWindowConversationComposerConfig::new(selection, config)
                        .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let composer = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.focus(window);
            input.replace_text_in_range(None, "a", window, input_cx);
        })
    });
    drive_until_quiescent(cx, &composer);
    let before = service.selected_identity().unwrap();
    let before_selection = input.read_with(cx, |input, _| input.surface().unwrap().selection());
    assert_eq!(settlements.retained_count(), 0);
    let gate = service.test_block_next_selected_dispatch();
    let key = composer
        .update(cx, |composer, composer_cx| {
            composer.insert_authenticated_image_marker(
                ComposerHostImageMarkerMetadata::new(InlineObjectId::new(0x7171), image_asset),
                InlineObjectOrder::new(1),
                composer_cx,
            )
        })
        .unwrap();
    for _ in 0..4096 {
        drive(cx, 1);
        if gate.is_blocked() {
            break;
        }
    }
    assert!(
        gate.is_blocked(),
        "admitted staging dispatch did not reach the selected dispatch gate"
    );
    assert!(composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
    let storage = syndic_storage::SyndicStorage::reacquire(&store).unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(held_session) = storage
        .draft_editor_candidate_session(
            &store,
            before.binding().candidate().draft_id(),
            before.binding().candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("held staging session is not active")
    };
    let held_operation = held_session.active_operation().unwrap();
    assert!(held_operation.is_staging());
    let mut operation_bytes = [0; 16];
    operation_bytes[8..].copy_from_slice(&key.operation().get().to_be_bytes());
    assert_eq!(held_operation.operation_id().as_bytes(), &operation_bytes);
    assert_eq!(service.selected_identity(), Some(before));
    input.update(cx, |input, input_cx| {
        assert_eq!(
            input.cancel_mutation(key, input_cx).unwrap(),
            gpui_text_input::MutationCancellation::Cancelled
        );
    });
    gate.release();
    drive_until_quiescent(cx, &composer);
    let drained = service.selected_identity().unwrap();
    assert_eq!(drained.binding().root(), before.binding().root());
    assert_eq!(drained.binding().history(), before.binding().history());
    assert_eq!(
        drained.binding().range_binding(),
        before.binding().range_binding()
    );
    assert!(
        drained.binding().candidate().session_generation()
            > before.binding().candidate().session_generation()
    );
    assert_eq!(settlements.retained_count(), 0);
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(drained_session) = storage
        .draft_editor_candidate_session(
            &store,
            drained.binding().candidate().draft_id(),
            drained.binding().candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("drained staging session is not active")
    };
    assert!(drained_session.active_operation().is_none());
    assert_eq!(
        input.read_with(cx, |input, _| input.surface().unwrap().selection()),
        before_selection
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
    drive(cx, 16);
    assert_eq!(service.selected_identity(), Some(drained));
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_text_in_range(None, "next", window, input_cx);
        })
    });
    drive_until_quiescent(cx, &composer);
    let committed = service.selected_identity().unwrap();
    assert_eq!(committed.binding().root().summary().logical_utf8_bytes(), 5);
    assert_eq!(committed.binding().root().summary().marker_count(), 0);
    assert_eq!(
        composer_support::candidate_text(storage, &store, committed.binding()),
        b"anext"
    );
    assert_eq!(settlements.retained_count(), 0);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
}
