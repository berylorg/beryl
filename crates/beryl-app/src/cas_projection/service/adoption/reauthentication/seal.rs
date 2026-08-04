use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{
    AdoptedProjectionCandidateReauthenticationLedger, ProjectionCandidateId,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateReauthenticationReason,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
    model::{
        CandidateLedgerEntry, CandidateSetConvergedAdoptedProjectionConnectionService,
        SealedDormantRecoveredProjectionInventory,
    },
    transaction,
};

pub(super) fn seal(
    mut ledger: AdoptedProjectionCandidateReauthenticationLedger,
) -> Result<
    CandidateSetConvergedAdoptedProjectionConnectionService,
    ProjectionCandidateLedgerSealFailure,
> {
    if let Some(reason) = ledger.terminal_reason {
        return Err(ProjectionCandidateLedgerSealFailure::Terminal(
            TerminalAdoptedProjectionConnectionService::new(reason, ledger),
        ));
    }
    if let Some(candidate_id) = first_outstanding_candidate(&ledger) {
        return Err(ProjectionCandidateLedgerSealError::new(
            ProjectionCandidateLedgerSealReason::OutstandingCandidate,
            Some(candidate_id),
            ledger,
        )
        .into());
    }

    let ledger_metadata = ledger.metadata();
    let accepted_capacity = ledger_metadata.accepted_count();
    let connection_owner_capacity = ledger_metadata.connection_owner_count();
    let mut accepted = Vec::new();
    let mut accepted_candidate_ids = Vec::new();
    let mut accepted_observations = Vec::new();
    if accepted.try_reserve_exact(accepted_capacity).is_err()
        || accepted_candidate_ids
            .try_reserve_exact(accepted_capacity)
            .is_err()
        || accepted_observations
            .try_reserve_exact(accepted_capacity)
            .is_err()
    {
        return Err(ProjectionCandidateLedgerSealError::new(
            ProjectionCandidateLedgerSealReason::AcceptedInventoryCapacityUnavailable,
            None,
            ledger,
        )
        .into());
    }

    let initial_service_authentication = catch_unwind(AssertUnwindSafe(|| {
        transaction::validate_recovered_service(
            ledger
                .attempt
                .as_ref()
                .expect("an owning ledger retains its adoption attempt"),
        )
    }));
    match initial_service_authentication {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => {
            return Err(classify_service_error(ledger, reason));
        }
        Err(_) => {
            return Err(classify_service_error(
                ledger,
                ProjectionCandidateReauthenticationReason::UnexpectedUnwind,
            ));
        }
    }

    for group_index in 0..ledger.groups.len() {
        for candidate_index in 0..ledger.groups[group_index].entries.len() {
            let candidate_id = ProjectionCandidateId::new(group_index, candidate_index);
            let result = {
                let entry = ledger.groups[group_index].entries[candidate_index]
                    .as_ref()
                    .expect("a seal never observes a vacant ledger entry");
                let CandidateLedgerEntry::Accepted(accepted) = entry else {
                    continue;
                };
                catch_unwind(AssertUnwindSafe(|| {
                    transaction::authenticate_accepted_candidate(
                        ledger
                            .attempt
                            .as_ref()
                            .expect("an owning ledger retains its adoption attempt"),
                        accepted,
                    )
                }))
                .unwrap_or(Err(
                    ProjectionCandidateReauthenticationReason::UnexpectedUnwind,
                ))
            };
            if let Err(reason) = result {
                if let ProjectionCandidateReauthenticationReason::ServiceTerminal(reason) = reason {
                    return Err(terminal_failure(ledger, reason));
                }
                demote_accepted(&mut ledger, candidate_id, reason);
                return Err(ProjectionCandidateLedgerSealError::new(
                    ProjectionCandidateLedgerSealReason::AcceptedCandidateAuthenticationFailed,
                    Some(candidate_id),
                    ledger,
                )
                .into());
            }
            let CandidateLedgerEntry::Accepted(accepted) = ledger.groups[group_index].entries
                [candidate_index]
                .as_ref()
                .expect("the authenticated accepted entry remains owned by the ledger")
            else {
                unreachable!("the authenticated seal entry remains accepted")
            };
            accepted_candidate_ids.push(candidate_id);
            accepted_observations.push(accepted.owner().registry_observation().clone());
        }
    }

    let final_service_authentication = catch_unwind(AssertUnwindSafe(|| {
        transaction::validate_recovered_service(
            ledger
                .attempt
                .as_ref()
                .expect("an owning ledger retains its adoption attempt"),
        )
    }));
    match final_service_authentication {
        Ok(Ok(())) => {}
        Ok(Err(reason)) => return Err(classify_service_error(ledger, reason)),
        Err(_) => {
            return Err(classify_service_error(
                ledger,
                ProjectionCandidateReauthenticationReason::UnexpectedUnwind,
            ));
        }
    }

    #[cfg(test)]
    let test_pauses = ledger.test_pauses.clone();
    let connection_transfer = catch_unwind(AssertUnwindSafe(|| {
        transaction::seal_candidate_set_connections(
            ledger
                .attempt
                .as_ref()
                .expect("an owning ledger retains its adoption attempt"),
            &mut ledger.connection_owners,
            &accepted_candidate_ids,
            &accepted_observations,
            #[cfg(test)]
            &test_pauses,
        )
    }));
    let connection_owners = match connection_transfer {
        Ok(Ok(owners)) => owners,
        Ok(Err(transaction::CandidateSetSealFailure::AcceptedCandidate {
            reason,
            candidate_id,
        })) => {
            demote_accepted(&mut ledger, candidate_id, reason);
            return Err(ProjectionCandidateLedgerSealError::new(
                ProjectionCandidateLedgerSealReason::AcceptedCandidateAuthenticationFailed,
                Some(candidate_id),
                ledger,
            )
            .into());
        }
        Ok(Err(transaction::CandidateSetSealFailure::RetryableConnectionOwnerCapacity)) => {
            return Err(ProjectionCandidateLedgerSealError::new(
                ProjectionCandidateLedgerSealReason::ConnectionOwnerCapacityUnavailable,
                None,
                ledger,
            )
            .into());
        }
        Ok(Err(transaction::CandidateSetSealFailure::Terminal(reason))) => {
            return Err(terminal_failure(ledger, reason));
        }
        Err(_) => {
            return Err(ProjectionCandidateLedgerSealError::new(
                ProjectionCandidateLedgerSealReason::UnexpectedUnwind,
                None,
                ledger,
            )
            .into());
        }
    };
    if connection_owners.len() != connection_owner_capacity {
        unreachable!("the atomic connection-owner transfer preserves the complete exact set")
    }
    for group in &mut ledger.groups {
        for entry in &mut group.entries {
            match entry
                .take()
                .expect("a terminal ledger retains every entry until seal")
            {
                CandidateLedgerEntry::Accepted(candidate) => accepted.push(candidate),
                CandidateLedgerEntry::Disposed => {}
                CandidateLedgerEntry::Unprocessed(_) | CandidateLedgerEntry::Rejected { .. } => {
                    unreachable!("seal checked complete candidate convergence")
                }
            }
        }
    }
    debug_assert_eq!(accepted.len(), accepted_capacity);
    Ok(CandidateSetConvergedAdoptedProjectionConnectionService {
        attempt: ledger.attempt.take(),
        accepted_inventory: Some(SealedDormantRecoveredProjectionInventory::new(
            accepted,
            connection_owners,
        )),
    })
}

fn first_outstanding_candidate(
    ledger: &AdoptedProjectionCandidateReauthenticationLedger,
) -> Option<ProjectionCandidateId> {
    for (group_index, group) in ledger.groups.iter().enumerate() {
        for (candidate_index, entry) in group.entries.iter().enumerate() {
            if matches!(
                entry
                    .as_ref()
                    .expect("a ledger entry is vacant only during a synchronous transition"),
                CandidateLedgerEntry::Unprocessed(_) | CandidateLedgerEntry::Rejected { .. }
            ) {
                return Some(ProjectionCandidateId::new(group_index, candidate_index));
            }
        }
    }
    None
}

fn classify_service_error(
    ledger: AdoptedProjectionCandidateReauthenticationLedger,
    reason: ProjectionCandidateReauthenticationReason,
) -> ProjectionCandidateLedgerSealFailure {
    if let ProjectionCandidateReauthenticationReason::ServiceTerminal(reason) = reason {
        return terminal_failure(ledger, reason);
    }
    ProjectionCandidateLedgerSealError::new(
        ProjectionCandidateLedgerSealReason::UnexpectedUnwind,
        None,
        ledger,
    )
    .into()
}

fn terminal_failure(
    mut ledger: AdoptedProjectionCandidateReauthenticationLedger,
    reason: TerminalAdoptedProjectionConnectionServiceReason,
) -> ProjectionCandidateLedgerSealFailure {
    ledger.mark_terminal(reason);
    ProjectionCandidateLedgerSealFailure::Terminal(TerminalAdoptedProjectionConnectionService::new(
        reason, ledger,
    ))
}

fn demote_accepted(
    ledger: &mut AdoptedProjectionCandidateReauthenticationLedger,
    candidate_id: ProjectionCandidateId,
    reason: ProjectionCandidateReauthenticationReason,
) {
    let entry = ledger.groups[candidate_id.group_index()].entries[candidate_id.candidate_index()]
        .take()
        .expect("seal demotion takes one exact accepted entry");
    let CandidateLedgerEntry::Accepted(accepted) = entry else {
        unreachable!("seal demotion targets one authenticated accepted entry")
    };
    ledger.groups[candidate_id.group_index()].entries[candidate_id.candidate_index()] =
        Some(CandidateLedgerEntry::Rejected {
            owner: accepted.into_pending_owner(),
            reason,
        });
}
