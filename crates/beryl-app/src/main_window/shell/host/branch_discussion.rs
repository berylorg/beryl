use super::*;

mod activation;
mod lifecycle;
#[cfg(all(test, feature = "test-faults"))]
mod test_access;
mod worker;
use crate::discussion_creation::{
    CreatedDiscussion, DiscussionCreationAuditOutcome, DiscussionCreationOutcome,
};
use crate::syndic_transcript::{DiscussionCreationRequested, SyndicTranscriptPanel};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BranchStage {
    Creating,
    Reconciling,
    Activating,
    Retry,
    NotCommitted,
    Unavailable,
    Complete,
}

struct CapturedBranchOperation {
    captured: DiscussionCreationRequested,
    request: syndic_storage::CreateDiscussion,
    reader: Option<crate::app_services::PublishedRunningThreadsReader>,
    home_id: beryl_model::BerylHomeId,
    cancellation: beryl_home_store::CommandCancellation,
    outcome: Option<Result<DiscussionCreationOutcome, String>>,
    resolution: Option<Result<DiscussionCreationAuditOutcome, String>>,
    stage: BranchStage,
    created: Option<CreatedDiscussion>,
    error: Option<String>,
    notice: Option<crate::main_window::NoticeRecordToken>,
    #[cfg(all(test, feature = "test-faults"))]
    activation_attempts: u32,
    #[cfg(all(test, feature = "test-faults"))]
    before_creation: Option<Box<dyn FnOnce(&beryl_home_store::CommandCancellation) + Send>>,
    #[cfg(all(test, feature = "test-faults"))]
    before_reconciliation: Option<Box<dyn FnOnce(CreatedDiscussion) + Send>>,
}

#[derive(Default)]
pub(super) struct BranchDiscussionContribution {
    operation: Option<Arc<Mutex<CapturedBranchOperation>>>,
    retained: Vec<Arc<Mutex<CapturedBranchOperation>>>,
    task: Option<gpui::Task<()>>,
    qualification_task: Option<gpui::Task<()>>,
    notice: Option<crate::main_window::NoticeRecordToken>,
    #[cfg(all(test, feature = "test-faults"))]
    before_creation: Option<Box<dyn FnOnce(&beryl_home_store::CommandCancellation) + Send>>,
    #[cfg(all(test, feature = "test-faults"))]
    before_reconciliation: Option<Box<dyn FnOnce(CreatedDiscussion) + Send>>,
}

pub(super) fn subscriptions(
    transcript: &Entity<SyndicTranscriptPanel>,
    window: &mut Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> (gpui::Subscription, gpui::Subscription, gpui::Subscription) {
    let qualification = cx.subscribe_in(
        transcript,
        window,
        |root,
         source,
         event: &crate::syndic_transcript::DiscussionSourceQualificationRequested,
         window,
         cx| {
            if source != &root.running_threads.transcript
                || root.running_threads.views_retired
                || root.running_threads.reads_suspended
            {
                return;
            }
            root.qualify_captured_branch(source.clone(), event.0.clone(), window, cx);
        },
    );
    let create = cx.subscribe_in(
        transcript,
        window,
        |root, source, event: &DiscussionCreationRequested, window, cx| {
            if source != &root.running_threads.transcript
                || root.running_threads.views_retired
                || root.running_threads.reads_suspended
                || root.running_selection_interaction_gated(cx)
                || root.ordinary_close_interaction_gated
                || root.shutdown_interaction_gated
                || root.startup_interaction_gated()
                || !source.read(cx).quote_request_current(
                    event.request,
                    event.activation,
                    event.presentation,
                    window,
                )
            {
                return;
            }
            root.begin_captured_branch(event.clone(), window, cx);
        },
    );
    let retry = cx.subscribe_in(
        transcript,
        window,
        |root,
         source,
         event: &crate::syndic_transcript::DiscussionActivationRetryRequested,
         window,
         cx| {
            if source == &root.running_threads.transcript
                && source
                    .read(cx)
                    .discussion_retry_request_current(&event.0, window)
            {
                root.retry_captured_branch_activation(&event.0.captured, window, cx);
            }
        },
    );
    (create, retry, qualification)
}

impl MainWindowShellRoot {
    pub(super) fn subscribe_branch_discussion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (create, retry, qualification) =
            subscriptions(&self.running_threads.transcript, window, cx);
        self.running_threads.branch_subscription = Some(create);
        self.running_threads.branch_retry_subscription = Some(retry);
        self.running_threads.branch_qualification_subscription = Some(qualification);
    }

    pub(super) fn captured_branch_pending(&self) -> bool {
        self.branch_discussion
            .operation
            .as_ref()
            .is_some_and(|operation| {
                operation.try_lock().map_or(true, |operation| {
                    matches!(
                        operation.stage,
                        BranchStage::Creating | BranchStage::Reconciling
                    )
                })
            })
    }

    fn begin_captured_branch(
        &mut self,
        captured: DiscussionCreationRequested,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(operation) = self.branch_discussion.operation.clone() {
            let mut state = operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if matches!(
                state.stage,
                BranchStage::Creating | BranchStage::Reconciling | BranchStage::Activating
            ) || (state.captured.request == captured.request
                && state.captured.activation == captured.activation
                && state.captured.presentation == captured.presentation)
                || (state.captured.captured == captured.captured
                    && state.stage != BranchStage::NotCommitted)
            {
                return;
            }
            if matches!(state.stage, BranchStage::Retry | BranchStage::Unavailable) {
                if self.branch_discussion.retained.len()
                    >= crate::discussion_creation::DISCUSSION_CREATION_SCOPE_CAPACITY.get()
                {
                    drop(state);
                    self.report_captured_branch_failure(
                        "Captured discussion evidence capacity is full.",
                        window,
                        cx,
                    );
                    return;
                }
                state.reader = None;
                self.branch_discussion.retained.push(operation.clone());
            }
            self.branch_discussion.operation = None;
        }
        if self.branch_discussion.retained.iter().any(|operation| {
            operation
                .try_lock()
                .map_or(true, |state| state.captured.captured == captured.captured)
        }) {
            return;
        }
        let Some((selection, _)) = self.cached_running_selection(cx) else {
            return;
        };
        if self.running_threads.transcript_claim != Some(selection.claim())
            || captured.captured.source.thread_id() != selection.claim().thread_id()
        {
            return;
        }
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        let request = match discussion_request() {
            Ok(request) => request,
            Err(error) => {
                self.report_captured_branch_failure(&error, window, cx);
                return;
            }
        };
        let operation = Arc::new(Mutex::new(CapturedBranchOperation {
            captured,
            request,
            home_id: reader.branch_home_id(),
            reader: Some(reader),
            cancellation: beryl_home_store::CommandCancellation::new(),
            outcome: None,
            resolution: None,
            stage: BranchStage::Creating,
            created: None,
            error: None,
            notice: None,
            #[cfg(all(test, feature = "test-faults"))]
            activation_attempts: 0,
            #[cfg(all(test, feature = "test-faults"))]
            before_creation: self.branch_discussion.before_creation.take(),
            #[cfg(all(test, feature = "test-faults"))]
            before_reconciliation: self.branch_discussion.before_reconciliation.take(),
        }));
        self.branch_discussion.operation = Some(operation.clone());
        self.publish_captured_branch_state(BranchStage::Creating, cx);
        self.run_captured_branch(operation, false, window, cx);
    }

    fn publish_captured_branch_state(&self, stage: BranchStage, cx: &mut Context<Self>) {
        let (label, reason, retry) = branch_menu_state(stage);
        let captured = self
            .branch_discussion
            .operation
            .as_ref()
            .and_then(|operation| {
                operation
                    .try_lock()
                    .ok()
                    .map(|state| state.captured.captured.clone())
            });
        self.running_threads.transcript.update(cx, |panel, cx| {
            panel.set_discussion_creation_state(captured, label, reason, retry, cx)
        });
    }

    fn report_captured_branch_failure(
        &mut self,
        error: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operation = self.branch_discussion.operation.clone();
        let (previous, unavailable) = if let Some(operation) = &operation {
            let mut state = operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.notice.take(), state.stage == BranchStage::Unavailable)
        } else {
            (self.branch_discussion.notice.take(), false)
        };
        let token = self.publish_captured_branch_failure(previous, error, unavailable);
        if let Some(operation) = operation {
            operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .notice = token;
        } else {
            self.branch_discussion.notice = token;
        }
        self.sync_notices(window, cx);
    }

    fn sync_captured_branch_menu(&self, cx: &mut Context<Self>) {
        let captured = self
            .running_threads
            .transcript
            .read(cx)
            .discussion_menu_source();
        let state = captured.as_ref().and_then(|captured| {
            self.branch_discussion
                .operation
                .iter()
                .chain(&self.branch_discussion.retained)
                .find_map(|operation| {
                    let state = operation.try_lock().ok()?;
                    (state.captured.captured == *captured).then_some(state.stage)
                })
        });
        if let Some(stage) = state {
            let (label, reason, retry) = branch_menu_state(stage);
            self.running_threads.transcript.update(cx, |panel, cx| {
                panel.set_discussion_creation_state(captured, label, reason, retry, cx)
            });
        }
    }
}

fn branch_menu_state(stage: BranchStage) -> (Option<&'static str>, Option<&'static str>, bool) {
    match stage {
        BranchStage::Creating => (
            Some("Creating discussion…"),
            Some("The captured discussion is being created."),
            false,
        ),
        BranchStage::Reconciling => (
            Some("Reconciling discussion…"),
            Some("The exact creation outcome is being reconciled."),
            false,
        ),
        BranchStage::Activating => (
            Some("Opening discussion…"),
            Some("The created discussion is being opened."),
            false,
        ),
        BranchStage::Retry => (Some("Retry"), None, true),
        BranchStage::Unavailable => (
            Some("Unavailable"),
            Some(
                "The exact discussion operation is unavailable. Its evidence is retained for same-home recovery.",
            ),
            false,
        ),
        BranchStage::NotCommitted | BranchStage::Complete => (None, None, false),
    }
}

fn discussion_request() -> Result<syndic_storage::CreateDiscussion, String> {
    let mut thread = [0; 16];
    let mut draft = [0; 16];
    getrandom::fill(&mut thread).map_err(|error| error.to_string())?;
    getrandom::fill(&mut draft).map_err(|error| error.to_string())?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    Ok(syndic_storage::CreateDiscussion::new(
        beryl_model::SyndicThreadId::from_bytes(thread),
        beryl_model::SyndicDraftId::from_bytes(draft),
        syndic_storage::SyndicTimestamp::from_unix_millis(
            u64::try_from(at).map_err(|error| error.to_string())?,
        ),
        syndic_storage::DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1)
            .ok_or("Discussion history policy is invalid")?,
    ))
}
