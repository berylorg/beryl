use std::{collections::VecDeque, sync::Arc, time::Duration};

use beryl_home_store::CommandCancellation;
use beryl_state::WindowClaimSelection;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Subscription, Task, Window,
};
use syndic_storage::DraftPieceOperationIdV1;

use crate::composer_host::{
    ComposerHostActivationRequest, ComposerHostFlushAdmission, ComposerHostFlushCapture,
    ComposerHostFlushState, ComposerHostFlushTicket, ComposerHostMarkerSealAuthority,
};
use crate::composer_marker_seal::DraftMarkerSealService;

use super::{
    MainWindowComposerActivationAdvance, MainWindowComposerActivationReceipt,
    MainWindowComposerActivationResidency, MainWindowComposerDisposalAdvance,
    MainWindowComposerPublishAdvance, MainWindowComposerResidencyBound,
    MainWindowComposerRetirementAdvance, MainWindowComposerSelectionIdentity,
    MainWindowComposerWidgetRelease, MainWindowConversationComposer,
    MainWindowConversationComposerCompositeHit, MainWindowConversationComposerConfig,
    MainWindowConversationComposerPendingRealizerToken,
    MainWindowConversationComposerPreparedSelection, MainWindowConversationComposerService,
    MainWindowConversationComposerSurfaceSnapshot, MainWindowNativeLineagePrepublicationSource,
};

pub(super) mod autosave;
mod close;
mod native_disposal;
mod native_lineage;
mod pending_presentation;
mod submission;

use pending_presentation::MainWindowConversationComposerPendingPresentation;
mod realization;

pub use autosave::*;
pub use close::{
    MainWindowConversationComposerCloseAdmission, MainWindowConversationComposerCloseAdvance,
    MainWindowConversationComposerCloseTicket,
};
#[cfg(feature = "test-faults")]
pub use submission::{
    MainWindowComposerSubmissionAdvanceTestRelease, MainWindowComposerSubmissionAdvanceTestToken,
    MainWindowComposerSubmissionTestAdvance,
    MainWindowConversationComposerSubmissionTestDiagnostics,
};
pub use submission::{
    MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerSubmissionStatus,
};

pub type MainWindowConversationComposerConfigurator = Box<
    dyn FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>
        + 'static,
>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerMountPublishAdvance {
    Retained(MainWindowComposerPublishAdvance),
    TargetSurfacePending(MainWindowComposerActivationReceipt),
    WidgetReleasePending(MainWindowComposerSelectionIdentity),
    Published(MainWindowComposerSelectionIdentity),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerMountDisposalAdvance {
    Retained(MainWindowComposerDisposalAdvance),
    WidgetReleasePending(MainWindowComposerSelectionIdentity),
    Disposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowConversationComposerMountEvent {
    ClipboardLimitExceeded {
        selection: MainWindowComposerSelectionIdentity,
    },
    RichPastePropagated {
        selection: MainWindowComposerSelectionIdentity,
    },
}

#[derive(Debug)]
pub enum MainWindowConversationComposerMountFlushStart {
    TargetPriming(MainWindowComposerActivationReceipt),
    WidgetFencePending(MainWindowComposerSelectionIdentity),
    Started(ComposerHostFlushAdmission),
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowNativeLineageMountDiagnostics {
    pub snapshot_present: bool,
    pub selection_current: bool,
    pub seed_present: bool,
    pub config_present: bool,
    pub validation_task_present: bool,
    pub validation_result_present: bool,
    pub prompt_published: bool,
    pub failure_present: bool,
    pub capacity_blocked: bool,
    pub disposal_active: bool,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowNativeLineagePromptCommandPresentation {
    Enabled,
    Disabled,
    Running,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowNativeLineagePromptDiagnostics {
    pub retry: MainWindowNativeLineagePromptCommandPresentation,
    pub recover_from_syndic: MainWindowNativeLineagePromptCommandPresentation,
    pub failed_command: Option<crate::cas_projection::NativeLineageRecoveryCommand>,
    pub retry_label: &'static str,
    pub recover_label: &'static str,
    pub retry_disabled_explanation: &'static str,
    pub recover_disabled_explanation: &'static str,
    pub local_failure_present: bool,
    pub disposal_failure_present: bool,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowNativeLineageDisposalDiagnostics {
    pub mount_release_present: bool,
    pub mount_contribution_present: bool,
    pub mount_subscription_present: bool,
    pub mount_flush_ticket_present: bool,
    pub mount_flush_capture: Option<ComposerHostFlushCapture>,
    pub mount_last_disposal_advance: Option<MainWindowComposerDisposalAdvance>,
    pub marker_current_flights: usize,
    pub marker_driving_flights: usize,
    pub marker_terminalizing_flights: usize,
    pub slot_selected: bool,
    pub slot_pending: bool,
    pub slot_disposed: bool,
    pub slot_suspended: bool,
    pub slot_disposal_flushing: bool,
    pub slot_awaiting_widget_release: bool,
    pub host_pending_requests: usize,
    pub host_settlement_custody: usize,
    pub host_timers: usize,
    pub host_barriers: usize,
    pub host_joined_publications: usize,
    pub host_publication_ready: bool,
}

pub struct MainWindowConversationComposerMount {
    service: Arc<MainWindowConversationComposerService>,
    configurator: MainWindowConversationComposerConfigurator,
    contribution: Option<Entity<MainWindowConversationComposer>>,
    pending_presentation: Option<MainWindowConversationComposerPendingPresentation>,
    autosave: autosave::MainWindowConversationComposerAutosave,
    submission: submission::MainWindowConversationComposerSubmission,
    contribution_subscription: Option<Subscription>,
    window_close: Option<close::ActiveWindowClose>,
    window_close_generation: u64,
    window_close_task: Option<Task<()>>,
    native_lineage_recovery: Option<crate::cas_projection::NativeLineageRecoveryControl>,
    native_lineage_snapshot: Option<crate::cas_projection::NativeLineageRecoverySnapshot>,
    native_lineage_selection: Option<MainWindowComposerSelectionIdentity>,
    native_lineage_seed: Option<gpui_text_input::RangeRestorationSeed>,
    native_lineage_widget_release: Option<MainWindowComposerWidgetRelease>,
    native_lineage_disposal_flush:
        Option<(MainWindowComposerSelectionIdentity, ComposerHostFlushTicket)>,
    native_lineage_disposal_capture: Option<ComposerHostFlushCapture>,
    native_lineage_disposal_task: Option<Task<()>>,
    native_lineage_disposal_cancellation: Option<CommandCancellation>,
    native_lineage_last_disposal_advance: Option<MainWindowComposerDisposalAdvance>,
    native_lineage_prompt_published: bool,
    native_lineage_route_lost: bool,
    native_lineage_config: Option<MainWindowConversationComposerConfig>,
    native_lineage_environment: Option<gpui_text_input::RangePrepublicationEnvironment>,
    native_lineage_session: Option<gpui_text_input::RangePrepublicationSession>,
    native_lineage_candidate: Option<gpui_text_input::RangePrepublicationCandidate>,
    native_lineage_effects: VecDeque<gpui_text_input::RangePrepublicationEffect>,
    native_lineage_cleanup: Option<gpui_text_input::RangePrepublicationCleanupLedger>,
    native_lineage_source: Option<Arc<MainWindowNativeLineagePrepublicationSource>>,
    native_lineage_next_environment: u64,
    native_lineage_validation: Option<(
        crate::cas_projection::NativeLineageRecoveryKey,
        MainWindowComposerSelectionIdentity,
        gpui_text_input::RangeRestorationSeed,
        Result<(), String>,
    )>,
    native_lineage_validation_task: Option<Task<()>>,
    native_lineage_host_result: Option<native_lineage::NativeLineageHostResult>,
    native_lineage_failure: Option<String>,
    native_lineage_capacity_blocked_epoch: Option<u64>,
    native_lineage_disposal_active: bool,
    native_lineage_retry_focus: FocusHandle,
    native_lineage_recovery_focus: FocusHandle,
    native_lineage_pending_focus: Option<FocusHandle>,
    native_lineage_refresh_task: Option<Task<()>>,
}

impl EventEmitter<MainWindowConversationComposerMountEvent>
    for MainWindowConversationComposerMount
{
}

impl MainWindowConversationComposerMount {
    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_disposal_diagnostics(
        &self,
    ) -> MainWindowNativeLineageDisposalDiagnostics {
        let mut diagnostics = self.service.test_native_lineage_disposal_diagnostics();
        diagnostics.mount_release_present = self.native_lineage_widget_release.is_some();
        diagnostics.mount_contribution_present = self.contribution.is_some();
        diagnostics.mount_subscription_present = self.contribution_subscription.is_some();
        diagnostics.mount_flush_ticket_present = self.native_lineage_disposal_flush.is_some();
        diagnostics.mount_flush_capture = self.native_lineage_disposal_capture;
        diagnostics.mount_last_disposal_advance = self.native_lineage_last_disposal_advance;
        let markers = self.submission_marker_seals().diagnostics();
        diagnostics.marker_current_flights = markers.current_flights();
        diagnostics.marker_driving_flights = markers.driving_flights();
        diagnostics.marker_terminalizing_flights = markers.terminalizing_flights();
        diagnostics
    }

    pub fn new(
        service: Arc<MainWindowConversationComposerService>,
        mut configurator: MainWindowConversationComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let prepared = Self::prepare_selected(service, &mut configurator)?;
        Self::from_prepared(
            prepared,
            configurator,
            marker_seals,
            submission_request_source,
            window,
            cx,
        )
    }

    pub fn prepare_selected(
        service: Arc<MainWindowConversationComposerService>,
        configurator: &mut impl FnMut(
            MainWindowComposerSelectionIdentity,
        ) -> Result<MainWindowConversationComposerConfig, String>,
    ) -> Result<MainWindowConversationComposerPreparedSelection, String> {
        let selection = service
            .selected_identity()
            .ok_or_else(|| "conversation composer mount has no selected slot".to_owned())?;
        let config = configurator(selection)?;
        MainWindowConversationComposerPreparedSelection::new(config, service)
    }

    pub fn from_prepared(
        prepared: MainWindowConversationComposerPreparedSelection,
        configurator: MainWindowConversationComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let service = prepared.service();
        let assets = prepared.assets();
        let contribution = prepared.mount(
            MainWindowConversationComposer::production_clipboard_writer(),
            window,
            cx,
        )?;
        let mut this = Self::complete(
            service,
            configurator,
            assets,
            marker_seals,
            submission_request_source,
            contribution,
            cx,
        );
        this.subscribe_to_contribution(window, cx)?;
        this.initialize_autosave(window, cx)?;
        Ok(this)
    }

    pub fn from_prepared_entity(
        prepared: MainWindowConversationComposerPreparedSelection,
        configurator: MainWindowConversationComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<Self>, String> {
        let service = prepared.service();
        let assets = prepared.assets();
        let contribution = prepared.mount(
            MainWindowConversationComposer::production_clipboard_writer(),
            window,
            cx,
        )?;
        let mount = cx.new(|mount_cx| {
            Self::complete(
                service,
                configurator,
                assets,
                marker_seals,
                submission_request_source,
                contribution,
                mount_cx,
            )
        });
        mount.update(cx, |mount, mount_cx| {
            mount.subscribe_to_contribution(window, mount_cx)?;
            mount.initialize_autosave(window, mount_cx)
        })?;
        Ok(mount)
    }

    fn complete(
        service: Arc<MainWindowConversationComposerService>,
        configurator: MainWindowConversationComposerConfigurator,
        assets: beryl_state::AssetState,
        marker_seals: DraftMarkerSealService,
        submission_request_source: MainWindowComposerSubmissionRequestSource,
        contribution: Entity<MainWindowConversationComposer>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            service,
            configurator,
            contribution: Some(contribution),
            pending_presentation: None,
            autosave: autosave::MainWindowConversationComposerAutosave::new(assets, marker_seals),
            submission: submission::MainWindowConversationComposerSubmission::new(
                submission_request_source,
                cx.background_executor().clone(),
            ),
            contribution_subscription: None,
            window_close: None,
            window_close_generation: 0,
            window_close_task: None,
            native_lineage_recovery: None,
            native_lineage_snapshot: None,
            native_lineage_selection: None,
            native_lineage_seed: None,
            native_lineage_widget_release: None,
            native_lineage_disposal_flush: None,
            native_lineage_disposal_capture: None,
            native_lineage_disposal_task: None,
            native_lineage_disposal_cancellation: None,
            native_lineage_last_disposal_advance: None,
            native_lineage_prompt_published: false,
            native_lineage_route_lost: false,
            native_lineage_config: None,
            native_lineage_environment: None,
            native_lineage_session: None,
            native_lineage_candidate: None,
            native_lineage_effects: VecDeque::new(),
            native_lineage_cleanup: None,
            native_lineage_source: None,
            native_lineage_next_environment: 1,
            native_lineage_validation: None,
            native_lineage_validation_task: None,
            native_lineage_host_result: None,
            native_lineage_failure: None,
            native_lineage_capacity_blocked_epoch: None,
            native_lineage_disposal_active: false,
            native_lineage_retry_focus: cx.focus_handle(),
            native_lineage_recovery_focus: cx.focus_handle(),
            native_lineage_pending_focus: None,
            native_lineage_refresh_task: None,
        }
    }

    pub fn selected_first_presentable(&self, cx: &App) -> bool {
        self.contribution.as_ref().is_some_and(|contribution| {
            contribution.read_with(cx, |composer, cx| composer.selected_first_presentable(cx))
        })
    }

    pub(in crate::main_window) fn apply_appearance(
        &mut self,
        theme: gpui_text_input::TextInputTheme,
        scrollbar_style: gpui_scrollbar::ScrollbarStyle,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .ok_or_else(|| "conversation composer mount has no contribution".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            composer.apply_appearance(theme, scrollbar_style, composer_cx)
        })
    }

    pub fn contribution(&self) -> Option<Entity<MainWindowConversationComposer>> {
        (!self.native_lineage_prompt_published)
            .then(|| self.contribution.clone())
            .flatten()
    }

    pub fn submission_status(&self) -> MainWindowConversationComposerSubmissionStatus {
        self.submission.status()
    }

    #[cfg(feature = "test-faults")]
    pub fn test_pending_contribution(&self) -> Option<Entity<MainWindowConversationComposer>> {
        self.pending_presentation
            .as_ref()
            .map(|pending| pending.contribution.clone())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_activation_residency(
        &self,
        cx: &App,
    ) -> Option<MainWindowComposerActivationResidency> {
        self.activation_residency(cx).ok().flatten()
    }

    pub fn selected_identity(&self) -> Option<MainWindowComposerSelectionIdentity> {
        self.service.selected_identity()
    }

    pub fn realization_diagnostics(
        &self,
        cx: &App,
    ) -> Option<gpui_text_input::RangeRealizationDiagnostics> {
        self.contribution.as_ref().map(|contribution| {
            contribution.read_with(cx, |composer, composer_cx| {
                composer.realization_diagnostics(composer_cx)
            })
        })
    }

    pub fn begin_activation(
        &mut self,
        claim: WindowClaimSelection,
        request: ComposerHostActivationRequest,
        retirement_operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerActivationAdvance, String> {
        if self.window_close.is_some() {
            return Err("conversation composer is waiting for window close".to_owned());
        }
        if self.native_lineage_snapshot.is_some() {
            return Err("native lineage recovery owns the selected composer".to_owned());
        }
        if let Some(receipt) = self.service.pending_receipt() {
            match self.retire_pending(receipt, cx)? {
                MainWindowComposerRetirementAdvance::Retired => {}
                MainWindowComposerRetirementAdvance::Pending => {
                    return Err("superseded composer target is still retiring".to_owned());
                }
                MainWindowComposerRetirementAdvance::DepartedFreshBoundary => {
                    return Err("superseded composer target departed fresh state".to_owned());
                }
            }
        }
        self.service
            .begin_activation(claim, request, retirement_operation_id, cancellation)
    }

    pub fn retire_pending(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerRetirementAdvance, String> {
        let advance = self.service.retire_pending(receipt);
        self.detach_pending_presentation(receipt, cx)?;
        advance
    }

    pub fn begin_publish(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountFlushStart, String> {
        if self.window_close.is_some() {
            return Err("conversation composer is waiting for window close".to_owned());
        }
        if self.native_lineage_snapshot.is_some() {
            return Err("native lineage recovery owns the selected composer".to_owned());
        }
        if !self.ensure_pending_composer(receipt, window, cx)? {
            return Ok(MainWindowConversationComposerMountFlushStart::TargetPriming(receipt));
        }
        let expected = match self.service.publish_preflight(receipt) {
            Ok(expected) => expected,
            Err(error) => {
                self.retire_failed_pending(receipt, cx)?;
                return Err(error);
            }
        };
        self.suspend_autosave()?;
        if !self.fence_contribution(expected, window, cx)? {
            return Ok(MainWindowConversationComposerMountFlushStart::WidgetFencePending(expected));
        }
        match self.service.begin_publish(receipt) {
            Ok(admission) => Ok(MainWindowConversationComposerMountFlushStart::Started(
                admission,
            )),
            Err(error) => {
                if self.contribution.is_some() {
                    self.resume_contribution(window, cx)?;
                    self.refresh_autosave(window, cx)?;
                }
                self.retire_failed_pending(receipt, cx)?;
                Err(error)
            }
        }
    }

    pub fn advance_publish(
        &mut self,
        receipt: MainWindowComposerActivationReceipt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountPublishAdvance, String> {
        let advance = match self.service.advance_publish(receipt) {
            Ok(advance) => advance,
            Err(error) => {
                self.resume_contribution(window, cx)?;
                self.refresh_autosave(window, cx)?;
                self.retire_failed_pending(receipt, cx)?;
                return Err(error);
            }
        };
        self.synchronize_contribution_selection(cx)?;
        if matches!(advance, MainWindowComposerPublishAdvance::PriorFlushFailed) {
            self.resume_contribution(window, cx)?;
            self.refresh_autosave(window, cx)?;
            self.retire_failed_pending(receipt, cx)?;
        }
        let MainWindowComposerPublishAdvance::WidgetReleaseRequired(expected) = advance else {
            return Ok(MainWindowConversationComposerMountPublishAdvance::Retained(
                advance,
            ));
        };
        let successor = self
            .pending_presentation
            .as_ref()
            .filter(|successor| {
                successor.receipt == receipt
                    && successor
                        .contribution
                        .read(cx)
                        .matches_pending_target(receipt)
            })
            .map(|successor| successor.contribution.clone())
            .ok_or_else(|| "published composer target is missing".to_owned())?;
        if let Some(error) = successor.read(cx).last_error().map(str::to_owned) {
            self.resume_contribution(window, cx)?;
            self.refresh_autosave(window, cx)?;
            self.retire_failed_pending(receipt, cx)?;
            return Err(error);
        }
        if self.activation_residency(cx)?.is_none() {
            self.resume_contribution(window, cx)?;
            self.refresh_autosave(window, cx)?;
            self.retire_failed_pending(receipt, cx)?;
            return Err("combined composer residency exceeded its activation bound".to_owned());
        }
        if !successor.update(cx, |successor, successor_cx| {
            successor.admit_pending_surface(successor_cx)
        }) {
            return Ok(
                MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(receipt),
            );
        }
        let contribution = self
            .contribution
            .as_ref()
            .filter(|contribution| contribution.read(cx).selection_identity() == expected)
            .cloned()
            .ok_or_else(|| {
                "composer mount contribution does not match release request".to_owned()
            })?;
        let ready = contribution.update(cx, |composer, composer_cx| {
            composer.begin_widget_release_fence(window, composer_cx)
        })?;
        if !ready {
            return Ok(
                MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(expected),
            );
        }
        if let Err(error) = self.service.begin_final_publish(receipt, expected) {
            self.resume_contribution(window, cx)?;
            self.refresh_autosave(window, cx)?;
            self.retire_failed_pending(receipt, cx)?;
            return Err(error);
        }
        let release = contribution.update(cx, |composer, composer_cx| {
            composer.release_widget(window, composer_cx)
        })?;
        let published = self
            .service
            .complete_publish_after_widget_release(receipt, &release)?;
        let MainWindowComposerPublishAdvance::Published(selection) = published else {
            return Ok(MainWindowConversationComposerMountPublishAdvance::Retained(
                published,
            ));
        };
        let successor = self
            .detach_pending_presentation(receipt, cx)?
            .ok_or_else(|| "published composer target is missing".to_owned())?;
        successor.update(cx, |composer, composer_cx| {
            composer.promote_pending(receipt, selection, window, composer_cx)
        })?;
        self.contribution = Some(successor);
        self.subscribe_to_contribution(window, cx)?;
        self.initialize_autosave(window, cx)?;
        cx.notify();
        Ok(MainWindowConversationComposerMountPublishAdvance::Published(selection))
    }

    pub fn capture_flush_disposal(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        operation_id: DraftPieceOperationIdV1,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, String> {
        self.service
            .capture_flush_disposal(selection, flush, operation_id, cancellation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn capture_flush_publication(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        flush: ComposerHostFlushTicket,
        assets: beryl_state::AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: syndic_storage::SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostFlushCapture, String> {
        self.service.capture_flush_publication(
            selection,
            flush,
            assets,
            marker_seals,
            operation_id,
            marker_authority,
            published_at,
            cancellation,
        )
    }

    pub fn begin_disposal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountFlushStart, String> {
        if self.native_lineage_disposal_task.is_some() {
            return Err("native lineage disposal is still advancing".to_owned());
        }
        if self.window_close.is_some() {
            return Err("conversation composer is waiting for window close settlement".to_owned());
        }
        if self.cancel_mounted_submission() {
            return Err("composer submission cancellation is still settling".to_owned());
        }
        self.clear_pending_presentation(cx)?;
        if !self.native_lineage_disposal_active {
            self.cancel_native_lineage_for_lifecycle(window, cx)?;
        }
        let expected = match self.service.disposal_preflight() {
            Ok(expected) => expected,
            Err(error) => {
                self.preserve_native_lineage_disposal_failure(
                    format!("Composer disposal could not start: {error}"),
                    cx,
                );
                return Err(error);
            }
        };
        if let Err(error) = self.suspend_autosave() {
            self.preserve_native_lineage_disposal_failure(
                format!("Composer disposal could not suspend autosave: {error}"),
                cx,
            );
            return Err(error);
        }
        let fenced = if self.contribution.is_some() {
            match self.fence_contribution(expected, window, cx) {
                Ok(fenced) => fenced,
                Err(error) => {
                    self.preserve_native_lineage_disposal_failure(
                        format!("Composer disposal could not fence its editor: {error}"),
                        cx,
                    );
                    return Err(error);
                }
            }
        } else {
            true
        };
        if self.contribution.is_some() && !fenced {
            return Ok(MainWindowConversationComposerMountFlushStart::WidgetFencePending(expected));
        }
        match self.service.begin_disposal() {
            Ok(admission) => {
                self.native_lineage_failure = None;
                if self.native_lineage_widget_release.is_some()
                    && let ComposerHostFlushAdmission::Started { ticket, .. }
                    | ComposerHostFlushAdmission::Joined { ticket, .. } = admission
                {
                    if self
                        .native_lineage_disposal_flush
                        .map(|(_, current)| current)
                        != Some(ticket)
                    {
                        self.native_lineage_disposal_cancellation =
                            Some(CommandCancellation::new());
                    }
                    self.native_lineage_disposal_flush = Some((expected, ticket));
                    self.native_lineage_disposal_capture = None;
                    self.native_lineage_last_disposal_advance = None;
                }
                Ok(MainWindowConversationComposerMountFlushStart::Started(
                    admission,
                ))
            }
            Err(error) => {
                self.preserve_native_lineage_disposal_failure(
                    format!("Composer disposal admission failed: {error}"),
                    cx,
                );
                if self.contribution.is_some() {
                    self.resume_contribution(window, cx)?;
                    self.refresh_autosave(window, cx)?;
                }
                Err(error)
            }
        }
    }

    pub fn advance_disposal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountDisposalAdvance, String> {
        let result = self.advance_disposal_inner(window, cx);
        match &result {
            Err(error) => self.preserve_native_lineage_disposal_failure(
                format!("Composer disposal failed: {error}"),
                cx,
            ),
            Ok(MainWindowConversationComposerMountDisposalAdvance::Retained(
                MainWindowComposerDisposalAdvance::Failed,
            )) => self.preserve_native_lineage_disposal_failure(
                "Composer disposal failed. Your preserved draft remains available for recovery."
                    .to_owned(),
                cx,
            ),
            _ => {}
        }
        result
    }

    fn advance_disposal_inner(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountDisposalAdvance, String> {
        if self.native_lineage_last_disposal_advance
            == Some(MainWindowComposerDisposalAdvance::Disposed)
        {
            return Ok(MainWindowConversationComposerMountDisposalAdvance::Disposed);
        }
        if self.native_lineage_disposal_flush.is_some() {
            return self.advance_native_lineage_disposal_task(window, cx);
        }
        let advance = self.service.advance_disposal()?;
        self.finish_disposal_advance(advance, window, cx)
    }

    pub(super) fn finish_disposal_advance(
        &mut self,
        advance: MainWindowComposerDisposalAdvance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountDisposalAdvance, String> {
        self.native_lineage_last_disposal_advance = Some(advance);
        if self.contribution.is_some() {
            self.synchronize_contribution_selection(cx)?;
        }
        if matches!(advance, MainWindowComposerDisposalAdvance::Failed) {
            if self.contribution.is_some() {
                self.resume_contribution(window, cx)?;
                self.refresh_autosave(window, cx)?;
            }
        }
        let MainWindowComposerDisposalAdvance::WidgetReleaseRequired(expected) = advance else {
            return Ok(MainWindowConversationComposerMountDisposalAdvance::Retained(advance));
        };
        if self
            .native_lineage_widget_release
            .as_ref()
            .is_some_and(|release| release.selection() == expected)
        {
            let completion = self.service.complete_disposal_after_widget_release(
                self.native_lineage_widget_release
                    .as_ref()
                    .expect("matching native-lineage widget release exists"),
            )?;
            return match completion {
                MainWindowComposerDisposalAdvance::Disposed => {
                    self.native_lineage_widget_release = None;
                    self.native_lineage_disposal_flush = None;
                    self.native_lineage_disposal_capture = None;
                    self.native_lineage_last_disposal_advance = None;
                    self.contribution = None;
                    self.contribution_subscription = None;
                    self.suspend_autosave()?;
                    self.clear_native_lineage_mount_state();
                    cx.notify();
                    Ok(MainWindowConversationComposerMountDisposalAdvance::Disposed)
                }
                retained => {
                    Ok(MainWindowConversationComposerMountDisposalAdvance::Retained(retained))
                }
            };
        }
        let contribution = self
            .contribution
            .as_ref()
            .filter(|contribution| contribution.read(cx).selection_identity() == expected)
            .cloned()
            .ok_or_else(|| {
                "composer mount contribution does not match disposal request".to_owned()
            })?;
        let ready = contribution.update(cx, |composer, composer_cx| {
            composer.begin_widget_release_fence(window, composer_cx)
        })?;
        if !ready {
            return Ok(
                MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(expected),
            );
        }
        let release = contribution.update(cx, |composer, composer_cx| {
            composer.release_widget(window, composer_cx)
        })?;
        match self
            .service
            .complete_disposal_after_widget_release(&release)?
        {
            MainWindowComposerDisposalAdvance::Disposed => {
                self.native_lineage_disposal_flush = None;
                self.native_lineage_disposal_capture = None;
                self.native_lineage_last_disposal_advance = None;
                self.contribution = None;
                self.contribution_subscription = None;
                self.suspend_autosave()?;
                self.clear_native_lineage_mount_state();
                cx.notify();
                Ok(MainWindowConversationComposerMountDisposalAdvance::Disposed)
            }
            retained => Ok(MainWindowConversationComposerMountDisposalAdvance::Retained(retained)),
        }
    }

    fn fence_contribution(
        &self,
        expected: MainWindowComposerSelectionIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let contribution = self
            .contribution
            .as_ref()
            .filter(|contribution| contribution.read(cx).selection_identity() == expected)
            .cloned()
            .ok_or_else(|| {
                "composer mount contribution does not match lifecycle fence".to_owned()
            })?;
        contribution.update(cx, |composer, composer_cx| {
            composer.begin_widget_release_fence(window, composer_cx)
        })
    }

    fn resume_contribution(&self, window: &Window, cx: &mut Context<Self>) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer mount has no contribution to resume".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            composer.resume_after_widget_release_fence(window, composer_cx)
        })
    }

    fn synchronize_contribution_selection(&self, cx: &mut Context<Self>) -> Result<(), String> {
        let successor = self
            .service
            .selected_identity()
            .ok_or_else(|| "composer service has no selected lifecycle identity".to_owned())?;
        let contribution = self
            .contribution
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer mount has no lifecycle contribution".to_owned())?;
        contribution.update(cx, |composer, composer_cx| {
            let expected = composer.selection_identity();
            if expected == successor {
                return Ok(());
            }
            composer.synchronize_lifecycle_selection(expected, successor, composer_cx)
        })
    }

    fn subscribe_to_contribution(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let contribution = self
            .contribution
            .as_ref()
            .cloned()
            .ok_or_else(|| "composer mount has no contribution to subscribe".to_owned())?;
        self.contribution_subscription = Some(cx.subscribe_in(
            &contribution,
            window,
            |this, _, event: &super::MainWindowConversationComposerEvent, window, cx| {
                match *event {
                    super::MainWindowConversationComposerEvent::SelectionAdvanced {
                        previous,
                        current,
                    } => {
                        this.advance_native_lineage_fenced_selection(previous, current);
                        if let Err(error) =
                            this.autosave_selection_advanced(previous, current, window, cx)
                        {
                            this.autosave.record_error(error);
                        }
                    }
                    super::MainWindowConversationComposerEvent::RichPastePropagated {
                        selection,
                    } if this.service.selected_identity() == Some(selection) => {
                        cx.emit(
                            MainWindowConversationComposerMountEvent::RichPastePropagated {
                                selection,
                            },
                        );
                    }
                    super::MainWindowConversationComposerEvent::ClipboardLimitExceeded {
                        selection,
                    } if this.service.selected_identity() == Some(selection) => {
                        cx.emit(
                            MainWindowConversationComposerMountEvent::ClipboardLimitExceeded {
                                selection,
                            },
                        );
                    }
                    super::MainWindowConversationComposerEvent::SubmitPropagated { selection }
                        if this.service.selected_identity() == Some(selection) =>
                    {
                        if this
                            .begin_mounted_submission(selection, window, cx)
                            .is_err()
                        {
                            this.finish_submission_failure(window, cx);
                        }
                    }
                    _ => {}
                }
            },
        ));
        Ok(())
    }
}
