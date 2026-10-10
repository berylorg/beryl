use super::*;
use crate::cas_projection::{
    ContextCompactionFeedback, ManualCompactionAvailability, ManualCompactionEligibility,
};
use crate::widgets::anchored_context_menu::{
    self as widget, AnchoredContextMenu, MenuCollection, MenuEvent, MenuIntent, MenuRow,
    MenuRowKind,
};

pub(super) struct ManualCompactionControls {
    #[cfg(feature = "test-faults")]
    rendered: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub(super) availability: ManualCompactionAvailability,
    pub(super) anchor: Option<Option<ManualCompactionEligibility>>,
    popup: Option<AnchoredContextMenu>,
    serial: u64,
    pub(super) pending: bool,
    pub(super) pending_order: Option<u64>,
    pub(super) feedback: Vec<CompactionFeedbackHandoff>,
}

pub(in crate::main_window::shell) struct CompactionFeedbackHandoff {
    pub(in crate::main_window::shell) order: u64,
    pub(in crate::main_window::shell) feedback: ContextCompactionFeedback,
}

impl Default for ManualCompactionControls {
    fn default() -> Self {
        Self {
            #[cfg(feature = "test-faults")]
            rendered: Default::default(),
            availability: ManualCompactionAvailability::Unavailable("Runtime is unavailable."),
            anchor: None,
            popup: None,
            serial: 0,
            pending: false,
            pending_order: None,
            feedback: Vec::new(),
        }
    }
}

impl MainWindowShellRoot {
    #[cfg(feature = "test-faults")]
    pub fn test_compaction_command_enabled(&self) -> bool {
        self.compaction_enabled()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_compaction_row_bounds(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.status_controls.compaction.rendered.get()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_compaction_feedback_states(
        &self,
    ) -> Vec<crate::cas_projection::ContextCompactionFeedbackState> {
        self.status_controls
            .compaction
            .feedback
            .iter()
            .map(|entry| entry.feedback.snapshot().state)
            .collect()
    }
    pub(in crate::main_window::shell) fn compaction_feedback_handoff(
        &self,
    ) -> &[CompactionFeedbackHandoff] {
        &self.status_controls.compaction.feedback
    }

    pub(in crate::main_window::shell) fn acknowledge_compaction_feedback(
        &mut self,
        feedback: &ContextCompactionFeedback,
    ) {
        if feedback.snapshot().state.resolved() {
            self.status_controls
                .compaction
                .feedback
                .retain(|entry| &entry.feedback != feedback);
        }
    }

    pub(super) fn apply_compaction_observation(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        generation: u64,
        availability: ManualCompactionAvailability,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_controls.generation.load(Ordering::Acquire) != generation
            || self.status_selection(cx) != Some(selection)
        {
            return;
        }
        let anchored = self.status_controls.compaction.anchor.as_ref().is_none_or(|anchor| {
            anchor.as_ref().is_none_or(|anchor| matches!(&availability, ManualCompactionAvailability::Eligible(eligible) if anchor.same_origin(eligible)))
        });
        if matches!(self.status_controls.compaction.anchor, Some(None))
            && let ManualCompactionAvailability::Eligible(eligibility) = &availability
        {
            self.status_controls.compaction.anchor = Some(Some(eligibility.clone()));
        }
        self.status_controls.compaction.availability = availability;
        if !anchored {
            self.close_compaction_menu(window, cx);
        }
        self.reconcile_compaction_menu(window);
    }

    pub(super) fn compaction_reason(&self) -> &'static str {
        if self.status_controls.snapshot.operation_active {
            return "Compaction is unavailable while the turn is active.";
        }
        if self.status_controls.compaction.pending {
            return "Requesting exact context compaction…";
        }
        if self.status_controls.feedback_budget() >= FEEDBACK_LIMIT {
            return "The bounded operation feedback capacity is full.";
        }
        match &self.status_controls.compaction.availability {
            ManualCompactionAvailability::Eligible(_) => "Compact the selected thread context.",
            ManualCompactionAvailability::Unavailable(reason) => reason,
        }
    }

    pub(super) fn compaction_menu_available(&self) -> bool {
        self.status_controls.selection.is_some()
            && !self.status_controls.snapshot.operation_active
            && self
                .status_controls
                .worker
                .as_ref()
                .is_some_and(|worker| worker.publication_current())
            && self.compaction_reason() != "Runtime is unavailable."
    }

    pub(super) fn compaction_enabled(&self) -> bool {
        self.compaction_menu_available()
            && !self.status_controls.compaction.pending
            && self.status_controls.feedback_budget() < FEEDBACK_LIMIT
            && self.status_mutation_gate().is_none()
            && matches!(
                self.status_controls.compaction.availability,
                ManualCompactionAvailability::Eligible(_)
            )
    }

    pub(super) fn close_compaction_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.status_controls.compaction.anchor.take().is_some() {
            #[cfg(feature = "test-faults")]
            self.status_controls.compaction.rendered.set(None);
            if let Some(mut popup) = self.status_controls.compaction.popup.take() {
                popup.dismiss(window, &self.status_controls.menu_focus);
            }
            cx.notify();
        }
    }

    pub(super) fn toggle_compaction_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_status_controls(window, cx);
        if self.status_controls.compaction.anchor.is_some() {
            self.close_compaction_menu(window, cx);
            return;
        }
        if !self.compaction_menu_available() {
            return;
        }
        let eligibility = match &self.status_controls.compaction.availability {
            ManualCompactionAvailability::Eligible(eligibility) => Some(eligibility.clone()),
            _ => None,
        };
        self.close_stop_menu(window, cx);
        self.status_controls.compaction.anchor = Some(eligibility);
        self.status_controls.compaction.serial = self
            .status_controls
            .compaction
            .serial
            .checked_add(1)
            .expect("context menu invocation");
        self.status_controls.compaction.popup = Some(AnchoredContextMenu::open(
            self.status_controls.compaction.serial,
            window,
            cx,
        ));
        self.reconcile_compaction_menu(window);
        cx.notify();
    }

    fn reconcile_compaction_menu(&mut self, window: &Window) {
        let facts = compact_row(self);
        if let Some(popup) = self.status_controls.compaction.popup.as_mut() {
            popup.reconcile(&facts);
            popup.realize(&[facts.row(0)], 0..1, window);
        }
    }

    pub(super) fn activate_compaction(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_status_controls(window, cx);
        if !self.compaction_enabled() {
            return;
        }
        let Some(Some(eligibility)) = self.status_controls.compaction.anchor.clone() else {
            return;
        };
        let Some(worker) = self.status_controls.worker.clone() else {
            return;
        };
        self.status_controls.compaction.pending = true;
        let order = self.status_controls.allocate_feedback_order();
        self.status_controls.compaction.pending_order = Some(order);
        self.close_compaction_menu(window, cx);
        let background = cx.background_executor().clone();
        let work = background.spawn(async move { worker.prepare_manual_compaction(&eligibility) });
        cx.spawn_in(window, async move |this, cx| {
            let request = work.await;
            let feedback = request.as_ref().ok().map(|request| request.feedback());
            let _ = this.update_in(cx, |root, window, cx| {
                root.status_controls.compaction.pending = false;
                root.status_controls.compaction.pending_order = None;
                if let Some(feedback) = feedback {
                    root.status_controls
                        .compaction
                        .feedback
                        .push(CompactionFeedbackHandoff { order, feedback });
                } else {
                    root.status_controls.compaction.availability =
                        ManualCompactionAvailability::Unavailable(
                            "The exact compaction request is no longer available.",
                        );
                }
                root.sync_notices(window, cx);
                cx.notify();
            });
            if let Ok(request) = request {
                background.spawn(async move { request.execute() }).await;
            }
        })
        .detach();
        cx.notify();
    }
}

struct CompactRow(MenuRow);

impl MenuCollection for CompactRow {
    fn len(&self) -> usize {
        1
    }
    fn row(&self, index: usize) -> MenuRow {
        assert_eq!(index, 0);
        self.0.clone()
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        (id == "main-window-compact").then_some(0)
    }
}

fn compact_row(root: &MainWindowShellRoot) -> CompactRow {
    CompactRow(MenuRow {
        index: 0,
        id: Some(Arc::from("main-window-compact")),
        label: "Compact".into(),
        kind: MenuRowKind::Command,
        selected: false,
        enabled: root.compaction_enabled(),
        disabled_reason: Some(
            root.status_mutation_gate()
                .unwrap_or_else(|| root.compaction_reason())
                .into(),
        ),
        selector: "main-window-compact".into(),
    })
}

fn input(
    root: &mut MainWindowShellRoot,
    event: MenuEvent,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) {
    let facts = compact_row(root);
    let Some(popup) = root.status_controls.compaction.popup.as_mut() else {
        return;
    };
    for intent in popup.input(event, &facts) {
        match intent {
            MenuIntent::Dismiss => root.close_compaction_menu(window, cx),
            MenuIntent::Focus(_) => cx.notify(),
            MenuIntent::Activate { .. } => root.activate_compaction(window, cx),
        }
    }
}

pub(super) fn render_menu(
    root: &MainWindowShellRoot,
    window: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> Option<AnyElement> {
    let popup = root.status_controls.compaction.popup.as_ref()?;
    let anchor = gpui::Bounds::new(
        gpui::point(px(171.), window.viewport_size().height - px(28.)),
        gpui::size(px(180.), px(28.)),
    );
    #[cfg(feature = "test-faults")]
    root.status_controls
        .compaction
        .rendered
        .set(Some(popup.bounds(anchor, 1, window.viewport_size())));
    Some(widget::render_static(
        popup,
        anchor,
        vec![compact_row(root).row(0)],
        super::super::model_controls::menu_colors(root),
        "main-window-context-menu",
        input,
        window,
        cx,
    ))
}
