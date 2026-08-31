use std::{error::Error, fmt};

use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, HomeStore, ReconciliationHandle,
    ReconciliationResolution,
};

use crate::{
    SyndicStorage,
    admission_attachment::{CancelTransient, DraftMarkerAdmissionPreparedAttempt},
};

use super::{
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionHeadsFamily,
    DraftMarkerAdmissionLifecycleV1, DraftMarkerAdmissionOwnerV1,
};

pub(crate) mod closure;
mod mutation;
mod settlement;

#[cfg(feature = "test-faults")]
mod test_fixture;

#[cfg(feature = "test-faults")]
pub use test_fixture::DraftMarkerAdmissionTerminalReceiptFaultV1;

use closure::{
    ExactTerminalClosure, TerminalClosureError, read_terminal_closure_from_store,
    terminal_nodes_empty_from_store, validate_compact_terminal_charge,
};
use mutation::{
    TerminalMutation, TerminalMutationFailureClass, TerminalMutationMode,
    classify_terminal_mutation_failure,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMarkerAdmissionTerminalRefusalV1 {
    Unavailable,
    Rejected,
    Stale,
}

#[derive(Debug)]
pub enum DraftMarkerAdmissionTerminalErrorV1 {
    Read(crate::SyndicReadError),
    Unavailable,
}

impl fmt::Display for DraftMarkerAdmissionTerminalErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "draft-marker terminal read failed: {error}"),
            Self::Unavailable => {
                formatter.write_str("draft-marker terminal boundary is unavailable")
            }
        }
    }
}

impl Error for DraftMarkerAdmissionTerminalErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::Unavailable => None,
        }
    }
}

pub struct DraftMarkerAdmissionTerminalFlightV1 {
    owner: DraftMarkerAdmissionOwnerV1,
    handle: ReconciliationHandle,
    retry_failed: bool,
    kind: TerminalFlightKind,
}

enum TerminalFlightKind {
    Cancellation,
    Cleanup,
}

pub enum DraftMarkerAdmissionTerminalOutcomeV1 {
    ReleasedTransient,
    Advanced {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    Replayed,
    RetainedClosure,
    Retryable,
    Refused(DraftMarkerAdmissionTerminalRefusalV1),
    Collision,
    ReconciliationPending(DraftMarkerAdmissionTerminalFlightV1),
}

impl SyndicStorage {
    #[cfg(feature = "test-faults")]
    pub fn transfer_draft_marker_admission_terminal_to_settlement_for_test(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        terminal_command: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<CommandOutcome, DraftMarkerAdmissionTerminalErrorV1> {
        let head = self
            .point::<DraftMarkerAdmissionHeadsFamily>(
                store,
                owner,
                crate::draft_piece::point_limit(),
            )
            .map_err(DraftMarkerAdmissionTerminalErrorV1::Read)?
            .ok_or(DraftMarkerAdmissionTerminalErrorV1::Unavailable)?;
        let closure = self
            .compact_terminal_closure(store, &head)
            .map_err(|error| match error {
                TerminalClosureError::Read(error) => {
                    DraftMarkerAdmissionTerminalErrorV1::Read(error.into())
                }
                TerminalClosureError::Invalid => DraftMarkerAdmissionTerminalErrorV1::Unavailable,
            })?
            .ok_or(DraftMarkerAdmissionTerminalErrorV1::Unavailable)?;
        if closure.key.command_id() != terminal_command {
            return Err(DraftMarkerAdmissionTerminalErrorV1::Unavailable);
        }
        let authority = settlement::DraftMarkerAdmissionSettlementAuthorityV1::new(
            owner,
            terminal_command,
            closure.receipt.digest(),
        );
        Ok(store.execute_current(settlement::settlement_transfer_command(self, authority)))
    }

    pub fn cancel_draft_marker_admission(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> DraftMarkerAdmissionTerminalOutcomeV1 {
        let head = match self.point::<DraftMarkerAdmissionHeadsFamily>(
            store,
            owner,
            crate::draft_piece::point_limit(),
        ) {
            Ok(head) => head,
            Err(_) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable),
        };
        let Some(head) = head else {
            return match store
                .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                    attachment.cancel_transient(owner)
                }) {
                Ok(Ok(CancelTransient::Released)) => {
                    DraftMarkerAdmissionTerminalOutcomeV1::ReleasedTransient
                }
                Ok(Ok(CancelTransient::Absent | CancelTransient::Protected)) => {
                    refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected)
                }
                _ => refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable),
            };
        };
        if head.lifecycle() == DraftMarkerAdmissionLifecycleV1::TerminalCleanup {
            return self.classify_terminal_replay(store, &head, command);
        }
        let reconstructed_inert = match store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.is_reconstructed_inert_cleanup(owner)
            }) {
            Ok(Ok(value)) => value,
            _ => return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable),
        };
        if reconstructed_inert {
            return refused(DraftMarkerAdmissionTerminalRefusalV1::Stale);
        }
        let Some(generation) = store.health().generation() else {
            return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
        };
        if head.home_generation().get() != generation.get() {
            return refused(DraftMarkerAdmissionTerminalRefusalV1::Stale);
        }
        if !matches!(
            head.lifecycle(),
            DraftMarkerAdmissionLifecycleV1::Ingesting
                | DraftMarkerAdmissionLifecycleV1::Assigning
                | DraftMarkerAdmissionLifecycleV1::Ready
        ) {
            return refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected);
        }
        let reservation = match store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.prepare_terminal_attempt(owner, command)
            }) {
            Ok(Ok(reservation)) => reservation,
            Ok(Err(())) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected),
            Err(_) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable),
        };
        self.execute_terminal_mutation(
            store,
            owner,
            command,
            TerminalMutationMode::CancelCurrent(generation.get()),
            Some(reservation),
        )
    }

    pub fn advance_draft_marker_admission_cleanup(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> DraftMarkerAdmissionTerminalOutcomeV1 {
        let Some(generation) = store.health().generation() else {
            return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
        };
        let head = match self.point::<DraftMarkerAdmissionHeadsFamily>(
            store,
            owner,
            crate::draft_piece::point_limit(),
        ) {
            Ok(Some(head)) => head,
            Ok(None) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected),
            Err(_) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable),
        };
        match head.lifecycle() {
            DraftMarkerAdmissionLifecycleV1::Ingesting
            | DraftMarkerAdmissionLifecycleV1::Assigning
            | DraftMarkerAdmissionLifecycleV1::Ready
                if head.home_generation().get() != generation.get() => {}
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup => {
                match self.compact_terminal_closure(store, &head) {
                    Ok(Some(_)) => {
                        self.finish_cleanup_schedule(store, owner, true);
                        return DraftMarkerAdmissionTerminalOutcomeV1::RetainedClosure;
                    }
                    Ok(None) => {}
                    Err(TerminalClosureError::Invalid) => {
                        return DraftMarkerAdmissionTerminalOutcomeV1::Collision;
                    }
                    Err(TerminalClosureError::Read(_)) => {
                        return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                    }
                }
            }
            _ => return refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected),
        }
        self.execute_terminal_mutation(
            store,
            owner,
            command,
            TerminalMutationMode::Cleanup(generation.get()),
            None,
        )
    }

    pub fn resolve_draft_marker_admission_terminal(
        &self,
        store: &HomeStore,
        flight: DraftMarkerAdmissionTerminalFlightV1,
    ) -> DraftMarkerAdmissionTerminalOutcomeV1 {
        let resolution = if flight.retry_failed {
            store.retry_reconciliation(&flight.handle)
        } else {
            store.reconcile(&flight.handle)
        };
        match resolution {
            Ok(ReconciliationResolution::ExactOld) => {
                if let TerminalFlightKind::Cancellation = flight.kind {
                    if !self.resolve_terminal_attachment(store, flight.owner, false, false) {
                        return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                    }
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Retryable
            }
            Ok(ReconciliationResolution::ExactNew { receipt }) => {
                match flight.kind {
                    TerminalFlightKind::Cancellation => {
                        if !self.resolve_terminal_attachment(store, flight.owner, true, false) {
                            return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                        }
                    }
                    TerminalFlightKind::Cleanup => {
                        self.finish_cleanup_schedule(store, flight.owner, false)
                    }
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Advanced {
                    receipt,
                    later_failure: None,
                }
            }
            Ok(ReconciliationResolution::Collision)
            | Ok(ReconciliationResolution::ExactSuccessor { .. }) => {
                if let TerminalFlightKind::Cancellation = flight.kind {
                    let _ = self.resolve_terminal_attachment(store, flight.owner, false, true);
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Collision
            }
            Err(_) => DraftMarkerAdmissionTerminalOutcomeV1::ReconciliationPending(
                DraftMarkerAdmissionTerminalFlightV1 {
                    retry_failed: true,
                    ..flight
                },
            ),
        }
    }

    pub fn next_inert_draft_marker_admission_cleanup(
        &self,
        store: &HomeStore,
    ) -> Result<Option<DraftMarkerAdmissionOwnerV1>, DraftMarkerAdmissionTerminalErrorV1> {
        store
            .with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.next_inert_cleanup()
            })
            .map_err(|_| DraftMarkerAdmissionTerminalErrorV1::Unavailable)?
            .map_err(|_| DraftMarkerAdmissionTerminalErrorV1::Unavailable)
    }

    fn execute_terminal_mutation(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        mode: TerminalMutationMode,
        reservation: Option<DraftMarkerAdmissionPreparedAttempt>,
    ) -> DraftMarkerAdmissionTerminalOutcomeV1 {
        let cancellation = reservation.is_some();
        let _reservation = match reservation {
            Some(reservation) => match reservation.disarm() {
                Ok(reservation) => Some(reservation),
                Err(()) => return refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected),
            },
            None => None,
        };
        match store.execute_current(self.handle.current_command(TerminalMutation {
            owner,
            command,
            mode,
        })) {
            CommandOutcome::NotCommitted { evidence } => {
                let class = classify_terminal_mutation_failure(&evidence);
                if cancellation {
                    let uncertain = matches!(class, TerminalMutationFailureClass::Collision);
                    if !self.finish_terminal_attachment(store, owner, command, false, uncertain) {
                        return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                    }
                }
                match class {
                    TerminalMutationFailureClass::Retryable => {
                        DraftMarkerAdmissionTerminalOutcomeV1::Retryable
                    }
                    TerminalMutationFailureClass::Collision => {
                        DraftMarkerAdmissionTerminalOutcomeV1::Collision
                    }
                    TerminalMutationFailureClass::Rejected => {
                        refused(DraftMarkerAdmissionTerminalRefusalV1::Rejected)
                    }
                    TerminalMutationFailureClass::Unavailable => {
                        refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable)
                    }
                }
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                if let Some(local_finalization) = local_finalization {
                    let finalized = store.with_committed_local_finalization(
                        local_finalization,
                        &receipt,
                        &self.handle,
                        |attachment| {
                            if cancellation {
                                attachment.finish_terminal(owner, command, true, false)
                            } else {
                                attachment.finish_cleanup(owner, false);
                                Ok(())
                            }
                        },
                    );
                    if !matches!(finalized, Ok(Ok(()))) {
                        return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                    }
                    return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                }
                if cancellation {
                    if !self.finish_terminal_attachment(store, owner, command, true, false) {
                        return refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable);
                    }
                } else {
                    self.finish_cleanup_schedule(store, owner, false);
                }
                DraftMarkerAdmissionTerminalOutcomeV1::Advanced {
                    receipt,
                    later_failure,
                }
            }
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                if cancellation {
                    let _ = self.finish_terminal_attachment(store, owner, command, false, true);
                }
                DraftMarkerAdmissionTerminalOutcomeV1::ReconciliationPending(
                    DraftMarkerAdmissionTerminalFlightV1 {
                        owner,
                        handle: reconciliation.install_and_handle(),
                        retry_failed: false,
                        kind: if cancellation {
                            TerminalFlightKind::Cancellation
                        } else {
                            TerminalFlightKind::Cleanup
                        },
                    },
                )
            }
        }
    }

    fn classify_terminal_replay(
        &self,
        store: &HomeStore,
        head: &DraftMarkerAdmissionHeadV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> DraftMarkerAdmissionTerminalOutcomeV1 {
        match read_terminal_closure_from_store(self, store, head) {
            Ok(closure) if closure.key.command_id() == command => {
                DraftMarkerAdmissionTerminalOutcomeV1::Replayed
            }
            Ok(_) | Err(TerminalClosureError::Invalid) => {
                DraftMarkerAdmissionTerminalOutcomeV1::Collision
            }
            Err(TerminalClosureError::Read(_)) => {
                refused(DraftMarkerAdmissionTerminalRefusalV1::Unavailable)
            }
        }
    }

    fn compact_terminal_closure(
        &self,
        store: &HomeStore,
        head: &DraftMarkerAdmissionHeadV1,
    ) -> Result<Option<ExactTerminalClosure>, TerminalClosureError> {
        if head.charge().associations() != 0
            || head.source_root().count() != 0
            || head.target_root().count() != 0
        {
            return Ok(None);
        }
        if !terminal_nodes_empty_from_store(self, store, head.owner())
            .map_err(TerminalClosureError::Read)?
        {
            return Ok(None);
        }
        let closure = read_terminal_closure_from_store(self, store, head)?;
        validate_compact_terminal_charge(head, &closure)?;
        Ok(Some(closure))
    }

    fn finish_terminal_attachment(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        exact_new: bool,
        uncertain: bool,
    ) -> bool {
        matches!(
            store.with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.finish_terminal(owner, command, exact_new, uncertain)
            }),
            Ok(Ok(()))
        )
    }

    fn resolve_terminal_attachment(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        exact_new: bool,
        uncertain: bool,
    ) -> bool {
        matches!(
            store.with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
                attachment.resolve_terminal(owner, exact_new, uncertain)
            }),
            Ok(Ok(()))
        )
    }

    fn finish_cleanup_schedule(
        &self,
        store: &HomeStore,
        owner: DraftMarkerAdmissionOwnerV1,
        retained: bool,
    ) {
        let _ = store.with_domain_attachment(&self.handle.attachment_capability(), |attachment| {
            attachment.finish_cleanup(owner, retained);
        });
    }
}

fn refused(
    refusal: DraftMarkerAdmissionTerminalRefusalV1,
) -> DraftMarkerAdmissionTerminalOutcomeV1 {
    DraftMarkerAdmissionTerminalOutcomeV1::Refused(refusal)
}
