use std::sync::{Arc, Mutex};

use beryl_model::SyndicDraftId;
use syndic_storage::{DraftEditorCandidateSessionIdV1, DraftPieceOperationIdV1};

use super::SyndicComposerHost;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ComposerHostDisposalExecutionIdentity {
    pub(crate) draft_id: SyndicDraftId,
    pub(crate) session_id: DraftEditorCandidateSessionIdV1,
    pub(crate) operation_id: DraftPieceOperationIdV1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ComposerHostDisposalExecutionSnapshot {
    pub(crate) original: Option<ComposerHostDisposalExecutionIdentity>,
    pub(crate) original_attempts: usize,
    pub(crate) different_identity_attempts: usize,
}

#[derive(Default)]
struct ObservationState {
    attached: bool,
    snapshot: ComposerHostDisposalExecutionSnapshot,
}

#[derive(Clone)]
pub(crate) struct ComposerHostDisposalExecutionObservation {
    state: Arc<Mutex<ObservationState>>,
}

impl ComposerHostDisposalExecutionObservation {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ObservationState::default())),
        }
    }

    pub(crate) fn snapshot(&self) -> ComposerHostDisposalExecutionSnapshot {
        self.state
            .lock()
            .expect("disposal execution observation is poisoned")
            .snapshot
    }

    fn attach(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "disposal execution observation is poisoned")?;
        if state.attached || state.snapshot.original.is_some() {
            return Err("disposal execution observation was already attached".into());
        }
        state.attached = true;
        Ok(())
    }

    pub(super) fn record(&self, identity: ComposerHostDisposalExecutionIdentity) {
        let mut state = self
            .state
            .lock()
            .expect("disposal execution observation is poisoned");
        assert!(state.attached);
        let snapshot = &mut state.snapshot;
        let original = *snapshot.original.get_or_insert(identity);
        let count = if original == identity {
            &mut snapshot.original_attempts
        } else {
            &mut snapshot.different_identity_attempts
        };
        *count = count
            .checked_add(1)
            .expect("disposal execution observation count exhausted");
    }
}

impl SyndicComposerHost {
    pub(crate) fn test_observe_disposal_execution(
        &mut self,
        observation: ComposerHostDisposalExecutionObservation,
    ) -> Result<(), String> {
        if self.disposal_execution_observation.is_some() {
            return Err("composer host already has a disposal execution observation".into());
        }
        observation.attach()?;
        self.disposal_execution_observation = Some(observation);
        Ok(())
    }
}
