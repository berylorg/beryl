use super::*;
use beryl_model::SyndicThreadId;

pub(super) struct ResidentSnapshot {
    window: WindowHandle<MainWindowShellRoot>,
    mount: gpui::Entity<crate::main_window::MainWindowConversationComposerMount>,
    resident: gpui::Entity<MainWindowConversationComposer>,
    input: gpui::Entity<gpui_text_input::RangeTextInput>,
    identity: crate::main_window::MainWindowComposerSelectionIdentity,
    selection: gpui_text_input::RangeSourceSelection,
    history: syndic_storage::DraftEditHistoryFrontierV1,
    text: String,
    pages: Vec<(u64, u64, String, bool)>,
    dirty: bool,
    focus: Option<gpui::FocusHandle>,
    lineage_focus: Option<(SyndicThreadId, SyndicThreadId)>,
}

impl ResidentSnapshot {
    pub(super) fn authenticate_original_page_custody(&self, cx: &mut AsyncApp) {
        cx.update(|app| {
            assert_eq!(self.resident.read(app).selection_identity(), self.identity);
            let input = self.input.read(app);
            assert!(input.is_enabled());
            assert!(input.is_quiescent());
            assert!(input.is_surface_current_and_interactive());
            assert_eq!(input.surface().unwrap().selection(), self.selection);
            assert_eq!(
                capture_resident_pages(
                    input,
                    self.identity.binding().range_binding(),
                    &self.text,
                    self.dirty,
                ),
                self.pages,
            );
        })
        .unwrap();
    }

    pub(super) fn expect_restored_focus(&mut self, focus: gpui::FocusHandle) {
        self.focus = Some(focus);
    }
    pub(super) fn expect_restored_lineage_focus(
        &mut self,
        selected: SyndicThreadId,
        parent: SyndicThreadId,
    ) {
        self.lineage_focus = Some((selected, parent));
    }
}

pub(super) async fn capture_resident(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    index: usize,
    dirty: bool,
    cx: &mut AsyncApp,
) -> ResidentSnapshot {
    capture_resident_with_text_shape(owner, window, index, dirty, None, cx).await
}

pub(super) async fn capture_original_page_resident(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    index: usize,
    page_view_height: gpui::Pixels,
    cx: &mut AsyncApp,
) -> ResidentSnapshot {
    capture_resident_with_text_shape(owner, window, index, true, Some(page_view_height), cx).await
}

async fn capture_resident_with_text_shape(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    index: usize,
    dirty: bool,
    original_page_view_height: Option<gpui::Pixels>,
    cx: &mut AsyncApp,
) -> ResidentSnapshot {
    let resident = composer(window, cx).await;
    let input = cx.update(|app| resident.read(app).gpui_input()).unwrap();
    let mut text = if dirty {
        format!("unsaved ordinary recovery editor {index} — retained")
    } else {
        String::new()
    };
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if cx
                .update(|app| {
                    let resident = resident.read(app);
                    let input = input.read(app);
                    input.is_enabled()
                        && input.is_quiescent()
                        && input.is_surface_current_and_interactive()
                        && !resident.test_has_active_flight()
                        && !resident.test_has_pending_realizer()
                })
                .unwrap()
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ordinary editor did not become interactive"
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
    }
    if let Some(page_view_height) = original_page_view_height {
        let layout = input
            .read_with(cx, |input, _| input.resident_layout_snapshot())
            .unwrap();
        let lines =
            (f32::from(page_view_height) / f32::from(layout.layout.line_height)).ceil() as usize;
        let line = format!("p{index}");
        assert!(lines > 0);
        assert!(lines <= layout.layout.limits.segment_bytes / (line.len() + 1));
        text = std::iter::repeat(line.as_str())
            .take(lines)
            .collect::<Vec<_>>()
            .join("\n");
    }
    if dirty {
        let initial_bytes = resident
            .read_with(cx, |resident, _| {
                resident
                    .selection_identity()
                    .binding()
                    .logical_extent()
                    .logical_utf8_bytes()
            })
            .unwrap();
        window
            .update(cx, |_, window, app| {
                input.update(app, |input, _| input.focus(window));
            })
            .unwrap();
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
        window
            .update(cx, |_, window, app| {
                window.dispatch_action(Box::new(gpui_text_input::SelectAll), app);
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let selected = cx
                .update(|app| {
                    let resident = resident.read(app);
                    assert!(
                        resident.last_error().is_none(),
                        "ordinary selection proof failed: {:?}",
                        resident.last_error()
                    );
                    let input = input.read(app);
                    input.is_quiescent()
                        && resident.test_edit_position_proof_ready(app)
                        && input
                            .surface()
                            .and_then(|surface| surface.platform_selection())
                            .is_some_and(|selection| {
                                selection.range().start().get() == 0
                                    && selection.range().end().get() == initial_bytes
                            })
                })
                .unwrap();
            if selected {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ordinary full-content selection did not settle: {:?}",
                cx.update(|app| {
                    let resident = resident.read(app);
                    let input = input.read(app);
                    (
                        input.surface().map(|surface| surface.selection()),
                        input
                            .surface()
                            .and_then(|surface| surface.platform_selection()),
                        input.is_surface_current_and_interactive(),
                        input.is_quiescent(),
                        resident.test_edit_position_proof_ready(app),
                        resident.test_has_active_flight(),
                        resident.test_has_pending_realizer(),
                        input.realization_diagnostics().current.active_geometry_jobs,
                    )
                })
                .unwrap(),
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        window
            .update(cx, |_, window, app| {
                input.update(app, |input, cx| {
                    input.focus(window);
                    assert!(input.surface().unwrap().platform_selection().is_some());
                    input.replace_and_mark_text_in_range(None, &text, None, window, cx);
                });
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let settled = cx
                .update(|app| {
                    let resident = resident.read(app);
                    assert!(
                        resident.last_error().is_none(),
                        "ordinary edit failed: {:?}",
                        resident.last_error()
                    );
                    resident
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes()
                        == text.len() as u64
                        && input.read(app).is_quiescent()
                })
                .unwrap();
            if settled {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "ordinary unsaved edit did not settle: expected_bytes={} actual_bytes={}",
                text.len(),
                resident
                    .read_with(cx, |resident, _| resident
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes())
                    .unwrap(),
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        assert_eq!(copy_all(window, &resident, cx).await, text);
    }
    if let Some(page_view_height) = original_page_view_height {
        window
            .update(cx, |_, _, app| {
                input.update(app, |input, cx| {
                    input.request_absolute_scroll(gpui::px(0.), cx).unwrap();
                });
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let settled = cx
                .update(|app| {
                    let resident = resident.read(app);
                    assert!(resident.last_error().is_none());
                    let input = input.read(app);
                    input.is_quiescent()
                        && input.is_surface_current_and_interactive()
                        && !resident.test_has_active_flight()
                        && !resident.test_has_pending_realizer()
                        && input.resident_layout_snapshot().viewport_extent == page_view_height
                        && input.surface().unwrap().content_height() >= page_view_height
                        && input.surface().unwrap().pages().iter().any(|page| {
                            page.range().start().get() == 0
                                && page.range().end().get() == text.len() as u64
                                && page.text() == text
                                && page.end_of_source()
                        })
                })
                .unwrap();
            if settled {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "original complete Page view admission did not settle"
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
    }
    let (mount, identity, selection) = cx
        .update(|app| {
            (
                window
                    .read(app)
                    .unwrap()
                    .controller()
                    .unwrap()
                    .composer_mount()
                    .unwrap(),
                resident.read(app).selection_identity(),
                input.read(app).surface().unwrap().selection(),
            )
        })
        .unwrap();
    let history = capture_resident_history(owner, identity.claim().thread_id(), identity.binding());
    let pages = input
        .read_with(cx, |input, _| {
            capture_resident_pages(input, identity.binding().range_binding(), &text, dirty)
        })
        .unwrap();
    if original_page_view_height.is_some() {
        assert_eq!(pages, vec![(0, text.len() as u64, text.clone(), true)]);
    }
    if dirty {
        assert_candidate_saved(
            owner,
            identity.claim().thread_id(),
            identity.binding(),
            false,
        );
    }
    let focus = if dirty {
        window
            .update(cx, |_, window, app| window.focused(app))
            .unwrap()
    } else {
        None
    };
    ResidentSnapshot {
        window,
        mount,
        resident,
        input,
        identity,
        selection,
        history,
        text,
        pages,
        dirty,
        focus,
        lineage_focus: None,
    }
}

pub(super) async fn verify_resident(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    before: ResidentSnapshot,
    cx: &mut AsyncApp,
) {
    verify_preserved_resident(owner, before, false, cx).await;
}

pub(super) async fn verify_resident_preserving_page_custody(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    before: ResidentSnapshot,
    cx: &mut AsyncApp,
) {
    verify_preserved_resident(owner, before, true, cx).await;
}

async fn verify_preserved_resident(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    before: ResidentSnapshot,
    preserve_page_custody: bool,
    cx: &mut AsyncApp,
) {
    let resident = composer(before.window, cx).await;
    assert_eq!(resident, before.resident);
    let fresh = cx
        .update(|app| {
            let root = before.window.read(app).unwrap();
            let mount = root.controller().unwrap().composer_mount().unwrap();
            assert_eq!(mount, before.mount);
            assert_eq!(mount.read(app).contribution().unwrap(), before.resident);
            let resident = resident.read(app);
            assert_eq!(resident.gpui_input(), before.input);
            let input = before.input.read(app);
            assert!(input.is_enabled());
            assert!(input.is_quiescent());
            assert!(input.is_surface_current_and_interactive());
            assert_eq!(input.surface().unwrap().selection(), before.selection);
            resident.selection_identity()
        })
        .unwrap();
    if before.dirty {
        if let Some((selected, parent)) = before.lineage_focus {
            assert!(
                before
                    .window
                    .update(cx, |root, window, app| root
                        .test_thread_lineage_focus(selected, parent, window, app))
                    .unwrap()
            );
        } else {
            assert_eq!(
                before
                    .window
                    .update(cx, |_, window, app| window.focused(app))
                    .unwrap(),
                before.focus
            );
        }
    }
    assert_eq!(fresh.claim(), before.identity.claim());
    assert_eq!(fresh.window_id(), before.identity.window_id());
    assert_ne!(
        fresh.binding().home_generation(),
        before.identity.binding().home_generation()
    );
    assert_eq!(fresh.binding().root(), before.identity.binding().root());
    let candidate = fresh.binding().candidate();
    let prior_candidate = before.identity.binding().candidate();
    assert_eq!(candidate.draft_id(), prior_candidate.draft_id());
    assert_eq!(candidate.session_id(), prior_candidate.session_id());
    assert_eq!(
        candidate.candidate_generation(),
        prior_candidate.candidate_generation()
    );
    assert_eq!(candidate.root(), prior_candidate.root());
    assert_eq!(candidate.logical_extent(), prior_candidate.logical_extent());
    if before.dirty {
        assert!(candidate.session_generation() > prior_candidate.session_generation());
        assert_ne!(candidate.history(), prior_candidate.history());
    } else {
        assert_eq!(candidate, prior_candidate);
    }
    let current = fresh.binding().history();
    let original = before.identity.binding().history();
    assert_eq!(current.root(), original.root());
    assert_eq!(
        current.candidate_generation(),
        original.candidate_generation()
    );
    assert_eq!(current.frontier_revision(), original.frontier_revision());
    assert_eq!(current.byte_budget(), original.byte_budget());
    assert_eq!(
        current.retention_policy_revision(),
        original.retention_policy_revision()
    );
    assert_eq!(current.availability(), original.availability());
    let history = capture_resident_history(owner, fresh.claim().thread_id(), fresh.binding());
    assert_eq!(history.journal_head(), before.history.journal_head());
    assert_eq!(history.undo_head(), before.history.undo_head());
    assert_eq!(history.redo_head(), before.history.redo_head());
    assert_eq!(history.oldest_eligible(), before.history.oldest_eligible());
    assert_eq!(
        history.cumulative_encoded_bytes(),
        before.history.cumulative_encoded_bytes()
    );
    assert_eq!(
        history.retained_encoded_bytes(),
        before.history.retained_encoded_bytes()
    );
    if before.dirty {
        if preserve_page_custody {
            cx.update(|app| {
                let input = before.input.read(app);
                let pages = capture_resident_pages(
                    input,
                    fresh.binding().range_binding(),
                    &before.text,
                    true,
                );
                assert_eq!(pages, before.pages);
            })
            .unwrap();
        } else {
            assert_eq!(copy_all(before.window, &resident, cx).await, before.text);
        }
        assert_candidate_saved(owner, fresh.claim().thread_id(), fresh.binding(), true);
    }
}

fn capture_resident_pages(
    input: &gpui_text_input::RangeTextInput,
    binding: gpui_text_input::RangeBinding,
    text: &str,
    authenticate_text: bool,
) -> Vec<(u64, u64, String, bool)> {
    let surface = input.surface().unwrap();
    assert!(
        !surface.pages().is_empty(),
        "original resident has no Page custody"
    );
    surface
        .pages()
        .iter()
        .map(|page| {
            assert_eq!(page.key().binding(), binding.binding());
            assert_eq!(page.key().revision(), binding.revision());
            let start = page.range().start().get();
            let end = page.range().end().get();
            if authenticate_text {
                assert_eq!(page.text(), &text[start as usize..end as usize]);
                assert_eq!(page.end_of_source(), end == text.len() as u64);
            }
            (start, end, page.text().to_owned(), page.end_of_source())
        })
        .collect()
}

fn assert_candidate_saved(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    thread: beryl_model::SyndicThreadId,
    binding: crate::composer_host::ComposerHostBinding,
    expected: bool,
) {
    let retained = owner.borrow();
    let graph = retained.test_services().graph().unwrap();
    let current = graph
        .syndic()
        .current_draft_piece_text_demand(
            graph.home(),
            thread,
            syndic_storage::DraftPieceTextDemandV1::Forward(0),
            4096,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        graph
            .syndic()
            .draft_editor_candidate_is_saved(graph.home(), binding.candidate(), current.selector(),)
            .unwrap(),
        expected
    );
}
