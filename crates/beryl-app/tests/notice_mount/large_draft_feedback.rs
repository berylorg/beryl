use super::*;
use beryl_home_store::test_faults::FaultPoint;
use composer_feedback::{
    asset, composer, configured_large_mount, editor_state, insert, notice, prepare_editor,
    request_marker,
};
use gpui::EntityInputHandler;
use syndic_storage::DraftMarkerAdmissionLimitsV1;

const DRAFT_BYTES: u64 = large_draft_support::LARGE_DRAFT_BYTES;

#[track_caller]
fn drive_until(
    cx: &mut gpui::TestAppContext,
    mut ready: impl FnMut(&mut gpui::TestAppContext) -> bool,
) {
    for _ in 0..4096 {
        support::draw(cx);
        if ready(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("large draft did not reach its bounded milestone");
}

fn budgets(input: &gpui::Entity<gpui_text_input::RangeTextInput>, cx: &gpui::TestAppContext) {
    let d = input.read_with(cx, |input, _| input.realization_diagnostics());
    assert!(d.high_water.owned_bytes <= d.max_surface_bytes, "{d:?}");
    assert!(d.high_water.owned_items <= d.max_surface_items, "{d:?}");
    assert!(d.high_water.resident_pages <= d.max_owned_pages, "{d:?}");
    assert!(d.geometry_high_water_bytes <= d.max_geometry_bytes, "{d:?}");
    assert!(d.geometry_high_water_items <= d.max_geometry_items, "{d:?}");
    assert!(
        d.high_water.resident_page_bytes < DRAFT_BYTES as usize,
        "{d:?}"
    );
}

fn interactive(
    input: &gpui::Entity<gpui_text_input::RangeTextInput>,
    cx: &mut gpui::TestAppContext,
) {
    drive_until(cx, |cx| {
        input.read_with(cx, |input, _| {
            input.is_semantically_quiescent() && input.is_surface_current_and_interactive()
        })
    });
}

#[gpui::test]
fn large_draft_size_refusal_preserves_editor_and_history(cx: &mut gpui::TestAppContext) {
    exercise(
        cx,
        171,
        DraftMarkerAdmissionLimitsV1::new(64, 0, u64::MAX),
        MainWindowComposerMutationFeedbackKind::OperationTooLarge,
    );
}

#[gpui::test]
fn large_draft_capacity_refusal_preserves_editor_and_history(cx: &mut gpui::TestAppContext) {
    exercise(
        cx,
        181,
        DraftMarkerAdmissionLimitsV1::new(0, u64::MAX, u64::MAX),
        MainWindowComposerMutationFeedbackKind::CapacityUnavailable,
    );
}

#[gpui::test]
fn large_draft_storage_refusal_preserves_editor(cx: &mut gpui::TestAppContext) {
    exercise(
        cx,
        191,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        MainWindowComposerMutationFeedbackKind::Storage,
    );
}

#[gpui::test]
fn large_draft_ambiguous_refusal_preserves_exact_custody(cx: &mut gpui::TestAppContext) {
    exercise(
        cx,
        201,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable,
    );
}

fn exercise(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
    kind: MainWindowComposerMutationFeedbackKind,
) {
    let (mounted, service) = configured_large_mount(
        cx,
        seed,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
    );
    let composer = composer(&mounted, cx);
    eprintln!(
        "large refusal fixture: {}",
        mounted.fixture.directory.path().display()
    );
    prepare_editor(&mounted, &composer, cx);
    eprintln!("large refusal: editor ready");
    let asset = asset(&mounted, cx);
    request_marker(&mounted, &composer, asset, 1, cx);
    drive_until(cx, |_| {
        service
            .selected_identity()
            .unwrap()
            .binding()
            .root()
            .summary()
            .marker_count()
            == 1
    });
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    eprintln!("large refusal: existing marker committed");
    drive_until(cx, |cx| {
        input.read_with(cx, |input, _| input.is_surface_current_and_interactive())
    });
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-end");
    drive_until(cx, |cx| {
        composer.read_with(cx, |composer, app| {
            composer
                .surface_snapshot(app)
                .is_some_and(|s| s.source_selection.head.byte_offset.get() == DRAFT_BYTES)
        })
    });
    interactive(&input, cx);
    request_marker(&mounted, &composer, asset, 2, cx);
    drive_until(cx, |_| {
        service
            .selected_identity()
            .unwrap()
            .binding()
            .root()
            .summary()
            .marker_count()
            == 2
    });
    interactive(&input, cx);
    cx.simulate_keystrokes(mounted.window.into(), "shift-left");
    drive_until(cx, |cx| {
        composer.read_with(cx, |composer, app| {
            composer
                .surface_snapshot(app)
                .is_some_and(|s| s.source_selection.anchor != s.source_selection.head)
        })
    });
    let before = editor_state(&composer, cx);
    eprintln!("large refusal: tail selection ready");
    assert_eq!(
        before.selection.binding().root().summary().marker_count(),
        2
    );
    assert_eq!(
        before.caret_and_selection.anchor.byte_offset.get(),
        DRAFT_BYTES
    );
    assert_eq!(
        before.caret_and_selection.head.byte_offset.get(),
        DRAFT_BYTES
    );
    let neighbor = gpui_text_input::InlineObjectNeighbor::new(
        gpui_text_input::InlineObjectId::new(2),
        gpui_text_input::InlineObjectOrder::new(2),
    );
    assert_eq!(
        before.caret_and_selection.anchor.gap,
        gpui_text_input::InlineObjectGap::after(neighbor)
    );
    assert_eq!(
        before.caret_and_selection.head.gap,
        gpui_text_input::InlineObjectGap::before(neighbor)
    );
    budgets(&input, cx);
    service
        .test_with_selected_host(|host| host.test_set_mutation_admission_retained_limits(limits));
    if kind == MainWindowComposerMutationFeedbackKind::Storage {
        mounted.fixture.faults.fail_next(FaultPoint::BeforeCommit);
    } else if kind == MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable {
        mounted.fixture.faults.fail_next(FaultPoint::AfterPersist);
    }
    let repeats = if matches!(
        kind,
        MainWindowComposerMutationFeedbackKind::OperationTooLarge
            | MainWindowComposerMutationFeedbackKind::CapacityUnavailable
    ) {
        4
    } else {
        1
    };
    let ingress = support::ingress(mounted.window, cx);
    let mut previous = None;
    for attempt in 0..repeats {
        interactive(&input, cx);
        insert(&mounted, &composer, asset, 3 + attempt, cx);
        assert_eq!(editor_state(&composer, cx), before);
        let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
        assert_eq!(feedback.kind, kind);
        assert_eq!(feedback.selection, before.selection);
        if let Some(key) = previous {
            assert_ne!(feedback.key, key);
        }
        previous = Some(feedback.key);
        let (token, content) = notice(&mounted, cx);
        assert_eq!(content.commands().count(), 0);
        if kind == MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable {
            assert_eq!(content.dismissal, NoticeDismissal::Persistent);
            service.test_with_selected_host(|host| {
                assert_eq!(host.binding(), Some(before.selection.binding()));
                assert_eq!(host.settlement_custody_in_use(), 1);
            });
            mounted
                .window
                .update(cx, |_, window, app| {
                    composer.update(app, |composer, cx| {
                        assert!(composer.retry_mutation_admission(window, cx).is_err());
                    })
                })
                .unwrap();
            assert!(
                cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token), app))
                    .is_err()
            );
        } else {
            assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
            cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token), app))
                .unwrap();
            for _ in 0..3 {
                support::draw(cx);
            }
            assert!(projection(mounted.window, cx).is_none());
            service.test_with_selected_host(|host| {
                assert_eq!(host.binding(), Some(before.selection.binding()));
                assert_eq!(host.settlement_custody_in_use(), 0);
                assert!(host.test_last_settlement_identity_custody_count() <= 3);
            });
        }
        budgets(&input, cx);
        eprintln!("large refusal: attempt {attempt} complete");
    }
    if matches!(
        kind,
        MainWindowComposerMutationFeedbackKind::OperationTooLarge
            | MainWindowComposerMutationFeedbackKind::CapacityUnavailable
    ) {
        if kind == MainWindowComposerMutationFeedbackKind::CapacityUnavailable {
            service.test_with_selected_host(|host| {
                host.test_set_mutation_admission_retained_limits(
                    DraftMarkerAdmissionLimitsV1::PRODUCTION,
                )
            });
        }
        eprintln!("large refusal: browsing origin");
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-home");
        drive_until(cx, |cx| {
            composer.read_with(cx, |composer, app| {
                composer
                    .surface_snapshot(app)
                    .is_some_and(|s| s.source_selection.head.byte_offset.get() == 0)
            })
        });
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-end");
        eprintln!("large refusal: browsing EOF");
        drive_until(cx, |cx| {
            composer.read_with(cx, |composer, app| {
                composer
                    .surface_snapshot(app)
                    .is_some_and(|s| s.source_selection.head.byte_offset.get() == DRAFT_BYTES)
            })
        });
        interactive(&input, cx);
        eprintln!("large refusal: smaller edit");
        mounted
            .window
            .update(cx, |_, window, app| {
                input.update(app, |input, cx| {
                    input.replace_text_in_range(None, "!", window, cx);
                })
            })
            .unwrap();
        drive_until(cx, |_| {
            service.selected_identity().unwrap().binding().root()
                != before.selection.binding().root()
        });
        let edited = service.selected_identity().unwrap().binding().root();
        assert_eq!(edited.summary().marker_count(), 2);
        assert_eq!(
            edited.summary().logical_extent().logical_utf8_bytes(),
            DRAFT_BYTES + 1
        );
        interactive(&input, cx);
        eprintln!("large refusal: undo");
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-z");
        drive_until(cx, |_| {
            service.selected_identity().unwrap().binding().root()
                == before.selection.binding().root()
        });
        interactive(&input, cx);
        eprintln!("large refusal: redo");
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-y");
        drive_until(cx, |_| {
            service.selected_identity().unwrap().binding().root() == edited
        });
        budgets(&input, cx);
    }
    mounted
        .window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    if kind == MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable {
        service.test_with_selected_host(|host| assert_eq!(host.settlement_custody_in_use(), 1));
        assert_eq!(editor_state(&composer, cx), before);
        assert_eq!(
            composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap().key),
            previous.unwrap()
        );
    }
    let weak_composer = composer.downgrade();
    let weak_input = input.downgrade();
    let weak_service = Arc::downgrade(&service);
    drop((input, composer, service));
    support::finish(mounted, cx);
    assert!(weak_composer.upgrade().is_none());
    assert!(weak_input.upgrade().is_none());
    assert!(weak_service.upgrade().is_none());
}
