use super::*;
use crate::bootstrap::test_support::{Producer, ProducerHold};
use crate::main_window::MainWindowShellRoot;
use gpui::{AppContext, WindowHandle};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

#[path = "bootstrap_progressive/support.rs"]
mod support;

fn running(app: &App) -> bool {
    app.global::<BootstrapLifetime>().running.is_some()
}
fn exit(app: &mut App) {
    app.global::<BootstrapLifetime>().commands.request_exit();
}

fn identity(
    window: WindowHandle<MainWindowShellRoot>,
    app: &App,
) -> (beryl_model::WindowId, Option<beryl_model::SyndicThreadId>) {
    window
        .read_with(app, |root, app| {
            (
                root.controller().unwrap().window_id(),
                root.diagnostic_selected_thread(app),
            )
        })
        .unwrap()
}

fn assert_local_editor(window: WindowHandle<MainWindowShellRoot>, app: &App, bytes: usize) -> bool {
    window
        .read_with(app, |root, app| {
            assert!(!root.startup_interaction_gated());
            let composer = root.test_submission_composer(app).unwrap();
            let composer = composer.read(app);
            assert!(
                composer.last_error().is_none(),
                "composer error: {:?}",
                composer.last_error()
            );
            assert_submission_gated(root, app);
            let input = composer.gpui_input();
            let input = input.read(app);
            input.is_enabled()
                && composer.selected_first_presentable(app)
                && composer
                    .selection_identity()
                    .binding()
                    .logical_extent()
                    .logical_utf8_bytes()
                    == bytes as u64
                && input.is_quiescent()
                && composer.test_edit_position_proof_ready(app)
        })
        .unwrap()
}

fn assert_submission_gated(root: &MainWindowShellRoot, app: &App) {
    assert!(
        root.test_submission_observation(app)
            .and_then(|proof| proof.availability())
            .is_err(),
        "held backend cannot admit submission through a retained proof"
    );
    assert!(
        root.test_submission_composer(app)
            .unwrap()
            .read(app)
            .submission_unavailable()
            .is_some(),
        "the ordinary composer and Enter share the genuine readiness gate"
    );
}

fn assert_saved_editor(window: WindowHandle<MainWindowShellRoot>, app: &App, bytes: usize) -> bool {
    assert_local_editor(window, app, bytes)
        && window
            .read_with(app, |root, app| {
                root.test_submission_composer(app)
                    .unwrap()
                    .read(app)
                    .test_saved_candidate_current(app)
            })
            .unwrap()
}

struct NativeTextFailureSnapshot {
    bytes: [u8; 8192],
    len: usize,
    truncated: bool,
}

impl std::fmt::Write for NativeTextFailureSnapshot {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if value.len() > 7936 - self.len {
            self.truncated = true;
            return Err(std::fmt::Error);
        }
        self.bytes[self.len..self.len + value.len()].copy_from_slice(value.as_bytes());
        self.len += value.len();
        Ok(())
    }
}

fn native_text_failure_snapshot(
    window: WindowHandle<MainWindowShellRoot>,
    app: &App,
    expected_bytes: usize,
) -> String {
    use std::fmt::Write as _;

    let mut snapshot = NativeTextFailureSnapshot {
        bytes: [0; 8192],
        len: 0,
        truncated: false,
    };
    let root_id = window.entity(app).map(|root| root.entity_id()).ok();
    let _ = writeln!(
        snapshot,
        "native_text_failure_context window={:?} root={root_id:?} expected_bytes={expected_bytes} running={}",
        window.window_id(),
        running(app),
    );
    let read = window.read_with(app, |root, app| {
        let member = root.controller().map(|controller| controller.window_id());
        let _ = writeln!(snapshot, "member={member:?} startup_gated={}", root.startup_interaction_gated());
        let Some(composer) = root.test_submission_composer(app) else {
            let _ = writeln!(snapshot, "composer=unavailable");
            return;
        };
        let composer_id = composer.entity_id();
        let composer = composer.read(app);
        let selected = composer.selection_identity();
        let selected_binding = selected.binding().range_binding();
        let input = composer.gpui_input();
        let input_id = input.entity_id();
        let input = input.read(app);
        let surface_binding = input.surface().map(|surface| surface.binding());
        let _ = writeln!(snapshot,
            "composer={composer_id:?} input={input_id:?} pending_target={} composer_error={} selected_binding={selected_binding:?} selected_bytes={} surface_binding={surface_binding:?} surface_bytes={:?} binding_equal={}",
            composer.is_pending_target(), composer.last_error().is_some(),
            selected.binding().logical_extent().logical_utf8_bytes(),
            surface_binding.map(|binding| binding.extent().byte_len()),
            surface_binding == Some(selected_binding));
        let _ = writeln!(snapshot,
            "enabled={} current={} quiescent={} semantic_quiescent={} edit_proof_ready={} selected_service_identity=unavailable_without_lock autosave=not_sampled native_handler_admission=unobserved",
            input.is_enabled(), input.is_surface_current_and_interactive(), input.is_quiescent(),
            input.is_semantically_quiescent(), composer.test_edit_position_proof_ready(app));
        let admission = composer.last_mutation_admission_failure().map(std::mem::discriminant);
        let (outcome, committed_bytes, cancelled_staging) = composer.test_last_mutation_outcome()
            .map_or((None, None, None), |(_, outcome, cancelled)| {
                let committed = match outcome {
                    crate::composer_host::ComposerHostMutationOutcome::Committed { binding, .. } =>
                        Some(binding.logical_extent().logical_utf8_bytes()),
                    _ => None,
                };
                (Some(std::mem::discriminant(outcome)), committed, Some(*cancelled))
            });
        let _ = writeln!(snapshot,
            "mutation_admission={admission:?} mutation_outcome={outcome:?} committed_bytes={committed_bytes:?} cancelled_staging={cancelled_staging:?}");
        let diagnostics = input.realization_diagnostics();
        let current = diagnostics.current;
        let _ = writeln!(snapshot,
            "frame_generation={} frame={:?} continuation={} capacity={:?} config_items={} candidates={} intents(target,layout,presentation,rebind,index)={:?} geometry(jobs,pages,objects,resident_page_waits,deferred_responses)={:?} requests(queued,pending_pages,pending_objects,dispatched_pages,dispatched_objects,custody)={:?} response_rejections={} rejection={:?} rejection_stage={:?} owned_bytes={} owned_items={}",
            diagnostics.frame_generation, diagnostics.frame, diagnostics.continuation_scheduled,
            diagnostics.capacity, current.pending_configuration_items, current.candidates,
            (current.pending_target_intents, current.pending_layout_intents, current.pending_presentation_intents, current.pending_rebind_intents, current.pending_index_intents),
            (current.active_geometry_jobs, current.pending_geometry_pages, current.pending_geometry_objects, current.resident_geometry_page_waits, current.deferred_geometry_responses),
            (current.queued_requests, current.pending_page_requests, current.pending_object_requests, current.dispatched_page_requests, current.dispatched_object_requests, current.response_custody_count),
            diagnostics.response_rejection_count, diagnostics.last_response_rejection, diagnostics.last_response_rejection_stage,
            current.owned_bytes, current.owned_items);
        let proof = composer.test_edit_position_proof_diagnostics(app);
        let mut end = proof.len().min(1536);
        while !proof.is_char_boundary(end) { end -= 1; }
        let _ = writeln!(snapshot, "cached_proof_prefix_truncated={} cached_proof_prefix={}", end < proof.len(), &proof[..end]);
    });
    if read.is_err() {
        let _ = writeln!(snapshot, "window_read=unavailable");
    }
    let tail = if snapshot.truncated {
        b"\nsnapshot_truncated=1\n".as_slice()
    } else {
        b"snapshot_truncated=0\n".as_slice()
    };
    snapshot.bytes[snapshot.len..snapshot.len + tail.len()].copy_from_slice(tail);
    snapshot.len += tail.len();
    String::from_utf8_lossy(&snapshot.bytes[..snapshot.len]).into_owned()
}

#[path = "bootstrap_progressive/first_editor.rs"]
mod first_editor;
#[path = "bootstrap_progressive/publication.rs"]
mod publication;
#[path = "bootstrap_progressive/readiness.rs"]
mod readiness;
#[path = "bootstrap_progressive/startup_modes.rs"]
mod startup_modes;
