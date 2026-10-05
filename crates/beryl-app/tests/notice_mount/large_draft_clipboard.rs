use super::*;
use beryl_home_store::test_faults::FaultPoint;
use composer_feedback::{
    asset, composer, configured_large_clipboard_mount, notice, prepare_editor, request_marker,
};
use gpui::{ClipboardItem, EntityInputHandler};
use gpui_text_input::{RangeHistoryFrontier, RangeSourceSelection, SourcePosition};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use syndic_storage::{
    DraftEditHistoryFrontierReferenceV1, DraftMarkerAdmissionLimitsV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1, DraftPieceRootReferenceV1,
};

const DRAFT_BYTES: u64 = large_draft_support::LARGE_DRAFT_BYTES;
const CLIPBOARD_BYTES: usize = 64 * 1024;
const MUTATION_PAGE_BYTES: usize = 4096;

#[derive(Debug, PartialEq)]
struct CachedEditor {
    root: DraftPieceRootReferenceV1,
    durable_history: DraftEditHistoryFrontierReferenceV1,
    selection: RangeSourceSelection,
    caret: SourcePosition,
    widget_history: RangeHistoryFrontier,
    realized_markers: usize,
}

fn cached_editor(
    composer: &gpui::Entity<MainWindowConversationComposer>,
    cx: &gpui::TestAppContext,
) -> CachedEditor {
    composer.read_with(cx, |composer, app| {
        let binding = composer.selection_identity().binding();
        let input = composer.gpui_input();
        let input = input.read(app);
        let surface = input.surface().unwrap();
        CachedEditor {
            root: binding.root(),
            durable_history: binding.history(),
            selection: surface.selection(),
            caret: surface.caret(),
            widget_history: input.history_frontier(),
            realized_markers: surface.realized_objects().len(),
        }
    })
}

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
    panic!("large draft clipboard did not reach its bounded milestone");
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

fn assert_origin_marker_unrealized(
    input: &gpui::Entity<gpui_text_input::RangeTextInput>,
    tail_present: bool,
    cx: &gpui::TestAppContext,
) {
    input.read_with(cx, |input, _| {
        let objects = input.surface().unwrap().realized_objects();
        assert!(
            !objects
                .iter()
                .any(|object| object.id() == gpui_text_input::InlineObjectId::new(1))
        );
        assert_eq!(
            objects
                .iter()
                .any(|object| object.id() == gpui_text_input::InlineObjectId::new(2)),
            tail_present,
        );
    });
}

fn marker_facts(
    mounted: &support::Mounted,
    service: &MainWindowConversationComposerService,
) -> Vec<syndic_storage::DraftPieceMarkerAtV1> {
    let selection = service.selected_identity().unwrap();
    let result = mounted
        .fixture
        .storage
        .candidate_draft_piece_marker_demand(
            &mounted.fixture.store,
            selection.binding().candidate(),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::InclusiveRange {
                    start: 0,
                    end: DRAFT_BYTES,
                },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                4,
                65_536,
            ),
        )
        .unwrap();
    assert!(result.value().requested_side_complete());
    result.value().markers().to_vec()
}

#[gpui::test]
fn large_draft_cut_size_refused_paste_retains_source_and_immediate_undo(
    cx: &mut gpui::TestAppContext,
) {
    exercise(
        cx,
        211,
        MainWindowComposerMutationFeedbackKind::OperationTooLarge,
    );
}

#[gpui::test]
fn large_draft_cut_capacity_refused_paste_retains_source_and_immediate_undo(
    cx: &mut gpui::TestAppContext,
) {
    exercise(
        cx,
        221,
        MainWindowComposerMutationFeedbackKind::CapacityUnavailable,
    );
}

#[gpui::test]
fn large_draft_cut_storage_refused_paste_retains_cached_editor_history_and_releases_custody(
    cx: &mut gpui::TestAppContext,
) {
    exercise(cx, 231, MainWindowComposerMutationFeedbackKind::Storage);
}

fn exercise(cx: &mut gpui::TestAppContext, seed: u8, kind: MainWindowComposerMutationFeedbackKind) {
    assert!(DRAFT_BYTES > CLIPBOARD_BYTES as u64);
    assert!(DRAFT_BYTES > MUTATION_PAGE_BYTES as u64);
    let (mounted, service) = configured_large_clipboard_mount(
        cx,
        seed,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        CLIPBOARD_BYTES,
        MUTATION_PAGE_BYTES,
    );
    let composer = composer(&mounted, cx);
    prepare_editor(&mounted, &composer, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    interactive(&input, cx);
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
    interactive(&input, cx);
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-end");
    drive_until(cx, |cx| {
        composer.read_with(cx, |composer, app| {
            composer.surface_snapshot(app).is_some_and(|surface| {
                surface.source_selection.head.byte_offset.get() == DRAFT_BYTES
            })
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
        cached_editor(&composer, cx).selection.anchor != cached_editor(&composer, cx).selection.head
    });
    interactive(&input, cx);
    let before_cut = cached_editor(&composer, cx);
    let before_markers = marker_facts(&mounted, &service);
    assert_eq!(before_markers.len(), 2);
    assert_eq!(before_markers[0].anchor(), 0);
    assert_eq!(before_markers[1].anchor(), DRAFT_BYTES);
    assert_origin_marker_unrealized(&input, true, cx);
    let neighbor = gpui_text_input::InlineObjectNeighbor::new(
        gpui_text_input::InlineObjectId::new(2),
        gpui_text_input::InlineObjectOrder::new(2),
    );
    assert_eq!(before_cut.selection.anchor.byte_offset.get(), DRAFT_BYTES);
    assert_eq!(before_cut.selection.head.byte_offset.get(), DRAFT_BYTES);
    assert_eq!(
        before_cut.selection.anchor.gap,
        gpui_text_input::InlineObjectGap::after(neighbor)
    );
    assert_eq!(
        before_cut.selection.head.gap,
        gpui_text_input::InlineObjectGap::before(neighbor)
    );
    assert_eq!(before_cut.caret, before_cut.selection.head);
    budgets(&input, cx);

    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let expected_tail_fallback = format!("[Image {}]", before_markers[1].marker().label());
    let checked_fallback = expected_tail_fallback.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_writer(Box::new(move |text, metadata, _| {
            assert_eq!(text, checked_fallback);
            captured
                .lock()
                .unwrap()
                .push(ClipboardItem::new_string_with_metadata(
                    text.to_owned(),
                    metadata
                        .expect("successful marker cut has private metadata")
                        .to_owned(),
                ));
            gpui_text_input::ClipboardWriteOutcome::Written
        }));
    });
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-x");
    drive_until(cx, |cx| {
        service.selected_identity().unwrap().binding().root() != before_cut.root
            && composer.read_with(cx, |composer, _| !composer.test_has_active_flight())
    });
    interactive(&input, cx);
    let cut = cached_editor(&composer, cx);
    assert_eq!(cut.root.summary().logical_utf8_bytes(), DRAFT_BYTES);
    assert_eq!(cut.root.summary().marker_count(), 1);
    assert_ne!(cut.durable_history, before_cut.durable_history);
    assert_ne!(cut.widget_history, before_cut.widget_history);
    assert!(cut.widget_history.undo_available);
    assert!(!cut.widget_history.redo_available);
    assert_eq!(cut.selection.anchor, cut.selection.head);
    assert_eq!(cut.caret, cut.selection.head);
    assert_eq!(cut.caret.byte_offset.get(), DRAFT_BYTES);
    assert_eq!(cut.caret.gap, gpui_text_input::InlineObjectGap::NoObjects);
    let cut_markers = marker_facts(&mounted, &service);
    assert_eq!(cut_markers, vec![before_markers[0]]);
    assert_origin_marker_unrealized(&input, false, cx);
    let item = {
        let mut writes = writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        writes.pop().unwrap()
    };
    assert_eq!(
        item.text().as_deref(),
        Some(expected_tail_fallback.as_str())
    );
    let token = item.metadata().unwrap().clone();
    let owner = service.private_clipboard_owner().unwrap();
    let descriptor = owner.descriptor(&token).unwrap();
    assert_eq!(descriptor.origin.binding().root(), cut.root);
    assert_eq!(descriptor.origin.binding().history(), cut.durable_history);
    assert_eq!(descriptor.content_origin.binding().root(), before_cut.root);
    assert_eq!(descriptor.selection, before_cut.selection);
    budgets(&input, cx);

    let reads = Arc::new(AtomicUsize::new(0));
    let captured_reads = reads.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |limits, _| {
            assert_eq!(limits.total_bytes, CLIPBOARD_BYTES);
            captured_reads.fetch_add(1, Ordering::SeqCst);
            Ok(gpui::CheckedClipboardSnapshot {
                item: item.clone(),
                sequence: 1,
            })
        }));
    });
    let limits = match kind {
        MainWindowComposerMutationFeedbackKind::OperationTooLarge => {
            DraftMarkerAdmissionLimitsV1::new(64, 0, u64::MAX)
        }
        MainWindowComposerMutationFeedbackKind::CapacityUnavailable => {
            DraftMarkerAdmissionLimitsV1::new(0, u64::MAX, u64::MAX)
        }
        MainWindowComposerMutationFeedbackKind::Storage => {
            DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX)
        }
        _ => unreachable!(),
    };
    service
        .test_with_selected_host(|host| host.test_set_mutation_admission_retained_limits(limits));
    let repeats = if kind == MainWindowComposerMutationFeedbackKind::Storage {
        1
    } else {
        4
    };
    let ingress = support::ingress(mounted.window, cx);
    let mut prior_notice: Option<NoticeVisibleToken> = None;
    let mut prior_operation = None;
    for attempt in 0..repeats {
        if kind == MainWindowComposerMutationFeedbackKind::Storage {
            mounted.fixture.faults.fail_next(FaultPoint::BeforeCommit);
        } else {
            interactive(&input, cx);
        }
        assert_origin_marker_unrealized(&input, false, cx);
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-v");
        drive_until(cx, |cx| {
            composer.read_with(cx, |composer, _| {
                !composer.paste_pending()
                    && !composer.test_has_active_flight()
                    && composer
                        .mutation_feedback()
                        .is_some_and(|feedback| feedback.kind == kind)
            })
        });
        assert_eq!(reads.load(Ordering::SeqCst), attempt + 1);
        assert_eq!(cached_editor(&composer, cx), cut);
        let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
        assert_eq!(feedback.kind, kind);
        assert_eq!(feedback.selection.binding().root(), cut.root);
        if let Some(prior) = prior_operation {
            assert_ne!(feedback.key, prior);
        }
        prior_operation = Some(feedback.key);
        let (visible, content) = notice(&mounted, cx);
        let (title, detail) = match kind {
            MainWindowComposerMutationFeedbackKind::OperationTooLarge => (
                "Image edit is too large",
                "Use a smaller selection. Waiting and retrying the same operation will not make it fit. Your draft is unchanged.",
            ),
            MainWindowComposerMutationFeedbackKind::CapacityUnavailable => (
                "Image edit capacity is temporarily unavailable",
                "Try the operation again later, after capacity is released. Your draft is unchanged.",
            ),
            MainWindowComposerMutationFeedbackKind::Storage => (
                "Image edit storage failure",
                "The image edit could not be stored. Your draft is unchanged.",
            ),
            _ => unreachable!(),
        };
        assert_eq!(content.title().as_str(), title);
        assert_eq!(content.detail().as_str(), detail);
        assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
        assert_eq!(content.commands().count(), 0);
        if let Some(prior) = &prior_notice {
            assert!(!visible.record().same_identity(prior.record()));
        }
        prior_notice = Some(visible.clone());
        service.test_with_selected_host(|host| {
            let binding = host.binding().unwrap();
            assert_eq!(binding.root(), cut.root);
            assert_eq!(binding.history(), cut.durable_history);
            assert_eq!(host.settlement_custody_in_use(), 0);
            assert!(host.test_last_settlement_identity_custody_count() <= 3);
        });
        if kind == MainWindowComposerMutationFeedbackKind::Storage {
            assert_eq!(
                mounted.fixture.store.health().state(),
                beryl_home_store::HomeHealthState::Failed
            );
        } else {
            assert_eq!(
                mounted.fixture.store.health().state(),
                beryl_home_store::HomeHealthState::Healthy
            );
            assert_eq!(marker_facts(&mounted, &service), cut_markers);
            assert_eq!(owner.descriptor(&token), Some(descriptor));
        }
        budgets(&input, cx);
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible), app))
            .unwrap();
        for _ in 0..4 {
            support::draw(cx);
        }
        assert!(projection(mounted.window, cx).is_none());
        assert_eq!(cached_editor(&composer, cx), cut);
        if kind != MainWindowComposerMutationFeedbackKind::Storage {
            assert_eq!(owner.descriptor(&token), Some(descriptor));
        }
    }

    if kind != MainWindowComposerMutationFeedbackKind::Storage {
        cx.simulate_keystrokes(mounted.window.into(), "ctrl-z");
        drive_until(cx, |_| {
            service.selected_identity().unwrap().binding().root() == before_cut.root
        });
        interactive(&input, cx);
        let undone = cached_editor(&composer, cx);
        assert_eq!(undone.root, before_cut.root);
        assert_eq!(undone.selection, before_cut.selection);
        assert_eq!(undone.caret, before_cut.caret);
        assert!(undone.widget_history.redo_available);
        assert_eq!(marker_facts(&mounted, &service), before_markers);
        assert!(owner.descriptor(&token).is_none());
        service.test_with_selected_host(|host| {
            host.test_set_mutation_admission_retained_limits(
                DraftMarkerAdmissionLimitsV1::PRODUCTION,
            )
        });
        mounted
            .window
            .update(cx, |_, window, app| {
                input.update(app, |input, cx| {
                    input.replace_text_in_range(None, "!", window, cx)
                });
            })
            .unwrap();
        drive_until(cx, |_| {
            service.selected_identity().unwrap().binding().root() != undone.root
        });
        interactive(&input, cx);
        assert_eq!(
            service
                .selected_identity()
                .unwrap()
                .binding()
                .root()
                .summary()
                .marker_count(),
            1
        );
        assert_eq!(
            service
                .selected_identity()
                .unwrap()
                .binding()
                .root()
                .summary()
                .logical_utf8_bytes(),
            DRAFT_BYTES + 1
        );
        budgets(&input, cx);
    }
    mounted
        .window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    let weak_composer = composer.downgrade();
    let weak_input = input.downgrade();
    let weak_service = Arc::downgrade(&service);
    drop((composer, input, service, writes, reads));
    support::finish(mounted, cx);
    assert!(owner.descriptor(&token).is_none());
    drop(owner);
    assert!(weak_composer.upgrade().is_none());
    assert!(weak_input.upgrade().is_none());
    assert!(weak_service.upgrade().is_none());
}
