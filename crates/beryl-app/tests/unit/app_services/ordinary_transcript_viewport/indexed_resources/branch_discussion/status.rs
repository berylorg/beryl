use super::*;
use crate::discussion_presentation::{
    DiscussionComposerContribution as Composer, DiscussionPresentationError,
    DiscussionStatus as Status,
};
use gpui::EntityInputHandler;

#[test]
fn native_discussion_mount_publishes_status_without_refresh_and_preserves_editor_through_gate_changes()
 {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| fixture::seed(path, SOURCE, true),
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                eprintln!("discussion mount scenario: waiting for parent transcript");
                ready(window, &panel, cx, &["branch prefix"]).await;
                eprintln!("discussion mount scenario: parent transcript ready");
                let parent = window
                    .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
                    .unwrap();
                assert!(
                    window
                        .read_with(cx, |root, app| root
                            .test_discussion_presentation(app)
                            .unwrap()
                            .0
                            .status()
                            .is_none())
                        .unwrap()
                );
                window
                    .update(cx, |root, _, _| root.test_suspend_discussion_refresh(true))
                    .unwrap();
                eprintln!("discussion mount scenario: invoking branch command");
                invoke(window, &panel, cx).await;
                eprintln!(
                    "discussion mount scenario: branch command invoked; waiting for publication"
                );
                wait_until(window, cx, |cx| window.read_with(cx, |root, app| root.coherent_viewed_thread(app).is_some_and(|thread| thread != parent) && !root.test_running_thread_activation_pending()).unwrap(), "discussion status must arrive in the ordinary activation election without a later poll").await;
                let (_, gated, enabled, editor, fresh) = window
                    .read_with(cx, |root, app| {
                        root.test_discussion_presentation(app).unwrap()
                    })
                    .unwrap();
                eprintln!("discussion mount scenario: discussion published");
                assert!(!gated && enabled && fresh);
                let resident = window
                    .read_with(cx, |root, app| root.test_submission_composer(app).unwrap())
                    .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |composer, app| {
                                composer
                                    .gpui_input()
                                    .read(app)
                                    .is_surface_current_and_interactive()
                                    && !composer.test_has_active_flight()
                            })
                            .unwrap()
                    },
                    "the selected discussion editor must finish its prepared realization",
                )
                .await;
                with_native(window, cx, |native, app| {
                    resident.read(app).gpui_input().update(app, |input, cx| {
                        input.focus(native);
                        input.replace_text_in_range(None, "retained discussion draft", native, cx);
                    })
                })
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |composer, app| {
                                composer
                                    .selection_identity()
                                    .binding()
                                    .logical_extent()
                                    .logical_utf8_bytes()
                                    == "retained discussion draft".len() as u64
                                    && composer.gpui_input().read(app).is_semantically_quiescent()
                                    && composer
                                        .gpui_input()
                                        .read(app)
                                        .is_surface_current_and_interactive()
                                    && !composer.test_has_active_flight()
                            })
                            .unwrap()
                    },
                    "the retained discussion draft must settle before the gate transitions",
                )
                .await;
                let target = window
                    .read_with(cx, |root, app| {
                        root.test_discussion_observation_target(app).unwrap()
                    })
                    .unwrap();
                let reader = target.3.clone();
                let thread = target.1.claim().thread_id();
                let original = cx
                    .background_executor()
                    .spawn(async move { reader.observe(thread).unwrap() })
                    .await;
                window
                    .update(cx, |root, native, app| {
                        root.test_apply_discussion_observation(
                            target.clone(),
                            Ok(original.clone()),
                            native,
                            app,
                        )
                    })
                    .unwrap();
                let retained = resident
                    .read_with(cx, |composer, app| composer.surface_snapshot(app).unwrap())
                    .unwrap();
                for (status, contribution, expected_gate, expected_enabled) in [
                    (Status::ResolutionPending, Composer::Inert, true, false),
                    (Status::HandingOff, Composer::Inert, true, false),
                    (Status::HandoffFailed, Composer::Inert, true, false),
                    (Status::HandoffFailed, Composer::Writable, false, true),
                    (Status::Unavailable, Composer::Inert, true, false),
                    (Status::Archived, Composer::ReadOnly, true, true),
                    (Status::Open, Composer::Writable, false, true),
                ] {
                    eprintln!("discussion mount scenario: applying {status:?}/{contribution:?}");
                    let snapshot = original.test_with_presentation(status, contribution, None);
                    window
                        .update(cx, |root, native, app| {
                            root.test_apply_discussion_observation(
                                target.clone(),
                                Ok(snapshot),
                                native,
                                app,
                            );
                            let (snapshot, gated, enabled, current, fresh) =
                                root.test_discussion_presentation(app).unwrap();
                            assert_eq!(snapshot.status(), Some(status));
                            assert_eq!(snapshot.composer(), contribution);
                            assert_eq!(
                                (gated, enabled, current, fresh),
                                (expected_gate, expected_enabled, editor, true)
                            );
                            resident.update(app, |composer, cx| {
                                composer.begin_widget_release_fence(native, cx).unwrap();
                                assert!(!composer.gpui_input().read(cx).is_enabled());
                                composer
                                    .resume_after_widget_release_fence(native, cx)
                                    .unwrap();
                                assert_eq!(
                                    composer.gpui_input().read(cx).is_enabled(),
                                    expected_enabled
                                );
                            });
                            if expected_gate {
                                resident.read(app).gpui_input().update(app, |input, cx| {
                                    input.replace_text_in_range(None, "forbidden edit", native, cx)
                                });
                            }
                            let current = resident.read(app).surface_snapshot(app).unwrap();
                            assert_eq!(current.binding, retained.binding);
                            assert_eq!(current.source_selection, retained.source_selection);
                            assert_eq!(current.scroll_block, retained.scroll_block);
                        })
                        .unwrap();
                }
                window
                    .update(cx, |root, native, app| {
                        eprintln!("discussion mount scenario: notice and stale-delivery checks");
                        let failure = original.test_with_presentation(Status::Unavailable, Composer::Inert, Some("The exact handoff is unavailable. The original resolution and parent binding are retained."));
                        root.test_apply_discussion_observation(target.clone(), Ok(failure.clone()), native, app);
                        let token = root.test_discussion_notice_token().expect("persistent discussion condition");
                        root.test_apply_discussion_observation(target.clone(), Ok(failure), native, app);
                        assert_eq!(root.test_discussion_notice_token(), Some(token));
                        root.test_apply_discussion_observation(target.clone(), Ok(original.clone()), native, app);
                        assert!(root.test_discussion_notice_token().is_none());
                        root.test_apply_discussion_observation(
                            target.clone(),
                            Err(DiscussionPresentationError::Changed),
                            native,
                            app,
                        );
                        let (snapshot, gated, enabled, current, fresh) =
                            root.test_discussion_presentation(app).unwrap();
                        assert_eq!(snapshot.status(), Some(Status::Open));
                        assert!(gated && enabled && !fresh);
                        assert_eq!(current, editor);
                        root.test_apply_discussion_observation(
                            target.clone(),
                            Ok(original.clone()),
                            native,
                            app,
                        );
                        assert!(!root.test_discussion_presentation(app).unwrap().1);
                        let mut stale = target.clone();
                        stale.2 = std::sync::Arc::new(());
                        root.test_apply_discussion_observation(
                            stale,
                            Ok(original.test_with_presentation(
                                Status::Archived,
                                Composer::ReadOnly,
                                None,
                            )),
                            native,
                            app,
                        );
                        assert_eq!(
                            root.test_discussion_presentation(app).unwrap().0.status(),
                            Some(Status::Open)
                        );
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    native.draw(app).clear();
                })
                .unwrap();
                window
                    .update(cx, |root, _, _| root.test_suspend_discussion_refresh(false))
                    .unwrap();
                eprintln!("discussion mount scenario: retiring fixture");
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
