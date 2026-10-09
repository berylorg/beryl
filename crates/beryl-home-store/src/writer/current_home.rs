use super::{
    ActiveWriter, CommandFaultContext, CommandReservation, ExecutionOutcome,
    callback_command_error, not_committed, read_domain_metadata, read_home_revision,
    revision_snapshot_error,
};
use crate::{
    CommandError, CommandOutcome, ContributorCallbackStage, CurrentHomeCommand, HomeCommand,
    HomeGeneration, HomeStore, MutationContribution, ReadStage,
    candidate_access::StoreOperationAccess, command::DomainParticipant, store::StoreGeneration,
};

impl HomeStore {
    pub fn execute_current_home(&self, command: CurrentHomeCommand) -> CommandOutcome {
        self.execute_current_home_with_access(StoreOperationAccess::Ordinary, command)
    }

    pub(crate) fn execute_current_home_with_access(
        &self,
        access: StoreOperationAccess,
        command: CurrentHomeCommand,
    ) -> CommandOutcome {
        let cancellation = command.cancellation_signals();
        let cancelled = || {
            cancellation
                .iter()
                .any(crate::CommandCancellation::is_cancelled)
        };
        if cancelled() {
            return not_committed(CommandError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return not_committed(CommandError::ReentrantWriter);
        }
        let declarations = command
            .commands
            .iter()
            .map(|command| {
                command.plan.reserve_reconciliation().map_err(|source| {
                    callback_command_error(
                        command.plan.domain,
                        ContributorCallbackStage::Reservation,
                        source,
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>();
        let reservation = match declarations.and_then(|value| self.reserve_reconciliation(value)) {
            Ok(reservation) => reservation,
            Err(error) => return not_committed(error),
        };
        self.execute_serialized(
            access,
            cancelled,
            reservation,
            |generation, health_generation, reservation| {
                self.execute_current_home_admitted(
                    generation,
                    health_generation,
                    command,
                    reservation,
                )
            },
        )
    }

    fn execute_current_home_admitted(
        &self,
        generation: &StoreGeneration,
        health_generation: HomeGeneration,
        command: CurrentHomeCommand,
        reservation: &mut CommandReservation,
    ) -> ExecutionOutcome {
        let fault_context = CommandFaultContext::current(&command.commands[0]);
        let prepared = (|| -> Result<HomeCommand, CommandError> {
            let snapshot = generation
                .database
                .snapshot()
                .map_err(|source| revision_snapshot_error(ReadStage::HomeRevision, source))?;
            let current_home = read_home_revision(&snapshot, generation.header_keyspace())
                .map_err(|source| CommandError::RevisionRead { source })?;
            let mut participants = Vec::with_capacity(command.commands.len());
            for command in command.commands {
                let plan = command.plan;
                if plan.store != generation.instance_id {
                    return Err(CommandError::ForeignDomain {
                        domain: plan.domain,
                    });
                }
                let domain = generation
                    .registry
                    .get(plan.slot)
                    .filter(|domain| domain.name == plan.domain && domain.owner == plan.owner)
                    .ok_or(CommandError::ForeignDomain {
                        domain: plan.domain,
                    })?;
                let metadata =
                    read_domain_metadata(&snapshot, generation.domains_keyspace(), plan.domain)
                        .map_err(|source| CommandError::RevisionRead { source })?;
                if metadata != domain.metadata(metadata.revision) {
                    return Err(CommandError::DomainRegistrationInvariant {
                        domain: plan.domain,
                    });
                }
                participants.push(DomainParticipant::Mutation(MutationContribution {
                    plan,
                    expected_revision: metadata.revision,
                }));
            }
            Ok(HomeCommand {
                expected_home_revision: current_home,
                cancellation: command.cancellation,
                participants,
                sidecars: Vec::new(),
                #[cfg(feature = "test-faults")]
                test_fault_scope: None,
            })
        })();
        match prepared {
            Ok(command) => self.execute_admitted(
                generation,
                health_generation,
                command,
                fault_context,
                reservation,
            ),
            Err(error) => ExecutionOutcome::NotCommitted(error),
        }
    }
}
