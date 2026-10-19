use super::DraftComposerRecallErrorV1;
use beryl_home_store::{
    CommandError, CommandOutcome, HomeCandidateRecoveryAccess, HomeStore, ReconciliationHandle,
    ReconciliationResolution,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct CommandObservation(Arc<Mutex<State>>);
#[derive(Default)]
enum State {
    #[default]
    Unobserved,
    Direct(CommandOutcome),
    Pending {
        _failure: CommandError,
        handle: ReconciliationHandle,
        resolution: Option<ReconciliationResolution>,
    },
}
impl CommandObservation {
    pub(crate) fn retain(&self, outcome: CommandOutcome) -> Result<(), DraftComposerRecallErrorV1> {
        let incoming = match outcome {
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => State::Pending {
                _failure: failure,
                handle: reconciliation.install_and_handle(),
                resolution: None,
            },
            direct => State::Direct(direct),
        };
        let mut state = self
            .0
            .lock()
            .map_err(|_| DraftComposerRecallErrorV1::Unavailable)?;
        if !matches!(*state, State::Unobserved) {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        *state = incoming;
        Ok(())
    }
    pub(crate) fn resume(&self, store: &HomeStore) -> Result<bool, DraftComposerRecallErrorV1> {
        self.resume_with(|h| Ok(store.retry_reconciliation(h)?), false)
    }
    pub(crate) fn resume_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<bool, DraftComposerRecallErrorV1> {
        self.resume_with(|h| Ok(access.retry_reconciliation(h)?), true)
    }
    fn resume_with(
        &self,
        retry: impl FnOnce(
            &ReconciliationHandle,
        ) -> Result<ReconciliationResolution, DraftComposerRecallErrorV1>,
        recovered: bool,
    ) -> Result<bool, DraftComposerRecallErrorV1> {
        let handle = {
            let state = self
                .0
                .lock()
                .map_err(|_| DraftComposerRecallErrorV1::Unavailable)?;
            match &*state {
                State::Pending {
                    handle,
                    resolution: None,
                    ..
                } => Some(handle.clone()),
                _ => return classify(&state, recovered),
            }
        };
        let resolution = retry(&handle.unwrap())?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| DraftComposerRecallErrorV1::Unavailable)?;
        if let State::Pending {
            resolution: slot, ..
        } = &mut *state
        {
            if slot.is_none() {
                *slot = Some(resolution);
            }
        }
        classify(&state, recovered)
    }
    pub(crate) fn known(&self) -> Result<bool, DraftComposerRecallErrorV1> {
        let state = self
            .0
            .lock()
            .map_err(|_| DraftComposerRecallErrorV1::Unavailable)?;
        classify(&state, false)
    }
}
fn classify(state: &State, recovered: bool) -> Result<bool, DraftComposerRecallErrorV1> {
    match state {
        State::Direct(CommandOutcome::Committed { later_failure, .. })
            if recovered || later_failure.is_none() =>
        {
            Ok(true)
        }
        State::Direct(CommandOutcome::Committed { .. }) => {
            Err(DraftComposerRecallErrorV1::Unavailable)
        }
        State::Direct(CommandOutcome::NotCommitted { .. }) => Ok(false),
        State::Pending {
            resolution: Some(ReconciliationResolution::ExactNew { .. }),
            ..
        } => Ok(true),
        State::Pending {
            resolution: Some(ReconciliationResolution::ExactOld),
            ..
        } => Ok(false),
        State::Pending {
            resolution: Some(_),
            ..
        } => Err(DraftComposerRecallErrorV1::Conflict),
        _ => Err(DraftComposerRecallErrorV1::Unavailable),
    }
}
