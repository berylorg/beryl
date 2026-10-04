use super::*;
use crate::cas_projection::{
    MinimumTurnCaptureReserve, ProcessScheduledExecutionProvider, ProjectionConnectionService,
    ProjectionServiceConfig, ScheduledExecutionSessions,
};
use crate::lifecycle_attention::{LifecycleAttentionToken, ProcessLifecycleAttentionPool};
use crate::main_window::*;
use crate::window_acquisition::*;
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{
    ExecutionBinding, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, SyndicTurnId, WindowBounds, WindowDisplayState, WindowId, WindowPlacement,
};
use beryl_state::{BerylState, RememberedTarget};
use gpui::AppContext;
use initial_support::{Fixture, config, native_path, placement};
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, SyndicStorage, SyndicTimestamp,
};

#[path = "../pending_composer_activation/support.rs"]
pub(super) mod composer_support;
#[path = "../main_window_creation/support.rs"]
mod creation_support;
#[path = "../main_window_shell/support.rs"]
pub(super) mod home_support;
#[path = "../initial_composer/support.rs"]
pub(super) mod initial_support;
#[path = "../notice_mount/support.rs"]
pub(super) mod support;

pub(super) struct Source {
    pub(super) reader: Arc<PublishedRunningThreadsReader>,
    pub(super) service: ProjectionConnectionService,
    pub(super) _sessions: ScheduledExecutionSessions,
    pub(super) attention: Arc<ProcessLifecycleAttentionPool>,
    _lifetime: Arc<()>,
    pub(super) home: Arc<beryl_home_store::HomeServiceReference>,
    pub(super) token: LifecycleAttentionToken,
}

#[track_caller]
fn drive(cx: &mut gpui::TestAppContext, mut ready: impl FnMut(&mut gpui::TestAppContext) -> bool) {
    let caller = std::panic::Location::caller();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        support::draw(cx);
        if ready(cx) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "running shell milestone {caller} did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_same_resident_presentation(
    actual: crate::syndic_transcript::ResidentTranscriptStatusFacts,
    expected: &crate::syndic_transcript::ResidentTranscriptStatusFacts,
) {
    assert_eq!(actual.state, expected.state);
    assert_eq!(
        (
            actual.activation_revision,
            actual.presentation_revision,
            actual.scroll_mode,
            actual.anchor_record_id,
            actual.anchor_position
        ),
        (
            expected.activation_revision,
            expected.presentation_revision,
            expected.scroll_mode,
            expected.anchor_record_id.clone(),
            expected.anchor_position
        ),
    );
    assert_eq!(
        (
            actual.resident_presentation_record_count,
            actual.resident_view_record_count,
            actual.resident_projection_record_count,
            actual.resident_resource_metadata_count,
            actual.resident_resource_slice_count,
            actual.resident_fallback_record_count
        ),
        (
            expected.resident_presentation_record_count,
            expected.resident_view_record_count,
            expected.resident_projection_record_count,
            expected.resident_resource_metadata_count,
            expected.resident_resource_slice_count,
            expected.resident_fallback_record_count
        ),
    );
    assert_eq!(actual.turn_view, expected.turn_view);
    assert!(actual.pending_demand_fact_count <= 128);
    assert_eq!(actual.pending_provider_request_count, 0);
}

pub(super) fn mount(cx: &mut gpui::TestAppContext, seed: u8) -> support::Mounted {
    let mounted = support::mount(cx, seed);
    eprintln!(
        "owned running-thread shell fixture: {}",
        mounted.fixture.directory.path().display()
    );
    mounted
}

pub(super) fn source(mounted: &support::Mounted, cx: &mut gpui::TestAppContext) -> Source {
    let home_store = mounted.fixture.store.clone();
    let state = mounted.fixture.state.clone();
    let storage = mounted.fixture.storage.clone();
    let selected = mounted
        .window
        .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
        .unwrap();
    home_support::join(
        home_support::worker(move || {
            let (provider, sessions) = ProcessScheduledExecutionProvider::new();
            let home = Arc::new(home_store.service_reference());
            let service = ProjectionConnectionService::new_borrowed_for_test(
                Default::default(),
                &home_store,
                storage.clone(),
                ProjectionServiceConfig::try_new(
                    8,
                    4,
                    MinimumTurnCaptureReserve::try_new(1).unwrap(),
                )
                .unwrap(),
                Box::new(provider),
            )
            .unwrap();
            let attention = Arc::new(ProcessLifecycleAttentionPool::new());
            let attempt = attention
                .track_accepted_yield(
                    service.home_id(),
                    selected,
                    SyndicTurnId::from_bytes([19; 16]),
                    crate::LifecycleYieldOutcome::PhaseNeedsReview,
                )
                .unwrap();
            attention.report_terminal(&attempt);
            let token = attention.snapshot()[0].token().clone();
            let lifetime = Arc::new(());
            let reader = Arc::new(PublishedRunningThreadsReader::for_test(
                &service,
                &sessions,
                state,
                home.clone(),
                storage,
                &attention,
                &lifetime,
            ));
            Source {
                reader,
                service,
                _sessions: sessions,
                attention,
                _lifetime: lifetime,
                home,
                token,
            }
        }),
        cx,
    )
}

pub(super) fn install_source(
    mounted: &support::Mounted,
    source: &Source,
    cx: &mut gpui::TestAppContext,
) {
    mounted
        .window
        .update(cx, |root, _, cx| {
            root.running_threads.fixture_reader = Some(Arc::downgrade(&source.reader));
            cx.notify();
        })
        .unwrap();
    let mut last = None;
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| {
                let facts = (
                    root.running_threads.count,
                    root.running_threads.reader.is_some(),
                    root.running_threads.transcript_provider.is_some(),
                    root.running_threads.workers.retained(),
                    root.running_threads.failure.is_some(),
                    root.running_threads.reads_suspended,
                );
                if last != Some(facts) {
                    eprintln!("running source settlement {facts:?}");
                    last = Some(facts);
                }
                root.running_threads.count == Some(1)
                    && root.running_threads.transcript_provider.is_some()
            })
            .unwrap()
    });
}

pub(super) fn open_by_command(
    mounted: &support::Mounted,
    cx: &mut gpui::TestAppContext,
) -> Entity<ThreadRootPicker> {
    mounted
        .window
        .update(cx, |root, window, _| {
            root.running_threads.focus.focus(window)
        })
        .unwrap();
    support::draw(cx);
    gpui::VisualTestContext::from_window(mounted.window.into(), cx).simulate_keystrokes("enter");
    let mut last = None;
    drive(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, app| {
                root.running_threads.picker.as_ref().is_some_and(|picker| {
                    let facts = picker.read(app).diagnostics();
                    let state = (
                        facts.total_count,
                        facts.pending_page_count,
                        facts.resident_row_count,
                    );
                    if last != Some(state) {
                        eprintln!("running picker settlement {state:?}");
                        last = Some(state);
                    }
                    facts.total_count == 1
                        && facts.pending_page_count == 0
                        && facts.resident_row_count == 1
                })
            })
            .unwrap()
    });
    mounted
        .window
        .read_with(cx, |root, _| root.running_threads.picker.clone().unwrap())
        .unwrap()
}

pub(super) fn finish(mounted: support::Mounted, source: Source, cx: &mut gpui::TestAppContext) {
    mounted
        .window
        .update(cx, |root, window, cx| {
            root.running_threads.fixture_reader = None;
            root.set_ordinary_close_interaction_gated(true, cx);
            root.suspend_running_thread_reads(window, cx);
        })
        .unwrap();
    support::drive_until(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| root.running_thread_reads_drained())
            .unwrap()
    });
    assert!(
        mounted
            .window
            .update(cx, |root, _, _| root
                .release_suspended_running_thread_sources())
            .unwrap()
    );
    source.attention.close();
    home_support::join(home_support::worker(move || drop(source)), cx);
    support::finish(mounted, cx);
}

#[gpui::test]
fn command_and_current_row_callbacks_acknowledge_exact_attention_without_arrival_focus(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 181);
    let initial = mounted
        .window
        .read_with(cx, |root, app| {
            (
                root.coherent_viewed_thread(app).unwrap(),
                root.notice_safe_focus(app),
            )
        })
        .unwrap();
    mounted
        .window
        .update(cx, |_, window, _| initial.1.focus(window))
        .unwrap();
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    gpui::VisualTestContext::from_window(mounted.window.into(), cx)
        .simulate_input("an edited draft");
    cx.executor().advance_clock(Duration::from_secs(1));
    support::drive_until(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, app| {
                root.cached_running_selection(app)
                    .is_some_and(|(selection, _)| selection.binding() != prior.binding())
            })
            .unwrap()
    });
    assert_eq!(
        mounted
            .window
            .read_with(cx, |root, app| root
                .cached_running_selection(app)
                .unwrap()
                .0
                .claim())
            .unwrap(),
        prior.claim()
    );
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    mounted
        .window
        .update(cx, |root, window, cx| {
            assert!(initial.1.is_focused(window));
            assert_eq!(root.coherent_viewed_thread(cx), Some(initial.0));
            assert!(root.running_threads.picker.is_none());
        })
        .unwrap();
    let picker = open_by_command(&mounted, cx);
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    mounted
        .window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.focus_row(0, window, cx))
        })
        .unwrap();
    support::draw(cx);
    gpui::VisualTestContext::from_window(mounted.window.into(), cx).simulate_keystrokes("enter");
    support::drive_until(cx, |cx| {
        source.attention.snapshot().is_empty()
            && mounted
                .window
                .read_with(cx, |root, _| !root.running_threads.has_activation_custody())
                .unwrap()
    });
    assert_eq!(
        mounted
            .window
            .read_with(cx, |root, app| root.coherent_viewed_thread(app))
            .unwrap(),
        Some(initial.0)
    );
    assert_eq!(cx.windows().len(), 1);
    finish(mounted, source, cx);
}

#[gpui::test]
fn elsewhere_row_reveals_the_exact_published_window_and_acknowledges_only_its_token(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 197);
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let picker = open_by_command(&mounted, cx);
    let other = support::mount_second(&mounted, cx);
    let prior = mounted
        .window
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    let other_selection = other
        .read_with(cx, |root, app| {
            root.cached_running_selection(app).unwrap().0
        })
        .unwrap();
    let attempt = source
        .attention
        .track_accepted_yield(
            source.service.home_id(),
            other_selection.claim().thread_id(),
            SyndicTurnId::from_bytes([31; 16]),
            crate::LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    source.attention.report_terminal(&attempt);
    let clicked_token = source
        .attention
        .snapshot()
        .into_iter()
        .find(|record| record.thread_id() == other_selection.claim().thread_id())
        .unwrap()
        .token()
        .clone();
    mounted
        .window
        .update(cx, |root, window, _| {
            root.running_threads.fixture_windows = vec![mounted.window, other];
            window.activate_window();
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(600));
    drive(cx, |cx| {
        picker.read_with(cx, |picker, _| {
            let facts = picker.diagnostics();
            facts.total_count == 2 && facts.pending_page_count == 0 && facts.resident_row_count == 2
        })
    });
    let position = mounted
        .window
        .read_with(cx, |root, _| {
            root.running_threads
                .pages
                .iter()
                .find_map(|page| {
                    page.records()
                        .iter()
                        .position(|row| row.thread_id == other_selection.claim().thread_id())
                        .and_then(|offset| {
                            usize::try_from(page.logical_start())
                                .ok()?
                                .checked_add(offset)
                        })
                })
                .unwrap()
        })
        .unwrap();
    mounted
        .window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.focus_row(position, window, cx))
        })
        .unwrap();
    support::draw(cx);
    gpui::VisualTestContext::from_window(mounted.window.into(), cx).simulate_keystrokes("enter");
    drive(cx, |cx| {
        source
            .attention
            .snapshot()
            .iter()
            .all(|record| record.token() != &clicked_token)
            && mounted
                .window
                .read_with(cx, |root, _| !root.running_threads.has_activation_custody())
                .unwrap()
    });
    assert!(cx.update(|app| app.active_window()) == Some(other.into()));
    assert_eq!(
        mounted
            .window
            .read_with(cx, |root, app| root
                .cached_running_selection(app)
                .unwrap()
                .0)
            .unwrap(),
        prior
    );
    assert_eq!(
        other
            .read_with(cx, |root, app| root
                .cached_running_selection(app)
                .unwrap()
                .0)
            .unwrap(),
        other_selection
    );
    assert_eq!(source.attention.snapshot().len(), 1);
    assert_eq!(source.attention.snapshot()[0].token(), &source.token);
    assert_eq!(cx.windows().len(), 2);
    finish(mounted, source, cx);
}

#[gpui::test]
fn actual_search_callbacks_supersede_pages_and_keep_the_final_query_coherent(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 185);
    let source = source(&mounted, cx);
    install_source(&mounted, &source, cx);
    let picker = open_by_command(&mounted, cx);
    let first_revision = picker.read_with(cx, |picker, _| picker.diagnostics().query_revision);
    mounted
        .window
        .update(cx, |_, window, cx| {
            picker.update(cx, |picker, cx| picker.focus_search(window, cx))
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(mounted.window.into(), cx);
    visual.simulate_input("Beryl");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("absent-thread");
    drop(visual);
    let mut last = None;
    drive(cx, |cx| {
        cx.executor().advance_clock(Duration::from_millis(10));
        let host = mounted
            .window
            .read_with(cx, |root, _| {
                (
                    root.running_threads.query_revision,
                    root.running_threads.query.as_str() == "absent-thread",
                    root.running_threads.page_jobs.len(),
                )
            })
            .unwrap();
        picker.read_with(cx, |picker, _| {
            let facts = picker.diagnostics();
            let state = (
                facts.query_revision,
                facts.total_count,
                facts.pending_page_count,
                facts.collection_failed,
                host,
            );
            if last != Some(state) {
                eprintln!("search settlement {state:?}");
                last = Some(state);
            }
            facts.query_revision > first_revision
                && facts.total_count == 0
                && facts.pending_page_count == 0
        })
    });
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(root.running_threads.query.as_str(), "absent-thread");
            assert!(root.running_threads.page_jobs.is_empty());
            assert!(
                root.running_threads
                    .pages
                    .iter()
                    .all(|page| page.query() == &root.running_threads.query)
            );
            assert!(!picker.read(app).diagnostics().collection_failed);
        })
        .unwrap();
    assert_eq!(source.attention.snapshot().len(), 1);
    finish(mounted, source, cx);
}

#[gpui::test]
fn cancelled_close_drains_outputs_and_home_sources_while_preserving_the_resident_panel(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 189);
    let source = source(&mounted, cx);
    let baseline = Arc::strong_count(&source.home);
    install_source(&mounted, &source, cx);
    let picker = open_by_command(&mounted, cx);
    let panel = mounted
        .window
        .read_with(cx, |root, _| root.running_threads.transcript.clone())
        .unwrap();
    let placement = panel.read_with(cx, |panel, _| panel.refresh_placement());
    let facts = panel.read_with(cx, |panel, _| panel.status_facts());
    let collection_facts = mounted
        .window
        .read_with(cx, |root, _| {
            (
                root.running_threads.count,
                root.running_threads.attention,
                root.running_threads
                    .pages
                    .iter()
                    .map(|page| page.records().len())
                    .sum::<usize>(),
            )
        })
        .unwrap();
    let content_identity = mounted
        .window
        .read_with(cx, |root, app| {
            (
                root.running_threads.transcript_claim,
                root.cached_running_selection(app).unwrap().0,
            )
        })
        .unwrap();
    let output = mounted
        .window
        .update(cx, |root, window, cx| {
            root.set_ordinary_close_interaction_gated(true, cx);
            root.suspend_running_thread_reads(window, cx);
            RunningReadOutput {
                result: (*source.reader).clone(),
                _release: root
                    .running_threads
                    .workers
                    .track(release_running_read_output as fn()),
            }
        })
        .unwrap();
    assert!(
        !mounted
            .window
            .read_with(cx, |root, _| root.running_thread_reads_drained())
            .unwrap()
    );
    assert!(
        !mounted
            .window
            .update(cx, |root, _, _| root
                .release_suspended_running_thread_sources())
            .unwrap()
    );
    assert_same_resident_presentation(panel.read_with(cx, |panel, _| panel.status_facts()), &facts);
    drop(output);
    support::drive_until(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| root.running_thread_reads_drained())
            .unwrap()
    });
    assert!(
        mounted
            .window
            .update(cx, |root, _, _| root
                .release_suspended_running_thread_sources())
            .unwrap()
    );
    let generation = mounted
        .window
        .read_with(cx, |root, _| {
            root.running_threads.generation.load(Ordering::Acquire)
        })
        .unwrap();
    assert!(
        mounted
            .window
            .update(cx, |root, _, _| root
                .release_suspended_running_thread_sources())
            .unwrap()
    );
    mounted
        .window
        .read_with(cx, |root, app| {
            assert_eq!(
                root.running_threads.generation.load(Ordering::Acquire),
                generation
            );
            assert_eq!(root.running_threads.transcript, panel);
            assert_eq!(root.running_threads.picker.as_ref(), Some(&picker));
            assert!(root.running_threads.reader.is_none());
            assert!(root.running_threads.transcript_provider.is_none());
            assert_eq!(root.running_threads.workers.retained(), 0);
            assert!(root.running_threads.source_revision.is_none());
            assert_eq!(
                (
                    root.running_threads.transcript_claim,
                    root.cached_running_selection(app).unwrap().0
                ),
                content_identity
            );
            assert_eq!(
                (
                    root.running_threads.count,
                    root.running_threads.attention,
                    root.running_threads
                        .pages
                        .iter()
                        .map(|page| page.records().len())
                        .sum::<usize>(),
                ),
                collection_facts
            );
            assert_eq!(panel.read(app).refresh_placement(), placement);
            assert_same_resident_presentation(panel.read(app).status_facts(), &facts);
        })
        .unwrap();
    assert_eq!(Arc::strong_count(&source.home), baseline);
    mounted
        .window
        .update(cx, |root, _, cx| {
            root.set_ordinary_close_interaction_gated(false, cx)
        })
        .unwrap();
    support::drive_until(cx, |cx| {
        mounted
            .window
            .read_with(cx, |root, _| {
                root.running_threads.reader.is_some()
                    && root.running_threads.transcript_provider.is_some()
                    && !root.running_threads.reads_suspended
            })
            .unwrap()
    });
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.refresh_placement()),
        placement
    );
    finish(mounted, source, cx);
}

#[gpui::test]
fn empty_running_sources_do_not_block_existing_threadless_or_source_unavailable_cleanup(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = mount(cx, 193);
    mounted
        .window
        .update(cx, |root, _, _| {
            assert!(root.running_thread_reads_drained());
            assert!(root.release_suspended_running_thread_sources());
            assert!(!root.running_threads.reads_suspended);
        })
        .unwrap();
    support::finish(mounted, cx);
}
