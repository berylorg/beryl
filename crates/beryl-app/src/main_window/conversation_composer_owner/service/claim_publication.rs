use super::*;
use crate::composer_host::{
    ComposerHostAutosaveInterval, ComposerHostAutosaveTimer, ComposerHostPublicationTicket,
};
use crate::main_window::{MainWindowComposerActivationReceipt, MainWindowComposerPublishAdvance};

pub(in crate::main_window) struct MainWindowComposerClaimPreparedPresentation {
    service: Arc<MainWindowConversationComposerService>,
    receipt: MainWindowComposerActivationReceipt,
    selection: MainWindowComposerSelectionIdentity,
    seeds: std::collections::VecDeque<crate::main_window::conversation_composer_owner::MainWindowConversationComposerActivationSeed>,
}

impl MainWindowComposerClaimPreparedPresentation {
    pub(in crate::main_window) fn try_elect_current<T>(
        self,
        publish: impl FnOnce(Self) -> T,
    ) -> Result<T, (Self, String)> {
        let service = Arc::clone(&self.service);
        let slot = match service.slot.try_lock() {
            Ok(slot) => slot,
            Err(_) => {
                return Err((
                    self,
                    "composer pending publication source is busy".to_owned(),
                ));
            }
        };
        let window_close = match service.window_close.try_lock() {
            Ok(close) => close,
            Err(_) => {
                return Err((
                    self,
                    "composer pending publication close fence is busy".to_owned(),
                ));
            }
        };
        if window_close.is_some()
            || slot.pending_receipt() != Some(self.receipt)
            || slot.pending_identity(self.receipt) != Some(self.selection)
        {
            return Err((
                self,
                "composer pending publication source is stale".to_owned(),
            ));
        }
        Ok(publish(self))
    }
    pub(in crate::main_window) fn receipt(&self) -> MainWindowComposerActivationReceipt {
        self.receipt
    }
    pub(in crate::main_window) fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    pub(in crate::main_window::conversation_composer_owner) fn into_seeds(self) -> std::collections::VecDeque<crate::main_window::conversation_composer_owner::MainWindowConversationComposerActivationSeed>{
        self.seeds
    }
    pub(in crate::main_window) fn matches_service(
        &self,
        service: &Arc<MainWindowConversationComposerService>,
    ) -> bool {
        Arc::ptr_eq(&self.service, service)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/running_threads_mount.rs"
    ));
}

pub(in crate::main_window) struct MainWindowComposerClaimPublication {
    service: Arc<MainWindowConversationComposerService>,
    receipt: MainWindowComposerActivationReceipt,
    selection: MainWindowComposerSelectionIdentity,
}

impl MainWindowComposerClaimPublication {
    pub(in crate::main_window) fn receipt(&self) -> MainWindowComposerActivationReceipt {
        self.receipt
    }
    pub(in crate::main_window) fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }
    pub(in crate::main_window) fn matches_service(
        &self,
        service: &Arc<MainWindowConversationComposerService>,
    ) -> bool {
        Arc::ptr_eq(&self.service, service)
    }
    pub(in crate::main_window) fn try_elect_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, String> {
        let slot = self
            .service
            .slot
            .try_lock()
            .map_err(|_| "composer publication source is busy".to_owned())?;
        let window_close = self
            .service
            .window_close
            .try_lock()
            .map_err(|_| "composer publication close fence is busy".to_owned())?;
        if window_close.is_some() || !slot.claim_publication_is_current(self.selection) {
            return Err("composer publication source is stale".to_owned());
        }
        Ok(publish())
    }
}

pub(in crate::main_window) struct MainWindowComposerClaimAdvance {
    pub(in crate::main_window) receipt: MainWindowComposerActivationReceipt,
    pub(in crate::main_window) advance: MainWindowComposerPublishAdvance,
    pub(in crate::main_window) selected: MainWindowComposerSelectionIdentity,
    pub(in crate::main_window) pending: MainWindowComposerSelectionIdentity,
}

#[derive(Debug)]
pub(in crate::main_window) enum MainWindowComposerClaimWidgetWork {
    Released(MainWindowComposerWidgetRelease),
    Requests {
        selection: MainWindowComposerSelectionIdentity,
        requests: Vec<RangeTextInputRequest>,
    },
}

pub(in crate::main_window) enum MainWindowComposerClaimCompletion {
    Pending(MainWindowComposerWidgetRelease),
    RetainedFailure {
        release: MainWindowComposerWidgetRelease,
        error: String,
    },
    Published {
        release: MainWindowComposerWidgetRelease,
        publication: MainWindowComposerClaimPublication,
    },
}

pub(in crate::main_window) enum MainWindowComposerClaimAutosave {
    Idle,
    Timer(ComposerHostAutosaveTimer),
    Publishing(ComposerHostPublicationTicket),
}

impl MainWindowConversationComposerService {
    pub(in crate::main_window) fn advance_committed_claim_disposal_source(
        &self,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
        operation: syndic_storage::DraftPieceOperationIdV1,
    ) -> Result<MainWindowComposerClaimAdvance, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "composer disposal source lock failed".to_owned())?;
        slot.begin_committed_claim_disposal(&self.store, receipt, expected, operation)
            .map_err(|error| format!("committed predecessor disposal admission failed: {error}"))?;
        let advance = slot
            .advance_committed_claim_disposal(&self.store, receipt)
            .map_err(|error| format!("committed predecessor disposal advance failed: {error}"))?;
        let selected = slot
            .selected_identity()
            .ok_or("committed predecessor source is missing")?;
        let pending = slot
            .pending_identity(receipt)
            .ok_or("committed target source is missing")?;
        Ok(MainWindowComposerClaimAdvance {
            receipt,
            advance,
            selected,
            pending,
        })
    }
    pub(in crate::main_window) fn begin_claim_publication_save(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<crate::composer_host::ComposerHostFlushAdmission, String> {
        self.slot
            .lock()
            .map_err(|_| "composer selection save source lock failed".to_owned())?
            .begin_claim_publication_save(&self.store, receipt)
            .map_err(|error| format!("composer selection save admission failed: {error}"))
    }
    pub(in crate::main_window) fn abort_claim_publication_before_release(
        &self,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<crate::main_window::MainWindowComposerRetirementAdvance, String> {
        self.ensure_no_window_close()?;
        self.slot
            .lock()
            .map_err(|_| "composer publication abort source lock failed".to_owned())?
            .abort_claim_publication_before_release(&self.store, receipt, expected)
            .map_err(|_| "composer publication abort source is stale".to_owned())
    }
    pub(in crate::main_window) fn prepare_claim_presentation_source(
        self: &Arc<Self>,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerClaimPreparedPresentation, String> {
        let (selection, seeds) =
            crate::main_window::MainWindowConversationComposer::prepare_pending_activation(
                self, receipt,
            )?;
        Ok(MainWindowComposerClaimPreparedPresentation {
            service: Arc::clone(self),
            receipt,
            selection,
            seeds,
        })
    }
    pub(in crate::main_window) fn advance_claim_publication_source(
        &self,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<MainWindowComposerClaimAdvance, String> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| "composer publication source lock failed".to_owned())?;
        let advance = slot
            .advance_claim_selection_save(&self.store, receipt)
            .map_err(|_| "composer publication source advance failed".to_owned())?;
        let selected = slot
            .selected_identity()
            .ok_or_else(|| "composer publication selected source is missing".to_owned())?;
        let pending = slot
            .pending_identity(receipt)
            .ok_or_else(|| "composer publication pending source is missing".to_owned())?;
        Ok(MainWindowComposerClaimAdvance {
            receipt,
            advance,
            selected,
            pending,
        })
    }

    pub(in crate::main_window) fn complete_claim_publication_source(
        self: &Arc<Self>,
        receipt: MainWindowComposerActivationReceipt,
        work: MainWindowComposerClaimWidgetWork,
    ) -> Result<MainWindowComposerClaimCompletion, (MainWindowComposerClaimWidgetWork, String)>
    {
        let mut slot = match self.slot.lock() {
            Ok(slot) => slot,
            Err(_) => return Err((work, "composer publication source lock failed".to_owned())),
        };
        let release = match &work {
            MainWindowComposerClaimWidgetWork::Released(release) => *release,
            MainWindowComposerClaimWidgetWork::Requests {
                selection,
                requests,
            } => match slot.release_claim_widget_work(receipt, *selection, requests) {
                Ok(release) => release,
                Err(_) => {
                    return Err((
                        work,
                        "composer widget release source was stale or unsettled".to_owned(),
                    ));
                }
            },
        };
        let advance =
            match slot.complete_publish_after_widget_release(&self.store, receipt, &release) {
                Ok(advance) => advance,
                Err(_) => {
                    return Ok(MainWindowComposerClaimCompletion::RetainedFailure {
                        release,
                        error: "composer publication source completion failed".to_owned(),
                    });
                }
            };
        match advance {
            MainWindowComposerPublishAdvance::Published(selection) => {
                Ok(MainWindowComposerClaimCompletion::Published {
                    release,
                    publication: MainWindowComposerClaimPublication {
                        service: Arc::clone(self),
                        receipt,
                        selection,
                    },
                })
            }
            MainWindowComposerPublishAdvance::ReconciliationPending => {
                Ok(MainWindowComposerClaimCompletion::Pending(release))
            }
            _ => Ok(MainWindowComposerClaimCompletion::RetainedFailure {
                release,
                error: "composer publication source returned an unexpected completion".to_owned(),
            }),
        }
    }

    pub(in crate::main_window) fn prepare_claim_autosave_source(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        settings: Option<(u64, ComposerHostAutosaveInterval)>,
    ) -> Result<MainWindowComposerClaimAutosave, String> {
        if let Some((generation, interval)) = settings {
            self.publish_autosave_interval(selection, generation, interval)?;
        }
        if let Some(ticket) = self.selected_autosave_publication(selection)? {
            return Ok(MainWindowComposerClaimAutosave::Publishing(ticket));
        }
        Ok(self
            .selected_autosave_timer(selection)?
            .map(MainWindowComposerClaimAutosave::Timer)
            .unwrap_or(MainWindowComposerClaimAutosave::Idle))
    }
}
