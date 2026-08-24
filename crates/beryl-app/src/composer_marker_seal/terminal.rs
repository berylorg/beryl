use beryl_home_store::{HomeCommand, HomeStore};
use syndic_storage::{DraftMarkerSealCustodyReleaseV1, DraftMarkerSealStatusV1, SyndicStorage};

use super::{
    CommandFault, DraftMarkerSealDisposeOutcome, DraftMarkerSealFlight,
    DraftMarkerSealFlightRequest, DraftMarkerSealObservedTerminal, DraftMarkerSealReleaseIntent,
    DraftMarkerSealReleaseOutcome, DraftMarkerSealRetireOutcome, DraftMarkerSealService,
    DraftMarkerSealServiceError, DurableCommandResult, HomeLoss, ReconcileFault, ServiceLifecycle,
    admission::authenticate_supersession,
    durability::{execute_command, retire, settle_command, validate_store},
    lock_state,
};

enum TerminalUpdate {
    Keep(DraftMarkerSealReleaseOutcome),
    Complete(DraftMarkerSealReleaseOutcome),
}

impl DraftMarkerSealService {
    pub fn release(
        &self,
        store: &HomeStore,
        flight: DraftMarkerSealFlight,
        intent: DraftMarkerSealReleaseIntent,
    ) -> Result<DraftMarkerSealReleaseOutcome, DraftMarkerSealServiceError> {
        let mut supersession_authenticated =
            !matches!(intent, DraftMarkerSealReleaseIntent::Superseded { .. });
        let (storage, command_fault, reconcile_fault) = loop {
            let mut state = lock_state(&self.inner);
            if store.home_id() != state.home_id {
                return Err(DraftMarkerSealServiceError::ForeignHome);
            }
            validate_store(&mut state, store)?;
            let Some(index) = state
                .flights
                .iter()
                .position(|current| current.handle == flight)
            else {
                return Ok(DraftMarkerSealReleaseOutcome::AlreadyReleased);
            };
            if let Some(active) = state.flights[index].terminal {
                if active != intent {
                    return Ok(DraftMarkerSealReleaseOutcome::ConflictingIntent {
                        active,
                        requested: intent,
                    });
                }
                if state.flights[index].driving {
                    return Ok(DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(intent));
                }
                state.flights[index].driving = true;
                break (
                    state.storage,
                    state.command_fault.take(),
                    state.reconcile_fault.take(),
                );
            }

            if !supersession_authenticated {
                let storage = state.storage;
                let request = state.flights[index].handle.request;
                drop(state);
                authenticate_supersession(storage, store, request, intent)?;
                supersession_authenticated = true;
                continue;
            }

            state.flights[index].terminal = Some(intent);
            if state.flights[index].driving {
                return Ok(DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(intent));
            }
            state.flights[index].driving = true;
            break (
                state.storage,
                state.command_fault.take(),
                state.reconcile_fault.take(),
            );
        };

        let update = settle_terminal(
            store,
            storage,
            flight.request,
            intent,
            command_fault,
            reconcile_fault,
        );
        let mut state = lock_state(&self.inner);
        let Some(index) = state
            .flights
            .iter()
            .position(|current| current.handle == flight)
        else {
            return Ok(DraftMarkerSealReleaseOutcome::HomeGenerationRetired);
        };
        if matches!(state.lifecycle, ServiceLifecycle::Retired(_)) {
            state.flights.swap_remove(index);
            return Ok(DraftMarkerSealReleaseOutcome::HomeGenerationRetired);
        }
        match update {
            Ok(TerminalUpdate::Keep(outcome)) => {
                state.flights[index].driving = false;
                Ok(outcome)
            }
            Ok(TerminalUpdate::Complete(outcome)) => {
                state.flights.swap_remove(index);
                finish_disposal(&mut state);
                Ok(outcome)
            }
            Err(DraftMarkerSealServiceError::ReconciliationCollision) => {
                state.flights.swap_remove(index);
                finish_disposal(&mut state);
                Err(DraftMarkerSealServiceError::ReconciliationCollision)
            }
            Err(error) => {
                state.flights[index].driving = false;
                Err(error)
            }
        }
    }

    pub fn dispose(
        &self,
        store: &HomeStore,
    ) -> Result<DraftMarkerSealDisposeOutcome, DraftMarkerSealServiceError> {
        let candidate = {
            let mut state = lock_state(&self.inner);
            validate_store(&mut state, store)?;
            if matches!(state.lifecycle, ServiceLifecycle::Active) {
                state.lifecycle = ServiceLifecycle::Disposing;
            }
            if matches!(state.lifecycle, ServiceLifecycle::Disposed) || state.flights.is_empty() {
                state.lifecycle = ServiceLifecycle::Disposed;
                return Ok(DraftMarkerSealDisposeOutcome::Disposed);
            }
            for flight in &mut state.flights {
                if flight.terminal.is_none() {
                    flight.terminal = Some(DraftMarkerSealReleaseIntent::ServiceDisposed);
                }
            }
            state
                .flights
                .iter()
                .find(|flight| !flight.driving)
                .map(|flight| (flight.handle, flight.terminal.unwrap()))
        };
        let Some((flight, intent)) = candidate else {
            return Ok(DraftMarkerSealDisposeOutcome::WaitingForDrive {
                remaining: self.diagnostics().current_flights(),
            });
        };
        let release = self.release(store, flight, intent)?;
        let remaining = self.diagnostics().current_flights();
        if remaining == 0 {
            let mut state = lock_state(&self.inner);
            finish_disposal(&mut state);
            Ok(DraftMarkerSealDisposeOutcome::Disposed)
        } else {
            Ok(DraftMarkerSealDisposeOutcome::Progress { remaining, release })
        }
    }

    pub fn retire_home_generation(&self) -> DraftMarkerSealRetireOutcome {
        let mut state = lock_state(&self.inner);
        let (released, settling_drives) = retire(&mut state, HomeLoss::GenerationChanged);
        DraftMarkerSealRetireOutcome {
            released,
            settling_drives,
        }
    }
}

fn settle_terminal(
    store: &HomeStore,
    storage: SyndicStorage,
    request: DraftMarkerSealFlightRequest,
    intent: DraftMarkerSealReleaseIntent,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
) -> Result<TerminalUpdate, DraftMarkerSealServiceError> {
    let key = request.seal_request().key();
    match storage.draft_marker_seal_status(store, key)? {
        DraftMarkerSealStatusV1::Absent => Ok(TerminalUpdate::Complete(
            DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(intent),
        )),
        DraftMarkerSealStatusV1::Open { .. } => {
            let revision = storage.revision(store)?;
            let mut command = HomeCommand::new(store.home_revision()?);
            let release = match intent {
                DraftMarkerSealReleaseIntent::Cancelled
                | DraftMarkerSealReleaseIntent::SessionDisposed
                | DraftMarkerSealReleaseIntent::ServiceDisposed => {
                    let prepared = storage.prepare_draft_marker_seal_cancel(store, key)?;
                    let release = prepared.release();
                    command.add(storage.cancel_draft_marker_seal(revision, prepared))?;
                    release
                }
                DraftMarkerSealReleaseIntent::Failed(reason) => {
                    let prepared = storage.prepare_draft_marker_seal_fail(store, key, reason)?;
                    let release = prepared.release();
                    command.add(storage.fail_draft_marker_seal(revision, prepared))?;
                    release
                }
                DraftMarkerSealReleaseIntent::Superseded {
                    successor_operation_id,
                    ..
                } => {
                    let prepared = storage.prepare_draft_marker_seal_supersede(
                        store,
                        key,
                        successor_operation_id,
                    )?;
                    let release = prepared.release();
                    command.add(storage.supersede_draft_marker_seal(revision, prepared))?;
                    release
                }
            };
            let outcome = execute_command(store, command, command_fault);
            match settle_command(store, outcome, storage, request, reconcile_fault)? {
                DurableCommandResult::ExactOld => Ok(TerminalUpdate::Keep(
                    DraftMarkerSealReleaseOutcome::NotCommitted(intent),
                )),
                DurableCommandResult::ExactNew => Ok(TerminalUpdate::Complete(
                    DraftMarkerSealReleaseOutcome::Settled { intent, release },
                )),
            }
        }
        DraftMarkerSealStatusV1::Cancelled(release) => {
            terminal_status(intent, DraftMarkerSealObservedTerminal::Cancelled, release)
        }
        DraftMarkerSealStatusV1::Failed { reason, release } => terminal_status(
            intent,
            DraftMarkerSealObservedTerminal::Failed(reason),
            release,
        ),
        DraftMarkerSealStatusV1::Superseded { successor, release } => terminal_status(
            intent,
            DraftMarkerSealObservedTerminal::Superseded(successor),
            release,
        ),
        DraftMarkerSealStatusV1::Sealed(_, _) => Ok(TerminalUpdate::Complete(
            DraftMarkerSealReleaseOutcome::ReleasedAfterSeal(intent),
        )),
    }
}

fn terminal_status(
    intent: DraftMarkerSealReleaseIntent,
    observed: DraftMarkerSealObservedTerminal,
    release: DraftMarkerSealCustodyReleaseV1,
) -> Result<TerminalUpdate, DraftMarkerSealServiceError> {
    let matches = match (intent, observed) {
        (
            DraftMarkerSealReleaseIntent::Cancelled
            | DraftMarkerSealReleaseIntent::SessionDisposed
            | DraftMarkerSealReleaseIntent::ServiceDisposed,
            DraftMarkerSealObservedTerminal::Cancelled,
        ) => true,
        (
            DraftMarkerSealReleaseIntent::Failed(expected),
            DraftMarkerSealObservedTerminal::Failed(actual),
        ) => expected == actual,
        (
            DraftMarkerSealReleaseIntent::Superseded {
                successor_operation_id,
                ..
            },
            DraftMarkerSealObservedTerminal::Superseded(actual),
        ) => successor_operation_id == actual,
        _ => false,
    };
    Ok(TerminalUpdate::Complete(if matches {
        DraftMarkerSealReleaseOutcome::Settled { intent, release }
    } else {
        DraftMarkerSealReleaseOutcome::ReleasedAfterOtherTerminal {
            requested: intent,
            observed,
        }
    }))
}

pub(super) fn finish_disposal(state: &mut super::ServiceState) {
    if state.flights.is_empty() && matches!(state.lifecycle, ServiceLifecycle::Disposing) {
        state.lifecycle = ServiceLifecycle::Disposed;
    }
}
