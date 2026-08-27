use std::sync::Arc;

use beryl_home_store::CommandCancellation;
use beryl_state::WindowClaimSelection;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Subscription, Window};
use syndic_storage::DraftPieceOperationIdV1;

use crate::composer_host::{
    ComposerHostActivationRequest, ComposerHostFlushAdmission, ComposerHostFlushCapture,
    ComposerHostFlushTicket, ComposerHostMarkerSealAuthority,
};
use crate::composer_marker_seal::DraftMarkerSealService;

use super::{
    MainWindowComposerActivationAdvance, MainWindowComposerActivationReceipt,
    MainWindowComposerActivationResidency, MainWindowComposerDisposalAdvance,
    MainWindowComposerPublishAdvance, MainWindowComposerResidencyBound,
    MainWindowComposerRetirementAdvance, MainWindowComposerSelectionIdentity,
    MainWindowConversationComposer, MainWindowConversationComposerCompositeHit,
    MainWindowConversationComposerConfig, MainWindowConversationComposerPendingRealizerToken,
    MainWindowConversationComposerService, MainWindowConversationComposerSurfaceSnapshot,
};

mod autosave;
mod pending_presentation;

use pending_presentation::MainWindowConversationComposerPendingPresentation;
mod realization;

pub use autosave::*;

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
    SubmitPropagated {
        selection: MainWindowComposerSelectionIdentity,
    },
}

#[derive(Debug)]
pub enum MainWindowConversationComposerMountFlushStart {
    TargetPriming(MainWindowComposerActivationReceipt),
    WidgetFencePending(MainWindowComposerSelectionIdentity),
    Started(ComposerHostFlushAdmission),
}

pub struct MainWindowConversationComposerMount {
    service: Arc<MainWindowConversationComposerService>,
    configurator: MainWindowConversationComposerConfigurator,
    contribution: Option<Entity<MainWindowConversationComposer>>,
    pending_presentation: Option<MainWindowConversationComposerPendingPresentation>,
    autosave: autosave::MainWindowConversationComposerAutosave,
    contribution_subscription: Option<Subscription>,
}

impl EventEmitter<MainWindowConversationComposerMountEvent>
    for MainWindowConversationComposerMount
{
}

impl MainWindowConversationComposerMount {
    pub fn new(
        service: Arc<MainWindowConversationComposerService>,
        mut configurator: MainWindowConversationComposerConfigurator,
        marker_seals: DraftMarkerSealService,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let assets = service.assets()?;
        let selection = service
            .selected_identity()
            .ok_or_else(|| "conversation composer mount has no selected slot".to_owned())?;
        let config = configurator(selection)?;
        let contribution = cx.new(|composer_cx| {
            MainWindowConversationComposer::new(
                config,
                service.clone(),
                MainWindowConversationComposer::production_clipboard_writer(),
                window,
                composer_cx,
            )
            .expect("validated selected composer contribution")
        });
        let mut this = Self {
            service,
            configurator,
            contribution: Some(contribution),
            pending_presentation: None,
            autosave: autosave::MainWindowConversationComposerAutosave::new(assets, marker_seals),
            contribution_subscription: None,
        };
        this.subscribe_to_contribution(window, cx)?;
        this.initialize_autosave(window, cx)?;
        Ok(this)
    }

    pub fn contribution(&self) -> Option<Entity<MainWindowConversationComposer>> {
        self.contribution.clone()
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
                self.resume_contribution(window, cx)?;
                self.refresh_autosave(window, cx)?;
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
        self.clear_pending_presentation(cx)?;
        let expected = self.service.disposal_preflight()?;
        self.suspend_autosave()?;
        if !self.fence_contribution(expected, window, cx)? {
            return Ok(MainWindowConversationComposerMountFlushStart::WidgetFencePending(expected));
        }
        match self.service.begin_disposal() {
            Ok(admission) => Ok(MainWindowConversationComposerMountFlushStart::Started(
                admission,
            )),
            Err(error) => {
                self.resume_contribution(window, cx)?;
                self.refresh_autosave(window, cx)?;
                Err(error)
            }
        }
    }

    pub fn advance_disposal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowConversationComposerMountDisposalAdvance, String> {
        let advance = self.service.advance_disposal()?;
        self.synchronize_contribution_selection(cx)?;
        if matches!(advance, MainWindowComposerDisposalAdvance::Failed) {
            self.resume_contribution(window, cx)?;
            self.refresh_autosave(window, cx)?;
        }
        let MainWindowComposerDisposalAdvance::WidgetReleaseRequired(expected) = advance else {
            return Ok(MainWindowConversationComposerMountDisposalAdvance::Retained(advance));
        };
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
                self.contribution = None;
                self.contribution_subscription = None;
                self.suspend_autosave()?;
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
            |this, _, event: &super::MainWindowConversationComposerEvent, window, cx| match *event {
                super::MainWindowConversationComposerEvent::SelectionAdvanced {
                    previous,
                    current,
                } => {
                    if let Err(error) =
                        this.autosave_selection_advanced(previous, current, window, cx)
                    {
                        this.autosave.record_error(error);
                    }
                }
                super::MainWindowConversationComposerEvent::RichPastePropagated { selection }
                    if this.service.selected_identity() == Some(selection) =>
                {
                    cx.emit(
                        MainWindowConversationComposerMountEvent::RichPastePropagated { selection },
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
                    cx.emit(MainWindowConversationComposerMountEvent::SubmitPropagated {
                        selection,
                    });
                }
                _ => {}
            },
        ));
        Ok(())
    }
}
