use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[gpui::test]
fn current_ordinary_intake_is_noop_without_attention_or_history(cx: &mut gpui::TestAppContext) {
    let mounted = mount(cx, 161);
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    let result = Rc::new(RefCell::new(None));
    let observed = result.clone();
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.request_ordinary_thread_activation(
                prior.claim().thread_id(),
                window,
                cx,
                move |value, _, _| *observed.borrow_mut() = Some(value),
            )
            .unwrap();
            assert!(
                root.request_ordinary_thread_activation(
                    prior.claim().thread_id(),
                    window,
                    cx,
                    |_, _, _| panic!("duplicate callback admitted")
                )
                .is_err()
            );
        })
        .unwrap();
    drive(cx, |_| result.borrow().is_some());
    assert_eq!(
        result.borrow().as_ref().unwrap(),
        &Ok(OrdinaryThreadActivationAcceptance::Current)
    );
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(root.cached_running_selection(app).unwrap().0, prior);
            assert!(!root.running_threads.has_activation_custody());
            assert!(
                root.running_threads
                    .navigation_history
                    .test_entries()
                    .is_empty()
            );
        })
        .unwrap();
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    finish(mounted, source, cx);
}

#[gpui::test]
fn elsewhere_ordinary_intake_refuses_without_reveal_or_attention(cx: &mut gpui::TestAppContext) {
    let mounted = mount(cx, 171);
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let other = support::mount_second(&mounted, cx);
    let target = other
        .read_with(cx, |root, app| {
            root.cached_running_selection(app)
                .unwrap()
                .0
                .claim()
                .thread_id()
        })
        .unwrap();
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    let result = Rc::new(RefCell::new(None));
    let observed = result.clone();
    mounted
        .window
        .update(cx, |root, window, cx| {
            window.activate_window();
            root.request_ordinary_thread_activation(target, window, cx, move |value, _, _| {
                *observed.borrow_mut() = Some(value)
            })
            .unwrap();
        })
        .unwrap();
    drive(cx, |_| result.borrow().is_some());
    assert!(
        result
            .borrow()
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("another window")
    );
    assert!(cx.update(|app| app.active_window()) == Some(mounted.window.into()));
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(root.cached_running_selection(app).unwrap().0, prior);
            assert!(
                root.running_threads
                    .navigation_history
                    .test_entries()
                    .is_empty()
            );
        })
        .unwrap();
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    finish(mounted, source, cx);
}

#[gpui::test]
fn idle_ordinary_intake_activates_without_running_membership(cx: &mut gpui::TestAppContext) {
    activate_idle(cx, 181, false, false);
}

#[gpui::test]
fn dirty_ordinary_intake_saves_original_prior_before_idle_target(cx: &mut gpui::TestAppContext) {
    activate_idle(cx, 191, true, false);
}

#[gpui::test]
fn mounted_navigation_missing_exact_target_stays_visible_and_refuses_all_controls(
    cx: &mut gpui::TestAppContext,
) {
    refuse_navigation(cx, false);
}

#[gpui::test]
fn mounted_navigation_elsewhere_exact_target_stays_visible_without_reveal_or_attention(
    cx: &mut gpui::TestAppContext,
) {
    refuse_navigation(cx, true);
}

fn refuse_navigation(cx: &mut gpui::TestAppContext, elsewhere: bool) {
    let mounted = mount(cx, if elsewhere { 211 } else { 221 });
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let other = elsewhere.then(|| support::mount_second(&mounted, cx));
    let target = other
        .map(|window| {
            window
                .read_with(cx, |root, app| {
                    root.cached_running_selection(app)
                        .unwrap()
                        .0
                        .claim()
                        .thread_id()
                })
                .unwrap()
        })
        .unwrap_or_else(|| SyndicThreadId::from_bytes([233; 16]));
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    mounted
        .window
        .update(cx, |root, window, _| {
            root.running_threads
                .navigation_history
                .begin(Some(target), prior.claim().thread_id());
            root.running_threads
                .navigation_history
                .settle(Some(prior.claim().thread_id()));
            window.activate_window();
        })
        .unwrap();
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, app| {
                root.test_thread_navigation_reason(false, app)
                    .is_some_and(|reason| {
                        reason.contains(if elsewhere {
                            "another window"
                        } else {
                            "unavailable"
                        })
                    })
            })
            .unwrap()
    });
    let before = mounted
        .window
        .read_with(cx, |root, _| root.test_thread_navigation_history())
        .unwrap();
    let other_before = other.map(|window| {
        window
            .read_with(cx, |root, app| {
                root.cached_running_selection(app).unwrap().0
            })
            .unwrap()
    });
    {
        let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
        let back = visual.debug_bounds("main-window-thread-back").unwrap();
        assert!(visual.debug_bounds("main-window-thread-forward").is_some());
        visual.simulate_click(back.center(), gpui::Modifiers::none());
    }
    mounted
        .window
        .update(cx, |root, window, _| {
            root.test_thread_navigation_focus(false).focus(window)
        })
        .unwrap();
    gpui::VisualTestContext::from_window(mounted.window.into(), cx)
        .simulate_keystrokes("enter space");
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.navigate_thread_history(false, window, cx)
        })
        .unwrap();
    support::draw(cx);
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(root.cached_running_selection(app).unwrap().0, prior);
            assert_eq!(root.test_thread_navigation_history(), before);
            assert!(!root.running_threads.has_activation_custody());
        })
        .unwrap();
    if let (Some(other), Some(before)) = (other, other_before) {
        assert_eq!(
            other
                .read_with(cx, |root, app| root
                    .cached_running_selection(app)
                    .unwrap()
                    .0)
                .unwrap(),
            before
        );
    }
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    assert!(cx.update(|app| app.active_window()) == Some(mounted.window.into()));
    finish(mounted, source, cx);
}

#[gpui::test]
fn mounted_navigation_pointer_enter_space_round_trips_exact_original_views(
    cx: &mut gpui::TestAppContext,
) {
    activate_idle(cx, 201, false, true);
}

fn activate_idle(cx: &mut gpui::TestAppContext, seed: u8, dirty: bool, navigation: bool) {
    let mounted = mount(cx, seed);
    if dirty {
        mounted
            .window
            .update(cx, |root, window, cx| {
                root.notice_safe_focus(cx).focus(window)
            })
            .unwrap();
        for (index, character) in "original prior".chars().enumerate() {
            gpui::VisualTestContext::from_window(mounted.window.into(), cx)
                .simulate_input(&character.to_string());
            drive(cx, |cx| {
                mounted
                    .window
                    .read_with(cx, |root, app| {
                        root.cached_running_selection(app)
                            .unwrap()
                            .0
                            .binding()
                            .logical_extent()
                            .logical_utf8_bytes()
                            == (index + 1) as u64
                    })
                    .unwrap()
            });
        }
    }
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let target = SyndicThreadId::from_bytes([seed.wrapping_add(20); 16]);
    let store = mounted.fixture.store.clone();
    let storage = mounted.fixture.storage.clone();
    let execution = mounted.fixture.execution();
    let reader = source.reader.clone();
    home_support::join(
        home_support::worker(move || {
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.create_thread(
                    storage.revision(&store).unwrap(),
                    CreateThread::ordinary(
                        target,
                        SyndicDraftId::from_bytes([seed.wrapping_add(21); 16]),
                        execution,
                        SyndicTimestamp::from_unix_millis(10_000),
                        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                    ),
                ))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            let query = beryl_state::CatalogNormalizedQuery::new("").unwrap();
            let page = reader
                .query(&query, 0, &ProjectionCancellationToken::new())
                .unwrap();
            assert!(
                page.records()
                    .iter()
                    .all(|record| record.thread_id != target)
            );
        }),
        cx,
    );
    let lease = mounted
        .fixture
        .process
        .admit_selection(&[prior.window_id()], prior.window_id())
        .unwrap();
    let result = Rc::new(RefCell::new(None));
    let observed = result.clone();
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.request_ordinary_thread_activation(target, window, cx, move |value, _, _| {
                *observed.borrow_mut() = Some(value)
            })
            .unwrap();
            root.running_threads.selection_lease = Some(Arc::new(lease));
        })
        .unwrap();
    drive(cx, |_| result.borrow().is_some());
    assert_eq!(
        result.borrow().as_ref().unwrap(),
        &Ok(OrdinaryThreadActivationAcceptance::Admitted)
    );
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| !root.running_threads.has_activation_custody())
            .unwrap()
    });
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(
                root.cached_running_selection(app)
                    .unwrap()
                    .0
                    .claim()
                    .thread_id(),
                target
            );
            assert_eq!(
                root.running_threads.transcript_claim.unwrap().thread_id(),
                target
            );
            let (claim, title) = root.coherent_selected_thread_title(app).unwrap();
            assert_eq!(claim.thread_id(), target);
            assert_eq!(title.source(), beryl_state::CatalogTitleSource::Absent);
            assert_eq!(
                root.running_threads.navigation_history.test_entries(),
                vec![prior.claim().thread_id(), target]
            );
            assert!(root.running_threads.failure.is_none());
        })
        .unwrap();
    if dirty {
        let store = mounted.fixture.store.clone();
        let storage = mounted.fixture.storage.clone();
        home_support::join(
            home_support::worker(move || {
                let draft = storage
                    .current_draft(
                        &store,
                        prior.claim().thread_id(),
                        SyndicPointReadLimit::new(65_536).unwrap(),
                    )
                    .unwrap()
                    .unwrap();
                assert_eq!(draft.root().reference(), prior.binding().root());
                assert!(
                    draft
                        .draft()
                        .history()
                        .key()
                        .publication_operation_id()
                        .is_some()
                );
            }),
            cx,
        );
    }
    if navigation {
        let third = SyndicThreadId::from_bytes([seed.wrapping_add(30); 16]);
        let store = mounted.fixture.store.clone();
        let storage = mounted.fixture.storage.clone();
        let execution = mounted.fixture.execution();
        home_support::join(
            home_support::worker(move || {
                let mut command = HomeCommand::new(store.home_revision().unwrap());
                command
                    .add(storage.create_thread(
                        storage.revision(&store).unwrap(),
                        CreateThread::ordinary(
                            third,
                            SyndicDraftId::from_bytes([seed.wrapping_add(31); 16]),
                            execution,
                            SyndicTimestamp::from_unix_millis(11_000),
                            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                        ),
                    ))
                    .unwrap();
                assert!(matches!(
                    store.execute(command),
                    CommandOutcome::Committed {
                        later_failure: None,
                        ..
                    }
                ));
            }),
            cx,
        );
        let invoking = mounted
            .window
            .read_with(cx, |root, _| root.controller().unwrap().window_id())
            .unwrap();
        let lease = mounted
            .fixture
            .process
            .admit_selection(&[invoking], invoking)
            .unwrap();
        mounted
            .window
            .update(cx, |root, window, cx| {
                root.request_ordinary_thread_activation(third, window, cx, |result, _, _| {
                    assert_eq!(result, Ok(OrdinaryThreadActivationAcceptance::Admitted))
                })
                .unwrap();
                root.running_threads.selection_lease = Some(Arc::new(lease));
            })
            .unwrap();
        drive(cx, |cx| {
            mounted
                .window
                .read_with(cx, |root, app| {
                    !root.running_threads.has_activation_custody()
                        && root
                            .cached_running_selection(app)
                            .unwrap()
                            .0
                            .claim()
                            .thread_id()
                            == third
                })
                .unwrap()
        });
        let focus = mounted
            .window
            .read_with(cx, |root, _| {
                (
                    root.test_thread_navigation_focus(false),
                    root.test_thread_navigation_focus(true),
                )
            })
            .unwrap();
        assert_ne!(focus.0, focus.1);
        for (index, (forward, expected)) in [
            (false, target),
            (false, prior.claim().thread_id()),
            (true, target),
            (true, third),
        ]
        .into_iter()
        .enumerate()
        {
            drive(cx, |cx| {
                mounted
                    .window
                    .read_with(cx, |root, app| {
                        root.test_thread_navigation_reason(forward, app).is_none()
                    })
                    .unwrap()
            });
            support::draw(cx);
            let invoking = mounted
                .window
                .read_with(cx, |root, _| root.controller().unwrap().window_id())
                .unwrap();
            let lease = mounted
                .fixture
                .process
                .admit_selection(&[invoking], invoking)
                .unwrap();
            mounted
                .window
                .update(cx, |root, _, app| {
                    assert!(root.running_threads.fixture_selection_lease.is_none());
                    assert!(!root.running_threads.has_activation_custody());
                    root.running_threads.fixture_selection_lease =
                        Some((expected, Arc::new(lease)));
                    assert!(root.test_thread_navigation_reason(forward, app).is_none());
                })
                .unwrap();
            if index == 0 {
                let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
                let back = visual.debug_bounds("main-window-thread-back").unwrap();
                let next = visual.debug_bounds("main-window-thread-forward").unwrap();
                assert_eq!(back.size, gpui::size(px(32.), px(32.)));
                assert_eq!(next.size, back.size);
                assert!(back.right() <= next.left());
                visual.simulate_click(back.center(), gpui::Modifiers::none());
            } else {
                mounted
                    .window
                    .update(cx, |root, window, _| {
                        root.test_thread_navigation_focus(forward).focus(window)
                    })
                    .unwrap();
                gpui::VisualTestContext::from_window(mounted.window.into(), cx)
                    .simulate_keystrokes(if index == 2 { "enter" } else { "space" });
            }
            let diagnostic_at = std::time::Instant::now() + Duration::from_secs(3);
            let mut reported = false;
            drive(cx, |cx| {
                if !reported && std::time::Instant::now() >= diagnostic_at {
                    reported = true;
                    let facts = mounted
                        .window
                        .update(cx, |root, window, app| {
                            (
                                root.cached_running_selection(app)
                                    .map(|(selection, _)| selection.claim().thread_id()),
                                root.running_threads.pending_activation,
                                root.running_threads.activation_operation.is_some(),
                                root.running_threads.activation_task.is_some(),
                                root.running_threads.selection_lease.is_some(),
                                root.running_threads.fixture_selection_lease.is_some(),
                                root.running_threads.navigation_history.target(false),
                                root.running_threads.navigation_history.target(true),
                                root.test_thread_navigation_reason(forward, app),
                                root.test_thread_navigation_focus(forward)
                                    .is_focused(window),
                                root.running_threads.failure.clone(),
                            )
                        })
                        .unwrap();
                    eprintln!(
                        "navigation input iteration={index} forward={forward} expected={expected:?} facts={facts:?}"
                    );
                }
                mounted
                    .window
                    .read_with(cx, |root, app| {
                        !root.running_threads.has_activation_custody()
                            && root
                                .cached_running_selection(app)
                                .unwrap()
                                .0
                                .claim()
                                .thread_id()
                                == expected
                    })
                    .unwrap()
            });
            mounted
                .window
                .read_with(cx, |root, app| {
                    let selected = expected;
                    assert_eq!(
                        root.running_threads.transcript_claim.unwrap().thread_id(),
                        selected
                    );
                    assert_eq!(
                        root.coherent_selected_thread_title(app)
                            .unwrap()
                            .0
                            .thread_id(),
                        selected
                    );
                    assert_eq!(
                        root.test_thread_navigation_history(),
                        vec![prior.claim().thread_id(), target, third]
                    );
                    assert!(root.running_threads.fixture_selection_lease.is_none());
                })
                .unwrap();
        }
    }
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    finish(mounted, source, cx);
}
