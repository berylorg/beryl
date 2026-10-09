use super::*;
use crate::main_window::running_threads::activation::{
    RunningThreadActivationError, RunningThreadActivationOutcome,
};
use std::sync::Mutex;

pub(super) fn fail_original_confirmed_read(
    home: &beryl_home_store::HomeStore,
    faults: &FaultController,
    reached: &AtomicBool,
) {
    let generation = home.health().generation().unwrap();
    assert_eq!(
        home.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    block.release_with_error(std::io::ErrorKind::Other);
    let observed = home.home_revision();
    assert!(block.wait_until_reached(std::time::Duration::from_secs(5)));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while home.health().state() == beryl_home_store::HomeHealthState::Healthy {
        assert!(
            std::time::Instant::now() < deadline,
            "consumed confirmation fault must fail its Home"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let error = match observed {
        Err(error) => error,
        Ok(_) => home
            .home_revision()
            .expect_err("consumed confirmed fault must gate the later read"),
    };
    match &error {
        beryl_home_store::ReadError::Storage { stage, source } => {
            assert_eq!(*stage, beryl_home_store::ReadStage::Confirmation);
            assert_eq!(
                source.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::Other
            );
            assert_eq!(
                source.to_string(),
                "synthetic Beryl-home fault at BeforeReadConfirmation"
            );
        }
        beryl_home_store::ReadError::HealthGate(gate) => {
            assert_eq!(gate.state(), beryl_home_store::HomeHealthState::Failed);
            assert_eq!(gate.generation(), generation);
        }
        _ => panic!("later read did not observe its confirmed failure: {error:?}"),
    }
    assert_eq!(
        home.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert_eq!(home.health().generation(), Some(generation));
    eprintln!("original custody retained through actual failed Home read: {error:?}");
    reached.store(true, Ordering::Release);
}

#[derive(Clone, Debug)]
struct OriginalClaimOutcome {
    classification: &'static str,
    receipt: Option<beryl_home_store::CommitReceipt>,
    command_commit_failure: bool,
    problem: Option<String>,
}

#[derive(Clone, Default)]
pub(super) struct ClaimOutcomeObservation {
    original: Arc<Mutex<Option<OriginalClaimOutcome>>>,
}

impl ClaimOutcomeObservation {
    pub(super) fn record(&self, outcome: &RunningThreadActivationOutcome) {
        let original = match outcome {
            RunningThreadActivationOutcome::Settled(commit) => OriginalClaimOutcome {
                classification: "Committed",
                receipt: Some(commit.receipt.clone()),
                command_commit_failure: false,
                problem: commit.later_failure.as_ref().map(ToString::to_string),
            },
            RunningThreadActivationOutcome::Pending(pending) => OriginalClaimOutcome {
                classification: if pending.test_original_commit_receipt().is_some() {
                    "Committed"
                } else {
                    "Indeterminate"
                },
                receipt: pending.test_original_commit_receipt().cloned(),
                command_commit_failure: false,
                problem: Some(pending.problem().to_string()),
            },
            RunningThreadActivationOutcome::NotCommitted(rejected) => OriginalClaimOutcome {
                classification: "NotCommitted",
                receipt: None,
                command_commit_failure: matches!(
                    rejected.problem(),
                    RunningThreadActivationError::Command(
                        beryl_home_store::CommandError::Commit { .. }
                    )
                ),
                problem: Some(rejected.problem().to_string()),
            },
        };
        eprintln!("original ordinary claim outcome: {original:?}");
        let mut slot = self.original.lock().unwrap();
        assert!(slot.is_none());
        *slot = Some(original);
    }

    pub(super) fn verify(&self, cut: Cut) {
        let original = self.original.lock().unwrap();
        match cut {
            Cut::SaveNoncommit | Cut::SaveIndeterminate => assert!(original.is_none()),
            Cut::ClaimNoncommit => {
                let original = original
                    .as_ref()
                    .expect("original claim outcome is missing");
                assert_eq!(original.classification, "NotCommitted", "{original:?}");
                assert!(original.command_commit_failure, "{original:?}");
                assert!(original.receipt.is_none());
                assert!(original.problem.is_some());
            }
            Cut::ClaimIndeterminate => {
                let original = original
                    .as_ref()
                    .expect("original claim outcome is missing");
                assert_eq!(original.classification, "Indeterminate", "{original:?}");
                assert!(original.receipt.is_none());
                assert!(original.problem.is_some());
            }
            Cut::ClaimCommitted | Cut::DisposalCommitted => {
                let original = original
                    .as_ref()
                    .expect("original claim outcome is missing");
                assert_eq!(original.classification, "Committed", "{original:?}");
                assert!(original.receipt.is_some());
            }
        }
    }
}
