use super::*;

#[test]
fn native_selected_child_publishes_exact_readonly_endpoint_while_original_head_worker_is_held() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        lineage_fixture::prepare_taller_child_home,
        final_selection,
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let original = composer(window, cx).await;
                let original_height = original
                    .read_with(cx, |composer, app| {
                        composer
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .content_height()
                    })
                    .unwrap();
                window
                    .read_with(cx, |root, _| {
                        assert!(root.test_thread_lineage_view().is_none())
                    })
                    .unwrap();
                let gate = window
                    .update(cx, |root, _, _| root.test_hold_lineage_head())
                    .unwrap();
                let child = lineage_fixture::child();
                let (picker, key) = switcher_entry::open(window, child, cx).await;
                window
                    .update(cx, |_, _, app| {
                        picker.update(app, |picker, cx| picker.activate(&key, cx))
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(20);
                let widget = loop {
                    if let Some(widget) = window.update(cx, |root, window, app| {
                    let (claim, title) = root.coherent_selected_thread_title(app)?;
                    if claim.thread_id() != child || !gate.entered() { return None; }
                    let widget = root.test_thread_lineage_view()?;
                    let view = widget.read(app);
                    if !view.diagnostics(window, widget.entity_id().as_u64()).current_endpoint_present { return None; }
                    assert_eq!(view.query().selected, claim.thread_id());
                    assert_eq!(view.test_current_title(), title.text().unwrap_or("Untitled"));
                    assert_eq!(root.test_thread_confirmation_visible_transcript_claim(), Some(claim));
                    let transcript = root.test_running_transcript_snapshot(app);
                    assert!(matches!(transcript.state, crate::syndic_transcript::ResidentTranscriptSnapshotState::ProviderBacked { .. }), "canonical inherited child transcript was not prepared: state={:?}, records={}; {}", transcript.state, transcript.record_count(), root.test_lineage_readiness(app));
                    assert!(view.test_inert());
                    assert!(view.test_parent_input(SyndicThreadId::from_bytes([242;16]), window).is_none());
                    Some(widget.clone())
                }).unwrap() { break widget; }
                    assert!(
                        Instant::now() < deadline,
                        "coherent selected-child publication failed to mount readonly endpoint before held head completion"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                };
                gate.release();
                let selected_child = composer(window, cx).await;
                let child_height = selected_child
                    .read_with(cx, |composer, app| {
                        assert_eq!(
                            composer
                                .selection_identity()
                                .binding()
                                .logical_extent()
                                .logical_utf8_bytes(),
                            1536
                        );
                        composer
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .content_height()
                    })
                    .unwrap();
                assert!(
                    child_height > original_height,
                    "actual child target must be taller than the original editor"
                );
                let parent = SyndicThreadId::from_bytes([242; 16]);
                ready(window, parent, cx).await;
                focus(window, parent, cx).await;
                let prior_focus = widget.read_with(cx, |view, _| view.focus_proxy()).unwrap();
                let native = native(window, cx).await;
                window
                    .update(cx, |root, window, app| {
                        post_input(root, parent, Input::Enter, native, window, app)
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(20);
                let final_claim = loop {
                    if let Some(claim) = window
                        .update(cx, |root, window, app| {
                            let (claim, _) = root.coherent_selected_thread_title(app)?;
                            if claim.thread_id() != parent
                                || root.test_running_thread_activation_pending()
                            {
                                return None;
                            }
                            assert!(root.test_thread_lineage_view().is_none());
                            assert!(!prior_focus.is_focused(window));
                            assert!(root.notice_safe_focus(app).is_focused(window));
                            Some(claim)
                        })
                        .unwrap()
                    {
                        break claim;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "exact parent promotion did not retire child endpoint and focus: {:?}",
                        window
                            .update(cx, |root, window, app| (
                                root.test_lineage_readiness(app),
                                root.test_thread_confirmation_diagnostics(),
                                root.test_running_activation_failure(),
                                root.test_thread_lineage_focus(child, parent, window, app),
                            ))
                            .unwrap(),
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                };
                let id = window
                    .read_with(cx, |root, _| root.controller().unwrap().window_id())
                    .unwrap();
                let selected_parent = composer(window, cx).await;
                let parent_height = selected_parent
                    .read_with(cx, |composer, app| {
                        assert_eq!(
                            composer
                                .selection_identity()
                                .binding()
                                .logical_extent()
                                .logical_utf8_bytes(),
                            768
                        );
                        composer
                            .gpui_input()
                            .read(app)
                            .surface()
                            .unwrap()
                            .content_height()
                    })
                    .unwrap();
                assert!(
                    parent_height < child_height,
                    "actual parent target must be shorter than the child editor"
                );
                *qualified.borrow_mut() = Some((id, final_claim.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}
