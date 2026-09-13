use super::*;
use beryl_app::{
    cas_projection::{SubmissionExecutionWake, SubmissionExecutionWakeTestProbe},
    composer_host::{
        ComposerHostSubmissionError, ComposerHostSubmissionTicket, SyndicComposerHost,
    },
    process_admission::{ProcessAdmissionError, ProcessAdmissionGate},
};
use beryl_home_store::{
    HomeStore,
    test_faults::{FaultController, FaultPoint},
};

struct Submission {
    host: SyndicComposerHost,
    seals: DraftMarkerSealService,
    assets: AssetState,
    storage: syndic_storage::SyndicStorage,
    store: HomeStore,
    process: ProcessAdmissionGate,
    wakes: SubmissionExecutionWakeTestProbe,
    faults: FaultController,
    ticket: ComposerHostSubmissionTicket,
    cancellation: CommandCancellation,
    thread: beryl_model::SyndicThreadId,
    binding: beryl_app::composer_host::ComposerHostBinding,
    _home: base::TestHome,
}

impl Submission {
    fn accepting(name: &str) -> Self {
        let (home, mut store, storage, thread, faults) = base::fault_fixture(name, 201);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
        let (mut host, binding) = activated(storage.clone(), &store, thread, 202, 203);
        commit_text(&mut host, &store, binding, 1, 0, 0, "preserve", 8, 1);
        let process = ProcessAdmissionGate::new();
        let (execution, wakes) = SubmissionExecutionWake::test_for_home(&store, process.clone());
        let ticket = host
            .begin_submission(ComposerHostSubmissionRequest::new(
                execution,
                SyndicDraftId::from_bytes([204; 16]),
                SyndicItemId::from_bytes([205; 16]),
                DraftComposerMaterializationOperationIdV1::from_bytes([206; 16]),
                DraftPieceOperationIdV1::from_bytes([207; 16]),
                SyndicTimestamp::from_unix_millis(1_000),
                admission_requirement(),
            ))
            .unwrap();
        let cancellation = CommandCancellation::new();
        for _ in 0..128 {
            if host.submission_diagnostics().stage() == Some(ComposerHostSubmissionStage::Accepting)
            {
                break;
            }
            let outcome = host
                .advance_submission(
                    &store,
                    ticket,
                    assets.clone(),
                    &seals,
                    operation_id(208),
                    None,
                    SyndicTimestamp::from_unix_millis(999),
                    &cancellation,
                )
                .unwrap();
            assert!(matches!(
                outcome,
                ComposerHostSubmissionAdvance::Progress(_)
            ));
        }
        assert_eq!(
            host.submission_diagnostics().stage(),
            Some(ComposerHostSubmissionStage::Accepting)
        );
        let binding = host.binding().unwrap();
        Self {
            host,
            seals,
            assets,
            storage,
            store,
            process,
            wakes,
            faults,
            ticket,
            cancellation,
            thread,
            binding,
            _home: home,
        }
    }

    fn advance(
        &mut self,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
        self.host.advance_submission(
            &self.store,
            self.ticket,
            self.assets.clone(),
            &self.seals,
            operation_id(208),
            None,
            SyndicTimestamp::from_unix_millis(999),
            cancellation,
        )
    }

    fn assert_unsent(&self) {
        let current = self
            .storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(current.draft().id(), self.binding.candidate().draft_id());
        assert_eq!(current.draft().root_history().root(), self.binding.root());
        assert_eq!(
            current.draft().root_history().history(),
            self.binding.history()
        );
        assert!(
            self.storage
                .canonical_item(
                    &self.store,
                    SyndicItemId::from_bytes([205; 16]),
                    point_limit()
                )
                .unwrap()
                .is_none()
        );
        assert!(
            self.storage
                .accepted_input(
                    &self.store,
                    self.binding.candidate().draft_id().accepted_input_id(),
                    point_limit()
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(self.wakes.wake_count(), 0);
    }
}

#[test]
fn fence_before_acceptance_preserves_draft_and_cannot_refresh_an_old_submission() {
    let mut submission = Submission::accepting("fenced-submission");
    let revision = submission.store.home_revision().unwrap();
    let fence = submission.process.test_fence().unwrap();
    let cancellation = submission.cancellation.clone();
    assert!(matches!(
        submission.advance(&cancellation),
        Err(ComposerHostSubmissionError::ProcessAdmission(
            ProcessAdmissionError::Fenced
        ))
    ));
    assert_eq!(submission.store.home_revision().unwrap(), revision);
    submission.assert_unsent();
    fence.try_reopen(true).unwrap();
    assert!(matches!(
        submission.advance(&cancellation),
        Err(ComposerHostSubmissionError::ProcessAdmission(
            ProcessAdmissionError::Stale
        ))
    ));
    submission.assert_unsent();
    cancellation.cancel();
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::Cancelled
    );
    assert!(!submission.host.submission_diagnostics().pending());
    submission.assert_unsent();
}

#[test]
fn new_submission_is_rejected_before_flush_while_the_process_is_fenced() {
    let mut submission = Submission::accepting("begin-fenced-submission");
    let cancellation = submission.cancellation.clone();
    cancellation.cancel();
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::Cancelled
    );
    let (execution, _) =
        SubmissionExecutionWake::test_for_home(&submission.store, submission.process.clone());
    submission.process.test_fence().unwrap();
    let revision = submission.store.home_revision().unwrap();
    assert!(matches!(
        submission
            .host
            .begin_submission(ComposerHostSubmissionRequest::new(
                execution,
                SyndicDraftId::from_bytes([214; 16]),
                SyndicItemId::from_bytes([215; 16]),
                DraftComposerMaterializationOperationIdV1::from_bytes([216; 16]),
                DraftPieceOperationIdV1::from_bytes([217; 16]),
                SyndicTimestamp::from_unix_millis(1_100),
                admission_requirement(),
            )),
        Err(ComposerHostSubmissionError::ProcessAdmission(
            ProcessAdmissionError::Fenced
        ))
    ));
    assert!(!submission.host.submission_diagnostics().pending());
    assert_eq!(submission.store.home_revision().unwrap(), revision);
    submission.assert_unsent();
}

#[test]
fn acceptance_that_wins_the_fence_keeps_custody_until_its_durable_result() {
    let mut submission = Submission::accepting("admitted-submission");
    let process = submission.process.clone();
    submission
        .host
        .test_arm_submission_before_execute_fault(move |_, _| {
            let fence = process.test_fence().unwrap();
            assert_eq!(
                fence.try_reopen(true),
                Err(ProcessAdmissionError::Unsettled)
            );
        });
    assert_eq!(
        submission.advance(&CommandCancellation::new()).unwrap(),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
            user_item_id: SyndicItemId::from_bytes([205; 16])
        })
    );
    assert_eq!(submission.wakes.wake_count(), 1);
    let item = submission
        .storage
        .canonical_item(
            &submission.store,
            SyndicItemId::from_bytes([205; 16]),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        item.presentation_content()
            .unwrap()
            .summary()
            .logical_utf8_bytes(),
        8
    );
    assert!(submission.store.pending_reconciliations().is_empty());
    submission
        .process
        .test_fence()
        .unwrap()
        .try_reopen(true)
        .unwrap();
}

#[test]
fn fenced_acceptance_reconciliation_settles_before_cancellation_or_execution_retry() {
    let mut submission = Submission::accepting("fenced-submission-reconcile");
    let process = submission.process.clone();
    let faults = submission.faults.clone();
    submission
        .host
        .test_arm_submission_before_execute_fault(move |_, _| {
            let fence = process.test_fence().unwrap();
            assert_eq!(
                fence.try_reopen(true),
                Err(ProcessAdmissionError::Unsettled)
            );
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        });
    let cancellation = submission.cancellation.clone();
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    let fence = submission.process.test_fence().unwrap();
    assert_eq!(submission.store.pending_reconciliations().len(), 1);
    assert_eq!(
        fence.try_reopen(submission.store.pending_reconciliations().is_empty()),
        Err(ProcessAdmissionError::Unsettled)
    );
    cancellation.cancel();
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle {
            user_item_id: SyndicItemId::from_bytes([205; 16])
        })
    );
    assert_eq!(submission.wakes.wake_count(), 1);
    assert!(submission.store.pending_reconciliations().is_empty());
    fence.try_reopen(true).unwrap();
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::Stale
    );
}

#[test]
fn cancellation_after_admission_releases_the_reservation_without_acceptance() {
    let mut submission = Submission::accepting("cancel-admitted-submission");
    let process = submission.process.clone();
    let cancellation = submission.cancellation.clone();
    let cancel_at_cut = cancellation.clone();
    submission
        .host
        .test_arm_submission_before_execute_fault(move |_, _| {
            process.test_fence().unwrap();
            cancel_at_cut.cancel();
        });
    assert_eq!(
        submission.advance(&cancellation).unwrap(),
        ComposerHostSubmissionAdvance::Cancelled
    );
    submission.assert_unsent();
    assert!(submission.store.pending_reconciliations().is_empty());
    submission
        .process
        .test_fence()
        .unwrap()
        .try_reopen(true)
        .unwrap();
}
