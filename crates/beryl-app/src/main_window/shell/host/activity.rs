use super::*;
use crate::activity_service::{
    ActivityDemand, ActivityDemandOwner, ActivityHeightCommit, ActivityHeightError,
    ActivityInteractionGate, ActivityReadError, ActivityRetry, ActivityWindowReader,
};
use crate::main_window::MainWindowComposerSelectionIdentity;
use crate::widgets::activity_panel::{ActivityPanel, Event, Gate, InputGate, model, probe};
use std::sync::atomic::{AtomicBool, Ordering};

mod actor;
mod events;
mod layout;
pub(super) mod metadata;
mod presentation;

struct HeightCommand {
    value: f32,
    authority: Arc<ActivityInteractionGate>,
}

pub(super) struct Contribution {
    clock: u64,
    owner: u64,
    attempt: u64,
    selection: Option<MainWindowComposerSelectionIdentity>,
    reader: Option<ActivityWindowReader>,
    lifetime: Option<Arc<()>>,
    cancel: Option<Arc<AtomicBool>>,
    task: Option<gpui::Task<()>>,
    demands: Option<ActivityDemandOwner>,
    demand: Option<ActivityDemand>,
    retry: Option<ActivityRetry>,
    retry_command: Option<ActivityRetry>,
    feedback_key: u64,
    query: Option<model::Query>,
    widget: Option<Entity<ActivityPanel>>,
    subscription: Option<gpui::Subscription>,
    gate: Option<InputGate>,
    authority: Option<Arc<ActivityInteractionGate>>,
    visible: bool,
    preferred_height: f32,
    committed_height: f32,
    resizing: bool,
    resize_origin: Option<f32>,
    height_command: Option<HeightCommand>,
    height_result: Option<Arc<crate::activity_service::ActivityHeightFlight>>,
    height_condition: crate::main_window::NoticeConditionId,
    height_notice_revision: u64,
    #[cfg(feature = "test-faults")]
    fixture_reader: Option<ActivityWindowReader>,
}

impl Contribution {
    pub(super) fn new() -> Self {
        Self {
            clock: 0,
            owner: 0,
            attempt: 0,
            selection: None,
            reader: None,
            lifetime: None,
            cancel: None,
            task: None,
            demands: None,
            demand: None,
            retry: None,
            retry_command: None,
            feedback_key: 0,
            query: None,
            widget: None,
            subscription: None,
            gate: None,
            authority: None,
            visible: false,
            preferred_height: model::DEFAULT_HEIGHT,
            committed_height: model::DEFAULT_HEIGHT,
            resizing: false,
            resize_origin: None,
            height_command: None,
            height_result: None,
            height_condition: crate::main_window::NoticeConditionId::new(),
            height_notice_revision: 0,
            #[cfg(feature = "test-faults")]
            fixture_reader: None,
        }
    }

    fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.lifetime = None;
        self.demands = None;
        self.demand = None;
        self.retry = None;
        self.retry_command = None;
        self.height_command = None;
        self.task = None;
        self.gate = None;
        self.authority = None;
    }
}

impl Drop for Contribution {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl MainWindowShellRoot {
    #[cfg(feature = "test-faults")]
    pub fn test_mount_activity(
        &mut self,
        harness: &crate::ActivityWindowHarness,
        cx: &mut Context<Self>,
    ) {
        self.activity.fixture_reader = Some(harness.window_reader());
        cx.notify();
    }

    #[cfg(feature = "test-faults")]
    pub fn test_focus_activity_resize(&self, window: &mut Window, app: &gpui::App) {
        self.activity
            .widget
            .as_ref()
            .expect("mounted Activity")
            .read(app)
            .test_focus_resize(window);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_activity_allocation_for_content(
        &self,
        content: gpui::Pixels,
        window: &Window,
    ) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
        let metrics = &self.controller.as_ref().expect("selected shell").metrics;
        let viewport = window.viewport_size().height;
        let composer = metrics.composer_height(content, viewport);
        (
            composer,
            self.activity_allocation(metrics, composer, viewport),
            viewport - metrics.pinned_height(),
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_activity_diagnostics(
        &self,
        app: &gpui::App,
    ) -> Option<crate::activity_panel_native_test_support::Diagnostics> {
        self.activity
            .widget
            .as_ref()
            .map(|widget| widget.read(app).diagnostics())
    }

    pub(crate) fn diagnostic_activity_facts(&self, app: &gpui::App) -> serde_json::Value {
        let Some(widget) = self.activity.widget.as_ref() else {
            return serde_json::json!({ "visible": false, "workerPending": self.activity.task.is_some() });
        };
        let facts = widget.read(app).diagnostics();
        serde_json::json!({
            "visible": self.activity.visible, "workerPending": self.activity.task.is_some(),
            "instance": facts.instance, "query": facts.query, "height": facts.height,
            "resizing": facts.resizing, "total": facts.total, "residentPages": facts.resident_pages,
            "pendingPages": facts.pending_pages, "realizedRows": facts.realized_rows,
            "visibleKeys": facts.visible_keys, "overscan": facts.overscan, "rowHeight": facts.row_height,
            "offset": facts.offset, "topAttached": facts.top_attached,
            "reconciliations": facts.reconciliations, "tooltip": facts.tooltip, "inert": facts.inert,
        })
    }

    pub(super) fn activity_visible(&self) -> bool {
        self.activity.visible && self.activity.widget.is_some()
    }
    fn activity_reader(&self, cx: &Context<Self>) -> Option<ActivityWindowReader> {
        #[cfg(feature = "test-faults")]
        if let Some(reader) = &self.activity.fixture_reader {
            return Some(reader.clone());
        }
        crate::running_owner::RunningProcessOwner::mounted_owner(cx)
            .and_then(|owner| owner.upgrade())
            .and_then(|owner| owner.borrow().activity_reader())
    }

    pub(super) fn suspend_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.activity.cancel();
        if let Some(widget) = self.activity.widget.clone() {
            widget.update(cx, |widget, cx| widget.set_inert(window, cx));
        }
        if self.activity.resizing {
            self.restore_activity_resize();
        }
    }

    pub(super) fn sync_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let gated = self.shutdown_interaction_gated
            || self.ordinary_close_interaction_gated
            || self.startup_interaction_gated()
            || self.running_threads.has_activation_custody();
        if gated {
            self.suspend_activity(window, cx);
            return;
        }
        let selected = self
            .cached_running_selection(cx)
            .map(|(selected, _)| selected);
        let reader = self.activity_reader(cx);
        let (Some(selected), Some(reader)) = (selected, reader) else {
            self.suspend_activity(window, cx);
            self.activity.visible = false;
            self.activity.widget = None;
            self.activity.subscription = None;
            self.activity.query = None;
            self.activity.selection = None;
            self.activity.reader = None;
            return;
        };
        let changed = self.activity.selection != Some(selected)
            || self
                .activity
                .reader
                .as_ref()
                .is_none_or(|current| !current.same_publication(&reader));
        if changed {
            self.suspend_activity(window, cx);
            if self
                .activity
                .height_result
                .as_ref()
                .is_some_and(|flight| Some(flight.generation()) != reader.generation())
            {
                self.activity.height_result = None;
            }
            self.activity.selection = Some(selected);
            self.activity.reader = Some(reader.clone());
            self.activity.query = None;
            self.activity.widget = None;
            self.activity.subscription = None;
            self.activity.visible = false;
        }
        if self.activity.task.is_none() {
            let Some(attempt) = self.activity.clock.checked_add(1) else {
                return;
            };
            self.activity.clock = attempt;
            self.activity.attempt = attempt;
            if changed || self.activity.owner == 0 {
                self.activity.owner = attempt;
            }
            self.start_activity(selected, reader, attempt, window, cx);
        }
        if self
            .activity
            .gate
            .as_ref()
            .is_some_and(|gate| probe(gate) == Gate::Retired)
        {
            if let Some(widget) = self.activity.widget.clone() {
                widget.update(cx, |widget, cx| widget.set_inert(window, cx));
            }
            if self.activity.resizing {
                self.restore_activity_resize();
            }
        }
    }

    fn restore_activity_resize(&mut self) {
        self.activity.preferred_height = self
            .activity
            .resize_origin
            .take()
            .unwrap_or(self.activity.committed_height);
        self.activity.resizing = false;
    }
}

fn gate(source: ActivityInteractionGate) -> InputGate {
    Rc::new(move |action| match source.with_current(action) {
        Ok(()) => Gate::Current,
        Err(error) if busy(&error) => Gate::Busy,
        Err(_) => Gate::Retired,
    })
}

fn busy(error: &ActivityReadError) -> bool {
    matches!(
        error,
        ActivityReadError::Busy
            | ActivityReadError::RequestPending
            | ActivityReadError::Runtime(crate::cas_projection::RuntimeActivityReadError::Busy)
            | ActivityReadError::Coherence(beryl_home_store::HomeCoherenceError::Busy)
            | ActivityReadError::Observation(
                beryl_home_store::HomeObservedCoherenceError::Coherence(
                    beryl_home_store::HomeCoherenceError::Busy
                )
            )
            | ActivityReadError::Observation(
                beryl_home_store::HomeObservedCoherenceError::Observation(
                    beryl_home_store::HomeMutationObservationError::Busy
                        | beryl_home_store::HomeMutationObservationError::Stale
                )
            )
    )
}
