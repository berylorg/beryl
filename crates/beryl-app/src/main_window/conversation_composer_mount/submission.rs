use std::{
    num::NonZeroU64,
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_home_store::{CommandCancellation, FreeSpaceOutcome, TurnStartAdmissionRequirement};
use beryl_model::{AssetReferenceSetId, SyndicDraftId, SyndicItemId};
use beryl_state::AssetReferenceSetStagingAuthority;
use gpui::{BackgroundExecutor, Context, Task, Window};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftEditorCandidateSessionIdV1,
    DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1, SyndicTimestamp,
};

use super::MainWindowConversationComposerMount;
use crate::composer_host::{
    ComposerHostActivationRequest, ComposerHostMarkerSealAuthority, ComposerHostSubmissionRequest,
    ComposerHostSubmissionStage, ComposerHostSubmissionTicket,
};
use crate::main_window::{
    MainWindowComposerActivationReceipt, MainWindowComposerAutosaveCaptureRequirement,
    MainWindowComposerSelectionIdentity, MainWindowComposerSubmissionAdvance,
};

#[cfg(feature = "test-faults")]
mod test_faults;
#[cfg(feature = "test-faults")]
pub use test_faults::{
    MainWindowComposerSubmissionAdvanceTestRelease, MainWindowComposerSubmissionAdvanceTestToken,
    MainWindowComposerSubmissionTestAdvance,
    MainWindowConversationComposerSubmissionTestDiagnostics,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerSubmissionStatus {
    Idle,
    Preparing,
    Pending(ComposerHostSubmissionStage),
    Reconciling,
    DirectAdmissionDenied(FreeSpaceOutcome),
    NotCommitted,
    Cancelled,
    Failed,
    OpeningSuccessor,
    Unavailable,
}

pub struct MainWindowComposerSubmissionRequestSource {
    turn_start_admission_requirement: TurnStartAdmissionRequirement,
}

impl MainWindowComposerSubmissionRequestSource {
    pub const fn new(turn_start_admission_requirement: TurnStartAdmissionRequirement) -> Self {
        Self {
            turn_start_admission_requirement,
        }
    }

    fn prepare(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        capture_requirement: MainWindowComposerAutosaveCaptureRequirement,
    ) -> Result<PreparedMountedSubmission, String> {
        let next_draft_id = SyndicDraftId::from_bytes(fresh_bytes()?);
        let admitted_at = current_timestamp()?;
        let request = ComposerHostSubmissionRequest::new(
            next_draft_id,
            SyndicItemId::from_bytes(fresh_bytes()?),
            DraftComposerMaterializationOperationIdV1::from_bytes(fresh_bytes()?),
            DraftPieceOperationIdV1::from_bytes(fresh_bytes()?),
            admitted_at,
            self.turn_start_admission_requirement,
        );
        let successor_presentation = selection
            .binding()
            .presentation_generation()
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| "conversation composer presentation generation exhausted".to_owned())?;
        Ok(PreparedMountedSubmission {
            request,
            publication_operation_id: DraftPieceOperationIdV1::from_bytes(fresh_bytes()?),
            marker_authority: match capture_requirement {
                MainWindowComposerAutosaveCaptureRequirement::ChangedMarkers => {
                    Some(ComposerHostMarkerSealAuthority::new(
                        DraftMarkerSealOperationIdV1::from_bytes(fresh_bytes()?),
                        AssetReferenceSetStagingAuthority::new(
                            AssetReferenceSetId::from_bytes(fresh_bytes()?),
                            fresh_bytes()?,
                        ),
                    ))
                }
                MainWindowComposerAutosaveCaptureRequirement::Clean
                | MainWindowComposerAutosaveCaptureRequirement::UnchangedMarkers => None,
            },
            published_at: admitted_at,
            successor_request: ComposerHostActivationRequest::new(
                selection.claim().thread_id(),
                DraftEditorCandidateSessionIdV1::from_bytes(fresh_bytes()?),
                DraftPieceOperationIdV1::from_bytes(fresh_bytes()?),
                successor_presentation,
                None,
                Box::new([]),
            ),
            successor_retirement_operation_id: DraftPieceOperationIdV1::from_bytes(fresh_bytes()?),
            next_draft_id,
        })
    }
}

pub(super) struct MainWindowConversationComposerSubmission {
    request_source: MainWindowComposerSubmissionRequestSource,
    executor: BackgroundExecutor,
    generation: u64,
    status: MainWindowConversationComposerSubmissionStatus,
    active: Option<Box<ActiveMountedSubmission>>,
    task: Option<Task<()>>,
    #[cfg(feature = "test-faults")]
    test_advance_gate: Option<test_faults::SubmissionAdvanceTestGate>,
    #[cfg(feature = "test-faults")]
    test_advance: Option<MainWindowComposerSubmissionTestAdvance>,
    #[cfg(feature = "test-faults")]
    test_fail_successor_after_readiness: bool,
}

#[derive(Clone)]
struct PreparedMountedSubmission {
    request: ComposerHostSubmissionRequest,
    publication_operation_id: DraftPieceOperationIdV1,
    marker_authority: Option<ComposerHostMarkerSealAuthority>,
    published_at: SyndicTimestamp,
    successor_request: ComposerHostActivationRequest,
    successor_retirement_operation_id: DraftPieceOperationIdV1,
    next_draft_id: SyndicDraftId,
}

struct ActiveMountedSubmission {
    selection: MainWindowComposerSelectionIdentity,
    prepared: PreparedMountedSubmission,
    ticket: Option<ComposerHostSubmissionTicket>,
    cancellation: CommandCancellation,
    terminal_after_cancel: Option<MainWindowConversationComposerSubmissionStatus>,
    successor: Option<SubmissionSuccessor>,
}

#[derive(Clone, Copy)]
struct SubmissionSuccessor {
    receipt: MainWindowComposerActivationReceipt,
    predecessor: MainWindowComposerSelectionIdentity,
    successor: MainWindowComposerSelectionIdentity,
}

impl MainWindowConversationComposerSubmission {
    pub(super) fn new(
        request_source: MainWindowComposerSubmissionRequestSource,
        executor: BackgroundExecutor,
    ) -> Self {
        Self {
            request_source,
            executor,
            generation: 0,
            status: MainWindowConversationComposerSubmissionStatus::Idle,
            active: None,
            task: None,
            #[cfg(feature = "test-faults")]
            test_advance_gate: None,
            #[cfg(feature = "test-faults")]
            test_advance: None,
            #[cfg(feature = "test-faults")]
            test_fail_successor_after_readiness: false,
        }
    }

    pub(super) const fn status(&self) -> MainWindowConversationComposerSubmissionStatus {
        self.status
    }

    fn clear_active(&mut self, status: MainWindowConversationComposerSubmissionStatus) {
        self.active = None;
        self.task = None;
        self.status = status;
    }
}

impl MainWindowConversationComposerMount {
    pub(super) fn cancel_mounted_submission(&mut self) -> bool {
        let Some(active) = self.submission.active.as_mut() else {
            return false;
        };
        if active.successor.is_some() {
            return true;
        }
        active.terminal_after_cancel =
            Some(MainWindowConversationComposerSubmissionStatus::Cancelled);
        active.cancellation.cancel();
        true
    }

    pub(super) fn begin_mounted_submission(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.submission.active.is_some()
            || matches!(
                self.submission.status,
                MainWindowConversationComposerSubmissionStatus::Unavailable
            )
        {
            return Ok(());
        }
        if self.service.selected_identity() != Some(selection) {
            return Ok(());
        }
        let generation =
            self.submission.generation.checked_add(1).ok_or_else(|| {
                "conversation composer submission generation exhausted".to_owned()
            })?;
        let capture_requirement = self.service.autosave_capture_requirement(selection)?;
        let prepared = self
            .submission
            .request_source
            .prepare(selection, capture_requirement)?;
        self.submission.generation = generation;
        self.submission.status = MainWindowConversationComposerSubmissionStatus::Preparing;
        self.submission.active = Some(Box::new(ActiveMountedSubmission {
            selection,
            prepared,
            ticket: None,
            cancellation: CommandCancellation::new(),
            terminal_after_cancel: None,
            successor: None,
        }));
        self.suspend_autosave()?;
        self.continue_submission_start(generation, window, cx)
    }

    fn continue_submission_start(
        &mut self,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.submission.generation != generation {
            return Ok(());
        }
        let selection = self
            .submission
            .active
            .as_ref()
            .ok_or_else(|| "conversation composer submission disappeared".to_owned())?
            .selection;
        if !self.fence_contribution(selection, window, cx)? {
            cx.defer_in(window, move |this, window, cx| {
                if this
                    .continue_submission_start(generation, window, cx)
                    .is_err()
                {
                    this.finish_submission_failure(window, cx);
                }
            });
            return Ok(());
        }
        let request = self.submission.active.as_ref().unwrap().prepared.request;
        match self.service.begin_submission(selection, request) {
            Ok(ticket) => {
                self.submission.active.as_mut().unwrap().ticket = Some(ticket);
                self.submission.status = MainWindowConversationComposerSubmissionStatus::Pending(
                    ComposerHostSubmissionStage::Flushing,
                );
                self.schedule_submission_advance(generation, ticket, window, cx)
            }
            Err(_) => {
                self.finish_submission_failure(window, cx);
                Ok(())
            }
        }
    }

    fn schedule_submission_advance(
        &mut self,
        generation: u64,
        ticket: ComposerHostSubmissionTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let active = self
            .submission
            .active
            .as_ref()
            .filter(|active| active.ticket == Some(ticket))
            .ok_or_else(|| "conversation composer submission ticket is stale".to_owned())?;
        let selection = active.selection;
        let prepared = active.prepared.clone();
        let cancellation = active.cancellation.clone();
        let service = self.service.clone();
        let assets = self.submission_assets();
        let marker_seals = self.submission_marker_seals();
        #[cfg(feature = "test-faults")]
        let test_gate = self.submission.test_advance_gate.take();
        #[cfg(feature = "test-faults")]
        let test_advance = self.submission.test_advance.take();
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if let Some(gate) = test_gate {
                gate.await;
            }
            let result = service.advance_submission(
                selection,
                ticket,
                assets.clone(),
                &marker_seals,
                prepared.publication_operation_id,
                prepared.marker_authority,
                prepared.published_at,
                &prepared.successor_request,
                prepared.successor_retirement_operation_id,
                prepared.next_draft_id,
                &cancellation,
            );
            #[cfg(feature = "test-faults")]
            if let Some(test_advance) = test_advance {
                return test_faults::apply_test_advance(
                    test_advance,
                    result,
                    &service,
                    selection,
                    ticket,
                    assets,
                    &marker_seals,
                    &prepared,
                    &cancellation,
                );
            }
            result
        });
        self.submission.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this
                    .finish_submission_advance(generation, ticket, result, window, cx)
                    .is_err()
                {
                    this.cancel_submission_after_failure(generation, ticket, window, cx);
                }
            });
        }));
        Ok(())
    }

    fn finish_submission_advance(
        &mut self,
        generation: u64,
        ticket: ComposerHostSubmissionTicket,
        result: Result<MainWindowComposerSubmissionAdvance, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.submission.generation != generation
            || self
                .submission
                .active
                .as_ref()
                .is_none_or(|active| active.ticket != Some(ticket))
        {
            return Ok(());
        }
        if let Some(current) = self.service.selected_identity() {
            let previous = self.submission.active.as_ref().unwrap().selection;
            if current.window_id() != previous.window_id()
                || current.claim() != previous.claim()
                || current.binding().presentation_generation()
                    != previous.binding().presentation_generation()
            {
                return Err("conversation composer submission selection drifted".to_owned());
            }
            if current != previous {
                self.synchronize_contribution_selection(cx)?;
                self.submission.active.as_mut().unwrap().selection = current;
            }
        }
        match result {
            Ok(MainWindowComposerSubmissionAdvance::Progress(stage)) => {
                self.submission.status =
                    MainWindowConversationComposerSubmissionStatus::Pending(stage);
                self.defer_submission_advance(generation, ticket, window, cx);
            }
            Ok(MainWindowComposerSubmissionAdvance::ReconciliationPending) => {
                self.submission.status =
                    MainWindowConversationComposerSubmissionStatus::Reconciling;
                self.defer_submission_advance(generation, ticket, window, cx);
            }
            #[cfg(feature = "test-faults")]
            Ok(MainWindowComposerSubmissionAdvance::TestReconciliationPending) => {
                self.submission.status =
                    MainWindowConversationComposerSubmissionStatus::Reconciling;
            }
            Ok(MainWindowComposerSubmissionAdvance::DirectAdmissionDenied(outcome)) => {
                let active = self.submission.active.as_mut().unwrap();
                active.terminal_after_cancel = Some(
                    MainWindowConversationComposerSubmissionStatus::DirectAdmissionDenied(outcome),
                );
                active.cancellation.cancel();
                self.defer_submission_advance(generation, ticket, window, cx);
            }
            Ok(MainWindowComposerSubmissionAdvance::NotCommitted) => {
                let active = self.submission.active.as_mut().unwrap();
                if active.terminal_after_cancel.is_none() {
                    active.terminal_after_cancel =
                        Some(MainWindowConversationComposerSubmissionStatus::NotCommitted);
                }
                active.cancellation.cancel();
                self.defer_submission_advance(generation, ticket, window, cx);
            }
            Ok(MainWindowComposerSubmissionAdvance::Cancelled) => {
                let status = self
                    .submission
                    .active
                    .as_ref()
                    .and_then(|active| active.terminal_after_cancel)
                    .unwrap_or(MainWindowConversationComposerSubmissionStatus::Cancelled);
                self.finish_submission_noncommit(status, window, cx)?;
            }
            Ok(MainWindowComposerSubmissionAdvance::Collision)
            | Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable) => {
                self.submission
                    .clear_active(MainWindowConversationComposerSubmissionStatus::Unavailable);
            }
            Ok(MainWindowComposerSubmissionAdvance::Stale) => {
                if let Some(status) = self
                    .submission
                    .active
                    .as_ref()
                    .and_then(|active| active.terminal_after_cancel)
                {
                    self.finish_submission_noncommit(status, window, cx)?;
                } else {
                    self.submission
                        .clear_active(MainWindowConversationComposerSubmissionStatus::Idle);
                }
            }
            Ok(MainWindowComposerSubmissionAdvance::SuccessorReady {
                receipt,
                predecessor,
                successor,
            }) => {
                self.submission.status =
                    MainWindowConversationComposerSubmissionStatus::OpeningSuccessor;
                self.submission.active.as_mut().unwrap().successor = Some(SubmissionSuccessor {
                    receipt,
                    predecessor,
                    successor,
                });
                self.finish_submission_successor(generation, window, cx)?;
            }
            Err(_) => self.cancel_submission_after_failure(generation, ticket, window, cx),
        }
        cx.notify();
        Ok(())
    }

    fn defer_submission_advance(
        &mut self,
        generation: u64,
        ticket: ComposerHostSubmissionTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, window, cx| {
            if this.submission.generation == generation
                && this
                    .schedule_submission_advance(generation, ticket, window, cx)
                    .is_err()
            {
                this.cancel_submission_after_failure(generation, ticket, window, cx);
            }
        });
    }

    fn cancel_submission_after_failure(
        &mut self,
        generation: u64,
        ticket: ComposerHostSubmissionTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .submission
            .active
            .as_ref()
            .is_some_and(|active| active.successor.is_some())
        {
            return;
        }
        let Some(active) = self.submission.active.as_mut() else {
            return;
        };
        if active.terminal_after_cancel.is_some() {
            self.submission
                .clear_active(MainWindowConversationComposerSubmissionStatus::Unavailable);
            return;
        }
        active.terminal_after_cancel = Some(MainWindowConversationComposerSubmissionStatus::Failed);
        active.cancellation.cancel();
        self.defer_submission_advance(generation, ticket, window, cx);
    }

    fn finish_submission_noncommit(
        &mut self,
        status: MainWindowConversationComposerSubmissionStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.synchronize_contribution_selection(cx)?;
        self.resume_contribution(window, cx)?;
        self.submission.clear_active(status);
        self.refresh_autosave(window, cx)
    }

    pub(super) fn finish_submission_failure(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.resume_contribution(window, cx);
        self.submission
            .clear_active(MainWindowConversationComposerSubmissionStatus::Failed);
        let _ = self.refresh_autosave(window, cx);
    }

    fn finish_submission_successor(
        &mut self,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let successor = self
            .submission
            .active
            .as_ref()
            .and_then(|active| active.successor)
            .ok_or_else(|| "conversation composer submission successor is missing".to_owned())?;
        if !self.ensure_pending_composer(successor.receipt, window, cx)? {
            self.defer_submission_successor(generation, window, cx);
            return Ok(());
        }
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.submission.test_fail_successor_after_readiness) {
            self.defer_submission_successor(generation, window, cx);
            return Ok(());
        }
        let predecessor = self
            .contribution
            .as_ref()
            .filter(|composer| composer.read(cx).selection_identity() == successor.predecessor)
            .cloned()
            .ok_or_else(|| "conversation composer predecessor is stale".to_owned())?;
        let release = predecessor.update(cx, |composer, composer_cx| {
            composer.release_widget(window, composer_cx)
        })?;
        let selected = self
            .service
            .complete_submission_successor_after_widget_release(successor.receipt, &release)?;
        if selected != successor.successor {
            return Err("conversation composer successor identity changed".to_owned());
        }
        let contribution = self
            .detach_pending_presentation(successor.receipt, cx)?
            .ok_or_else(|| "conversation composer successor presentation is missing".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            composer.promote_pending(successor.receipt, selected, window, composer_cx)
        })?;
        self.contribution = Some(contribution);
        self.subscribe_to_contribution(window, cx)?;
        self.submission
            .clear_active(MainWindowConversationComposerSubmissionStatus::Idle);
        self.initialize_autosave(window, cx)?;
        cx.notify();
        Ok(())
    }

    fn defer_submission_successor(
        &mut self,
        generation: u64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        cx.defer_in(window, move |this, window, cx| {
            if this.submission.generation == generation {
                let _ = this.finish_submission_successor(generation, window, cx);
            }
        });
    }
}

impl Drop for MainWindowConversationComposerMount {
    fn drop(&mut self) {
        if let Some(task) = self.submission.task.take() {
            task.detach();
        }
        let Some(active) = self.submission.active.take() else {
            return;
        };
        active.cancellation.cancel();
        let service = self.service.clone();
        let assets = self.submission_assets();
        let marker_seals = self.submission_marker_seals();
        let executor = self.submission.executor.clone();
        executor
            .clone()
            .spawn(async move {
                drain_unmounted_submission(service, *active, assets, marker_seals, executor).await;
            })
            .detach();
    }
}

async fn drain_unmounted_submission(
    service: std::sync::Arc<crate::main_window::MainWindowConversationComposerService>,
    active: ActiveMountedSubmission,
    assets: beryl_state::AssetState,
    marker_seals: crate::composer_marker_seal::DraftMarkerSealService,
    executor: BackgroundExecutor,
) {
    if let Some(successor) = active.successor {
        retire_unmounted_successor(&service, successor.receipt, &executor).await;
        return;
    }
    let Some(ticket) = active.ticket else {
        return;
    };
    let mut selection = active.selection;
    let mut delay = std::time::Duration::from_millis(1);
    let mut errors = 0_u8;
    loop {
        if let Some(current) = service.selected_identity() {
            if current.window_id() != selection.window_id()
                || current.claim() != selection.claim()
                || current.binding().presentation_generation()
                    != selection.binding().presentation_generation()
            {
                return;
            }
            selection = current;
        }
        let advance = service.advance_submission(
            selection,
            ticket,
            assets.clone(),
            &marker_seals,
            active.prepared.publication_operation_id,
            active.prepared.marker_authority,
            active.prepared.published_at,
            &active.prepared.successor_request,
            active.prepared.successor_retirement_operation_id,
            active.prepared.next_draft_id,
            &active.cancellation,
        );
        match advance {
            Ok(MainWindowComposerSubmissionAdvance::Progress(_))
            | Ok(MainWindowComposerSubmissionAdvance::ReconciliationPending)
            | Ok(MainWindowComposerSubmissionAdvance::DirectAdmissionDenied(_))
            | Ok(MainWindowComposerSubmissionAdvance::NotCommitted) => {}
            #[cfg(feature = "test-faults")]
            Ok(MainWindowComposerSubmissionAdvance::TestReconciliationPending) => {}
            Ok(MainWindowComposerSubmissionAdvance::SuccessorReady { receipt, .. }) => {
                retire_unmounted_successor(&service, receipt, &executor).await;
                return;
            }
            Ok(MainWindowComposerSubmissionAdvance::Cancelled)
            | Ok(MainWindowComposerSubmissionAdvance::Collision)
            | Ok(MainWindowComposerSubmissionAdvance::Stale)
            | Ok(MainWindowComposerSubmissionAdvance::SuccessorUnavailable) => return,
            Err(_) => {
                if let Some(receipt) = service.pending_receipt() {
                    retire_unmounted_successor(&service, receipt, &executor).await;
                    return;
                }
                if errors != 0 {
                    return;
                }
                errors = 1;
            }
        }
        executor.timer(delay).await;
        delay = delay
            .saturating_mul(2)
            .min(std::time::Duration::from_millis(100));
    }
}

async fn retire_unmounted_successor(
    service: &crate::main_window::MainWindowConversationComposerService,
    receipt: MainWindowComposerActivationReceipt,
    executor: &BackgroundExecutor,
) {
    let mut delay = std::time::Duration::from_millis(1);
    loop {
        match service.release_failed_pending(receipt) {
            Ok(crate::main_window::MainWindowComposerRetirementAdvance::Pending) => {}
            Ok(crate::main_window::MainWindowComposerRetirementAdvance::Retired)
            | Ok(crate::main_window::MainWindowComposerRetirementAdvance::DepartedFreshBoundary)
            | Err(_) => return,
        }
        executor.timer(delay).await;
        delay = delay
            .saturating_mul(2)
            .min(std::time::Duration::from_millis(100));
    }
}

fn fresh_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| "conversation composer submission identity generation failed".to_owned())?;
    Ok(bytes)
}

fn current_timestamp() -> Result<SyndicTimestamp, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "conversation composer submission clock precedes the Unix epoch".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "conversation composer submission timestamp overflowed".to_owned())?;
    Ok(SyndicTimestamp::from_unix_millis(millis))
}
