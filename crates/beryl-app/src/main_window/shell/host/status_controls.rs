use super::*;
use crate::app_services::PublishedExactStopWorker;
use crate::cas_projection::{
    ExactOperationOrigin, ExactParentState, ExactSelectedOperationSnapshot,
    ExactSoftStopAvailability, ExactSoftStopEligibility, ExactSoftStopUnavailable,
    ExactStopFeedback, ExactStopFeedbackState, ExactStopRequestError,
};
use crate::main_window::MainWindowComposerSelectionIdentity;
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

mod render;
pub(super) use render::{render_menu, render_strip};

const FEEDBACK_LIMIT: usize = 72;

pub(crate) struct ExactStopFeedbackHandoff {
    pub(crate) origin: ExactOperationOrigin,
    pub(crate) feedback: ExactStopFeedback,
}

pub(super) struct ExactStatusControls {
    worker: Option<PublishedExactStopWorker>,
    selection: Option<MainWindowComposerSelectionIdentity>,
    snapshot: ExactSelectedOperationSnapshot,
    generation: Arc<AtomicU64>,
    poll: Option<gpui::Task<()>>,
    request_pending: Option<ExactOperationOrigin>,
    feedback: Vec<ExactStopFeedbackHandoff>,
    request_failure: Option<&'static str>,
    menu_anchor: Option<ExactOperationOrigin>,
    menu_focus: gpui::FocusHandle,
    return_focus: Option<gpui::WeakFocusHandle>,
}

impl ExactStatusControls {
    pub(super) fn new(cx: &mut Context<MainWindowShellRoot>) -> Self {
        Self {
            worker: None,
            selection: None,
            snapshot: ExactSelectedOperationSnapshot::unavailable(),
            generation: Arc::new(AtomicU64::new(1)),
            poll: None,
            request_pending: None,
            feedback: Vec::new(),
            request_failure: None,
            menu_anchor: None,
            menu_focus: cx.focus_handle(),
            return_focus: None,
        }
    }

    fn feedback(&self) -> Option<&ExactStopFeedback> {
        let origin = self.snapshot.origin.as_ref()?;
        self.feedback
            .iter()
            .rev()
            .find(|entry| &entry.origin == origin)
            .map(|entry| &entry.feedback)
    }

    fn request_blocked(&self) -> bool {
        self.request_pending.is_some()
            || self.feedback().is_some_and(|feedback| {
                matches!(
                    feedback.snapshot().state,
                    ExactStopFeedbackState::Waiting | ExactStopFeedbackState::VolatileNondispatch
                )
            })
    }

    fn menu_available(&self) -> bool {
        self.snapshot.operation_active
            && self.snapshot.origin.is_some()
            && (matches!(self.snapshot.stop, ExactSoftStopAvailability::Eligible(_))
                || self.feedback().is_some()
                || self.request_pending == self.snapshot.origin)
    }

    fn command_enabled(&self) -> bool {
        self.menu_available()
            && !self.request_blocked()
            && self.feedback.len() < FEEDBACK_LIMIT
            && matches!(self.snapshot.stop, ExactSoftStopAvailability::Eligible(_))
    }

    fn reason(&self) -> &'static str {
        if self.feedback.len() == FEEDBACK_LIMIT {
            return "The bounded stop feedback capacity is full.";
        }
        if self.request_pending.is_some() {
            return "The exact soft-stop request is being admitted.";
        }
        if let Some(feedback) = self.feedback() {
            match feedback.snapshot().state {
                ExactStopFeedbackState::Waiting => {
                    return "The exact soft-stop request is awaiting its outcome.";
                }
                ExactStopFeedbackState::VolatileNondispatch => {
                    return "This volatile soft-stop request was not dispatched and cannot be retried.";
                }
                _ => {}
            }
        }
        match &self.snapshot.stop {
            ExactSoftStopAvailability::Eligible(_) => {
                "Soft stop is available for this exact operation."
            }
            ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::NoExactTarget) => {
                "No exact interruptible operation is available."
            }
            ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress) => {
                "An exact soft-stop request is already in progress."
            }
            ExactSoftStopAvailability::Unavailable(
                ExactSoftStopUnavailable::AuthorityUnavailable,
            ) => "Exact operation authority is unavailable.",
        }
    }

    fn retain_feedback(&mut self, origin: ExactOperationOrigin, feedback: ExactStopFeedback) {
        if feedback.operation_origin().as_ref() != Some(&origin)
            || self.feedback.iter().any(|entry| entry.feedback == feedback)
        {
            return;
        }
        if self.feedback.len() < FEEDBACK_LIMIT {
            self.feedback
                .push(ExactStopFeedbackHandoff { origin, feedback });
        }
    }
}

impl Drop for ExactStatusControls {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl MainWindowShellRoot {
    fn status_mutation_gate(&self) -> Option<&'static str> {
        if self.shutdown_interaction_gated {
            Some("Application Exit is waiting for active work and durable state.")
        } else if self.ordinary_close_interaction_gated {
            Some("This window is waiting for its draft and durable close state.")
        } else if self.startup_interaction_gated() {
            Some("Beryl is preparing its windows.")
        } else {
            None
        }
    }

    pub(crate) fn mount_exact_status_worker(
        &mut self,
        worker: PublishedExactStopWorker,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_controls.worker.as_ref().is_some_and(|old| {
            old.worker_identity() == worker.worker_identity() && old.publication_current()
        }) {
            return;
        }
        self.close_stop_menu(window, cx);
        self.status_controls
            .generation
            .fetch_add(1, Ordering::AcqRel);
        self.status_controls.worker = Some(worker);
        self.status_controls.snapshot = ExactSelectedOperationSnapshot::unavailable();
        self.status_controls.selection = None;
        self.start_status_poll(window, cx);
        cx.notify();
    }

    fn status_selection(&self, app: &App) -> Option<MainWindowComposerSelectionIdentity> {
        let controller = self.controller.as_ref()?;
        let expected = match &controller.content {
            ShellContent::Acquired { selection, .. }
            | ShellContent::Restored { selection, .. }
            | ShellContent::Recovered { selection, .. } => selection,
            _ => return None,
        };
        let mount = controller.composer_mount.as_ref()?.read(app);
        let composer = mount.contribution()?;
        let selection = composer.read(app).selection_identity();
        if selection.window_id() != expected.window_id() || selection.claim() != expected.claim() {
            return None;
        }
        let worker = self.status_controls.worker.as_ref()?;
        let (home, generation, _) = worker.worker_identity();
        (worker.publication_current()
            && selection.binding().home_id() == home
            && selection.binding().home_generation() == generation)
            .then_some(selection)
    }

    pub(super) fn sync_status_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .status_controls
            .worker
            .as_ref()
            .is_none_or(|worker| !worker.publication_current())
        {
            let replacement = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                .and_then(|owner| owner.upgrade())
                .and_then(|owner| {
                    owner
                        .try_borrow()
                        .ok()
                        .and_then(|owner| owner.status_stop_worker())
                });
            if let Some(worker) = replacement {
                self.mount_exact_status_worker(worker, window, cx);
            }
        }
        let selection = self.status_selection(cx);
        if selection != self.status_controls.selection {
            self.close_stop_menu(window, cx);
            self.status_controls
                .generation
                .fetch_add(1, Ordering::AcqRel);
            self.status_controls.selection = selection;
            self.status_controls.snapshot = ExactSelectedOperationSnapshot::unavailable();
            self.status_controls.request_failure = None;
        }
        if !self.status_controls.menu_available()
            || self.status_controls.menu_anchor != self.status_controls.snapshot.origin
        {
            self.close_stop_menu(window, cx);
        }
    }

    fn start_status_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status_controls.poll.is_some() {
            return;
        }
        let background = cx.background_executor().clone();
        self.status_controls.poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let observation = this.update_in(cx, |root, window, cx| {
                    root.sync_status_controls(window, cx);
                    Some((
                        root.status_controls.worker.clone()?,
                        root.status_controls.selection?,
                        root.status_controls.generation.load(Ordering::Acquire),
                    ))
                });
                let Ok(observation) = observation else {
                    break;
                };
                if let Some((worker, selection, generation)) = observation {
                    let result = background
                        .spawn(async move { worker.selected_operation_snapshot(selection) })
                        .await;
                    let _ = this.update_in(cx, |root, window, cx| {
                        root.apply_status_observation(selection, generation, result, window, cx);
                    });
                }
                background.timer(Duration::from_millis(250)).await;
            }
        }));
    }

    fn apply_status_observation(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        generation: u64,
        mut result: ExactSelectedOperationSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_status_controls(window, cx);
        if self.status_controls.generation.load(Ordering::Acquire) != generation
            || self.status_selection(cx) != Some(selection)
        {
            return;
        }
        if self.status_controls.request_pending.is_some()
            && self.status_controls.request_pending == result.origin
        {
            result.stop =
                ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress);
        }
        if self.status_controls.snapshot.origin != result.origin {
            self.status_controls
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        self.status_controls.snapshot = result;
        self.sync_status_controls(window, cx);
        cx.notify();
    }

    fn close_stop_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status_controls.menu_anchor.take().is_some() {
            if self.status_controls.menu_focus.is_focused(window)
                && let Some(focus) = self
                    .status_controls
                    .return_focus
                    .take()
                    .and_then(|focus| focus.upgrade())
            {
                window.focus(&focus);
            }
            self.status_controls.return_focus = None;
            cx.notify();
        }
    }

    fn toggle_stop_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_status_controls(window, cx);
        if self.status_controls.menu_anchor.is_some() {
            self.close_stop_menu(window, cx);
            return;
        }
        if !self.status_controls.menu_available() {
            return;
        }
        self.status_controls.return_focus = window.focused(cx).map(|focus| focus.downgrade());
        self.status_controls.menu_anchor = self.status_controls.snapshot.origin.clone();
        window.focus(&self.status_controls.menu_focus);
        cx.notify();
    }

    fn activate_soft_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_status_controls(window, cx);
        if self.status_mutation_gate().is_some() {
            return;
        }
        if self.status_controls.menu_anchor != self.status_controls.snapshot.origin
            || self.status_controls.menu_anchor.is_none()
            || !self.status_controls.command_enabled()
        {
            return;
        }
        let Some(selection) = self.status_selection(cx) else {
            return;
        };
        let Some(worker) = self.status_controls.worker.clone() else {
            return;
        };
        let worker_identity = worker.worker_identity();
        let ExactSoftStopAvailability::Eligible(eligibility) = &self.status_controls.snapshot.stop
        else {
            return;
        };
        let eligibility: ExactSoftStopEligibility = eligibility.clone();
        let origin = eligibility.operation_origin();
        if Some(&origin) != self.status_controls.menu_anchor.as_ref() {
            return;
        }
        self.status_controls.request_pending = Some(origin.clone());
        self.status_controls.request_failure = None;
        self.status_controls.snapshot.stop =
            ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress);
        let fence = self.status_controls.generation.clone();
        let generation = fence.fetch_add(1, Ordering::AcqRel) + 1;
        self.close_stop_menu(window, cx);
        let work = cx.background_executor().spawn(async move {
            if fence.load(Ordering::Acquire) != generation {
                return Err(ExactStopRequestError::Revoked);
            }
            worker.request_selected_soft_stop(selection, &eligibility)
        });
        cx.spawn_in(window, async move |this, cx| {
            let feedback = work.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.apply_stop_feedback(
                    selection,
                    generation,
                    worker_identity,
                    origin,
                    feedback,
                    window,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    fn apply_stop_feedback(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        generation: u64,
        worker_identity: (
            beryl_model::BerylHomeId,
            beryl_home_store::HomeGeneration,
            crate::cas_projection::ProjectionServiceGeneration,
        ),
        origin: ExactOperationOrigin,
        feedback: Result<ExactStopFeedback, ExactStopRequestError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_status_controls(window, cx);
        let current = self.status_controls.generation.load(Ordering::Acquire) == generation
            && self.status_selection(cx) == Some(selection)
            && self.status_controls.snapshot.origin.as_ref() == Some(&origin)
            && self.status_controls.worker.as_ref().is_some_and(|worker| {
                worker.publication_current() && worker.worker_identity() == worker_identity
            });
        if self.status_controls.request_pending.as_ref() == Some(&origin) {
            self.status_controls.request_pending = None;
        }
        match feedback {
            Ok(feedback) => self.status_controls.retain_feedback(origin, feedback),
            Err(error) if current => {
                self.status_controls.request_failure = Some(match error {
                    ExactStopRequestError::Revoked => {
                        "The exact soft-stop request is no longer eligible."
                    }
                    ExactStopRequestError::Capacity => "The exact-stop feedback capacity is full.",
                })
            }
            _ => {}
        }
        if current {
            self.status_controls
                .generation
                .fetch_add(1, Ordering::AcqRel);
            self.status_controls.snapshot.stop =
                ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress);
        }
        self.sync_status_controls(window, cx);
        cx.notify();
    }

    pub(crate) fn exact_stop_feedback_handoff(&self) -> &[ExactStopFeedbackHandoff] {
        &self.status_controls.feedback
    }
}

#[cfg(feature = "test-faults")]
impl MainWindowShellRoot {
    pub fn test_mount_exact_status_worker(
        &mut self,
        worker: crate::cas_projection::ExactStopWorker,
        lifetime: std::sync::Weak<()>,
        session: beryl_state::SessionState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mount_exact_status_worker(
            PublishedExactStopWorker::for_test(worker, lifetime, session),
            window,
            cx,
        );
    }

    pub fn test_exact_status_diagnostics(
        &self,
    ) -> (
        &'static str,
        bool,
        bool,
        usize,
        Option<ExactStopFeedbackState>,
    ) {
        (
            self.status_controls.snapshot.state.label(),
            self.status_controls.menu_anchor.is_some(),
            self.status_controls.command_enabled() && self.status_mutation_gate().is_none(),
            self.status_controls.feedback.len(),
            self.status_controls
                .feedback()
                .map(|feedback| feedback.snapshot().state),
        )
    }

    pub fn test_exact_status_selection_present(&self, app: &App) -> bool {
        self.status_selection(app).is_some()
    }

    pub fn test_retain_exact_status_feedback(
        &mut self,
        feedback: ExactStopFeedback,
        cx: &mut Context<Self>,
    ) {
        let origin = feedback.operation_origin().unwrap();
        self.status_controls.retain_feedback(origin, feedback);
        self.status_controls
            .generation
            .fetch_add(1, Ordering::AcqRel);
        self.status_controls.snapshot.stop =
            ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress);
        cx.notify();
    }

    pub fn test_exact_status_observation_stamp(
        &self,
        app: &App,
    ) -> (MainWindowComposerSelectionIdentity, u64) {
        (
            self.status_selection(app).unwrap(),
            self.status_controls.generation.load(Ordering::Acquire),
        )
    }

    pub fn test_apply_exact_status_observation(
        &mut self,
        stamp: (MainWindowComposerSelectionIdentity, u64),
        result: ExactSelectedOperationSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_status_observation(stamp.0, stamp.1, result, window, cx);
    }

    pub fn test_apply_retained_stop_completion(
        &mut self,
        stamp: (MainWindowComposerSelectionIdentity, u64),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = &self.status_controls.feedback[0];
        let origin = entry.origin.clone();
        let feedback = entry.feedback.clone();
        let identity = self
            .status_controls
            .worker
            .as_ref()
            .unwrap()
            .worker_identity();
        self.apply_stop_feedback(stamp.0, stamp.1, identity, origin, Ok(feedback), window, cx);
    }
}
