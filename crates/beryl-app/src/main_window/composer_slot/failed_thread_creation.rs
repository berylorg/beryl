use super::*;
mod completed;
mod conversion;
mod settlement;
use crate::composer_host::{ComposerHostFailedThreadCreation, ComposerHostFailedThreadSuccessor};
use crate::main_window::{
    MainWindowComposerClaimWidgetWork, MainWindowRetiredThreadPredecessorSave,
    MainWindowThreadCreationRetirementSource,
};
use beryl_home_store::HomeRecoveryCandidate;

pub(crate) struct MainWindowCompletedThreadPredecessorDisposal {
    prior: MainWindowComposerSelectionIdentity,
    receipt: MainWindowComposerActivationReceipt,
    target: beryl_state::WindowClaimSelection,
    retirement_operation: DraftPieceOperationIdV1,
    host: Box<ComposerHostFailedThreadCreation>,
}

pub(crate) struct MainWindowCompletedThreadSuccessorCleanup {
    retired: RetiredThreadSuccessor,
}

pub(crate) struct MainWindowCompletedThreadSuccessorProgress {
    receipt: MainWindowComposerActivationReceipt,
    selection: MainWindowComposerSelectionIdentity,
    host: Box<crate::composer_host::ComposerHostCompletedThreadSuccessorProgress>,
}

pub(super) struct RetiredThreadSuccessor {
    receipt: MainWindowComposerActivationReceipt,
    selection: MainWindowComposerSelectionIdentity,
    host: Box<ComposerHostFailedThreadSuccessor>,
}

pub(crate) struct MainWindowFailedThreadCreationRetirement {
    prior: MainWindowComposerSelectionIdentity,
    prior_widget: MainWindowComposerSelectionIdentity,
    saved: Option<MainWindowRetiredThreadPredecessorSave>,
    predecessor: Option<Box<ComposerHostFailedThreadCreation>>,
    successor: Option<RetiredThreadSuccessor>,
    completed_successor: Option<MainWindowCompletedThreadSuccessorCleanup>,
    completed_progress: Option<MainWindowCompletedThreadSuccessorProgress>,
    mounted_successor: Option<MainWindowComposerSelectionIdentity>,
    cleanup_settled: bool,
    committed_target: Option<beryl_state::WindowClaimSelection>,
    claim_settled: bool,
    last_activation_generation: u64,
    predecessor_release: Option<MainWindowComposerWidgetRelease>,
    successor_release: Option<MainWindowComposerWidgetRelease>,
}

fn same_claim(
    a: MainWindowComposerSelectionIdentity,
    b: MainWindowComposerSelectionIdentity,
) -> bool {
    a.window_id() == b.window_id() && a.claim() == b.claim()
}

impl MainWindowComposerSlot {
    pub(crate) fn authenticate_failed_thread_creation_selection(
        &self,
        expected: MainWindowComposerSelectionIdentity,
        receipt: Option<MainWindowComposerActivationReceipt>,
        prior: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        if self.disposed
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self.window_close.is_some()
            || expected.window_id() != self.window_id
            || prior.window_id() != self.window_id
            || expected.binding().home_id() != prior.binding().home_id()
            || expected.binding().home_generation() != prior.binding().home_generation()
        {
            return Err("failed thread creation slot authority changed".into());
        }
        let selected_matches = self.selected_identity() == Some(expected)
            || self
                .failed_thread_successor
                .as_ref()
                .is_some_and(|successor| successor.selection == expected);
        if self
            .failed_thread_successor
            .as_ref()
            .is_some_and(|successor| Some(successor.receipt) != receipt)
        {
            return Err("partially retired successor original receipt changed".into());
        }
        let pending_matches = self.pending.as_ref().is_some_and(|pending| {
            Some(pending.receipt) == receipt
                && pending.claim == expected.claim()
                && pending.host.binding() == Some(expected.binding())
                && same_claim(pending.receipt.expected_prior, prior)
        });
        if !selected_matches && !pending_matches {
            return Err("failed thread creation selection changed".into());
        }
        if let Some(receipt) = receipt {
            if receipt.window_id != self.window_id
                || !same_claim(receipt.expected_prior, prior)
                || self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.receipt != receipt)
                || !same_claim(expected, prior)
                    && (expected.claim().thread_id() != receipt.target_thread
                        || expected.binding().candidate().session_id() != receipt.session_id
                        || expected.binding().presentation_generation()
                            != receipt.presentation_generation)
            {
                return Err("failed thread creation original receipt changed".into());
            }
        } else if !same_claim(expected, prior) || self.pending.is_some() {
            return Err("failed thread creation has no original activation receipt".into());
        }
        Ok(())
    }

    pub(crate) fn take_completed_thread_predecessor_disposal(
        &mut self,
        store: &HomeStore,
        receipt: MainWindowComposerActivationReceipt,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<MainWindowCompletedThreadPredecessorDisposal, String> {
        self.ensure_receipt(receipt)
            .map_err(|error| error.to_string())?;
        if self.selected_identity() != Some(expected)
            || !same_selected_host(Some(expected), receipt.expected_prior)
            || !matches!(
                self.pending.as_ref().unwrap().stage,
                PendingStage::AwaitingWidgetRelease | PendingStage::Finalizing
            )
            || !self.selected.as_ref().unwrap().dispatcher.is_drained()
        {
            return Err("completed predecessor disposal source changed".into());
        }
        let host = self
            .selected
            .as_mut()
            .unwrap()
            .host
            .take_completed_thread_creation_disposal(store, expected.binding())
            .map_err(|error| error.to_string())?;
        let pending = self.pending.as_ref().unwrap();
        Ok(MainWindowCompletedThreadPredecessorDisposal {
            prior: expected,
            receipt,
            target: pending.claim,
            retirement_operation: pending.retirement_operation_id,
            host: Box::new(host),
        })
    }

    pub(crate) fn take_failed_thread_creation(
        &mut self,
        store: &HomeStore,
        source: &mut MainWindowThreadCreationRetirementSource,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    ) -> Result<Box<MainWindowFailedThreadCreationRetirement>, String> {
        if store.health().state() != beryl_home_store::HomeHealthState::Failed
            || store.health().generation() != Some(source.selected.binding().home_generation())
            || store.home_id() != source.selected.binding().home_id()
        {
            return Err("failed thread creation requires its original failed home".into());
        }
        if let Some(completed) = self.completed_thread_successor.as_ref() {
            if source.completed_successor.is_some()
                || Some(completed.retired.selection.claim()) != source.committed_target
            {
                return Err(
                    "completed successor cleanup transfer does not match its source".into(),
                );
            }
            source.completed_successor = self.completed_thread_successor.take();
        }
        self.authenticate_failed_thread_creation_selection(
            source.selected,
            source.receipt,
            source.prior,
        )?;
        if source.saved.as_ref().is_some_and(|saved| {
            !same_selected_host(Some(saved.selected), source.prior)
                || saved.saved.binding() != saved.selected.binding()
        }) || source
            .completed_predecessor
            .as_ref()
            .is_some_and(|completed| {
                Some(completed.receipt) != source.receipt
                    || !same_claim(completed.prior, source.prior)
                    || Some(completed.target) != source.committed_target
                    || source
                        .saved
                        .as_ref()
                        .is_none_or(|saved| saved.selected != completed.prior)
            })
        {
            return Err("failed thread creation original save or disposal custody changed".into());
        }
        if let Some(work) = source.widget_work.as_ref() {
            let release = match work {
                MainWindowComposerClaimWidgetWork::Released(release) => {
                    if release.selection() != source.prior
                        && source
                            .saved
                            .as_ref()
                            .is_none_or(|saved| release.selection() != saved.selected)
                    {
                        return Err("original widget release source changed".into());
                    }
                    *release
                }
                MainWindowComposerClaimWidgetWork::Requests {
                    selection,
                    requests,
                } => self
                    .release_claim_widget_work(
                        source
                            .receipt
                            .ok_or("original widget release has no activation receipt")?,
                        *selection,
                        requests,
                    )
                    .map_err(|error| error.to_string())?,
            };
            if source.release.is_some_and(|prior| prior != release) {
                return Err("original widget release evidence changed".into());
            }
            source.release = Some(release);
            source.widget_work = None;
        }
        if source.release.is_some_and(|release| {
            release.selection() != source.prior
                && source
                    .saved
                    .as_ref()
                    .is_none_or(|saved| release.selection() != saved.selected)
        }) {
            return Err(
                "failed predecessor widget release differs from its original source".into(),
            );
        }
        for (receipt, selection) in source
            .completed_successor
            .as_ref()
            .map(|completed| (completed.retired.receipt, completed.retired.selection))
            .into_iter()
            .chain(
                source
                    .completed_progress
                    .as_ref()
                    .map(|progress| (progress.receipt, progress.selection)),
            )
        {
            if !same_selected_host(Some(receipt.expected_prior), source.prior)
                || Some(selection.claim()) != source.committed_target
                || selection.window_id() != self.window_id
            {
                return Err("completed successor cleanup original source changed".into());
            }
        }
        if let Some(mounted) = source.mounted_successor {
            let matches = self.pending.as_ref().is_some_and(|pending| {
                pending.claim == mounted.claim()
                    && pending.host.binding() == Some(mounted.binding())
            }) || self.selected_identity() == Some(mounted)
                && !same_claim(mounted, source.prior)
                || self
                    .failed_thread_successor
                    .as_ref()
                    .is_some_and(|retired| retired.selection == mounted)
                || source
                    .completed_successor
                    .as_ref()
                    .is_some_and(|completed| completed.retired.selection == mounted)
                || source
                    .completed_progress
                    .as_ref()
                    .is_some_and(|completed| completed.selection == mounted);
            if !matches {
                return Err("mounted successor does not belong to original cleanup custody".into());
            }
        }
        if let Some(pending) = self.pending.as_ref() {
            if !pending.dispatcher.is_drained()
                || source.receipt != Some(pending.receipt)
                || source
                    .committed_target
                    .is_some_and(|target| target != pending.claim)
            {
                return Err("failed successor still owns dispatcher work".into());
            }
        }
        self.retire_failed_thread_successors(store, source)?;
        let prior_identity = source
            .saved
            .as_ref()
            .map_or(source.selected, |saved| saved.selected);
        let predecessor =
            self.take_failed_thread_predecessor(store, source, prior_identity, markers)?;
        self.disposed = true;
        self.thread_predecessor_save = None;
        Ok(Box::new(MainWindowFailedThreadCreationRetirement {
            prior: prior_identity,
            prior_widget: source.prior,
            saved: source.saved.take(),
            predecessor: Some(predecessor),
            successor: self.failed_thread_successor.take(),
            completed_successor: source.completed_successor.take(),
            completed_progress: source.completed_progress.take(),
            mounted_successor: source.mounted_successor,
            cleanup_settled: false,
            committed_target: source.committed_target,
            claim_settled: false,
            last_activation_generation: self.last_activation_generation,
            predecessor_release: source.release.take(),
            successor_release: None,
        }))
    }
}
