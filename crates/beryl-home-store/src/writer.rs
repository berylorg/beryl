use std::{cell::RefCell, sync::MutexGuard};

use fjall::PersistMode;

use crate::{
    CommandError, CommitReceipt, ContributorCallbackStage, CurrentDomainCommand, HomeCommand,
    MutationContribution, ReadStage, RevisionConflict,
    command::{DomainParticipant, PendingMutation},
    domain::{RegisteredDomain, StoreInstanceId},
    fault::FaultPoint,
    health::FailureSeverity,
    read::{read_domain_metadata, read_home_revision},
    store::{HomeStore, StoreGeneration},
};

mod batch;
mod command_error;
mod fault_context;

use batch::assemble;
use command_error::{
    callback_command_error, command_failure_severity, commit_fjall_error, persistence_fjall_error,
    revision_snapshot_error,
};
use fault_context::CommandFaultContext;

thread_local! {
    static ACTIVE_WRITERS: RefCell<Vec<StoreInstanceId>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct ActiveWriter {
    store: StoreInstanceId,
}

struct FailClosedOnWriterPanic<'a> {
    health: &'a crate::health::HealthGate,
}

impl Drop for FailClosedOnWriterPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.health.signal_failure(FailureSeverity::Structural);
        }
    }
}

impl ActiveWriter {
    pub(crate) fn already_active(store: StoreInstanceId) -> bool {
        ACTIVE_WRITERS.with(|active| active.borrow().contains(&store))
    }

    pub(crate) fn enter(store: StoreInstanceId) -> Self {
        ACTIVE_WRITERS.with(|active| active.borrow_mut().push(store));
        Self { store }
    }
}

impl Drop for ActiveWriter {
    fn drop(&mut self) {
        ACTIVE_WRITERS.with(|active| {
            let removed = active.borrow_mut().pop();
            debug_assert_eq!(removed, Some(self.store));
        });
    }
}

struct PreparedParticipant<'a> {
    participant: &'a DomainParticipant,
    domain: &'a RegisteredDomain,
    current_revision: beryl_model::DomainRevision,
}

struct PreparedMutation<'a> {
    participant: &'a DomainParticipant,
    domain: &'a RegisteredDomain,
    current_revision: beryl_model::DomainRevision,
    pending: Vec<PendingMutation>,
}

impl HomeStore {
    /// Executes one revision-checked command on the process-wide serialized writer.
    ///
    /// Cancellation is honored before writer admission only. After admission,
    /// validation, assembly, batch commit, and `SyncAll` complete as one
    /// synchronous result so a caller cannot abandon an indeterminate success.
    pub fn execute(&self, command: HomeCommand) -> Result<CommitReceipt, CommandError> {
        let cancellation = command.cancellation.clone();
        self.execute_serialized(cancellation, |generation, health_generation| {
            self.execute_admitted(
                generation,
                health_generation,
                command,
                CommandFaultContext::unscoped(),
            )
        })
    }

    /// Executes one typed single-domain mutation after capturing its physical revisions under
    /// serialized writer admission.
    ///
    /// The mutation remains responsible for exact logical record-revision validation. This method
    /// removes only conflicts caused by unrelated commits between caller preparation and writer
    /// admission; it never retries a rejected mutation.
    pub fn execute_current(
        &self,
        command: CurrentDomainCommand,
    ) -> Result<CommitReceipt, CommandError> {
        let cancellation = command.cancellation.clone();
        self.execute_serialized(cancellation, |generation, health_generation| {
            self.execute_current_admitted(generation, health_generation, command)
        })
    }

    fn execute_serialized(
        &self,
        cancellation: crate::CommandCancellation,
        operation: impl FnOnce(
            &StoreGeneration,
            crate::HomeGeneration,
        ) -> Result<CommitReceipt, CommandError>,
    ) -> Result<CommitReceipt, CommandError> {
        if cancellation.is_cancelled() {
            return Err(CommandError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return Err(CommandError::ReentrantWriter);
        }
        let _writer = match self.acquire_writer() {
            Ok(writer) => writer,
            Err(error) => {
                self.health.signal_failure(FailureSeverity::Structural);
                return Err(error);
            }
        };
        if cancellation.is_cancelled() {
            return Err(CommandError::CancelledBeforeAdmission);
        }
        let _active = ActiveWriter::enter(self.writer_id);
        let admission = self.health.admit()?;
        let _fail_closed_on_panic = FailClosedOnWriterPanic {
            health: &self.health,
        };

        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(CommandError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(CommandError::GenerationPoisoned);
            }
        };
        let result = operation(generation, admission.generation());
        if let Err(error) = &result {
            if let Some(severity) = command_failure_severity(error) {
                admission.fail(severity);
            }
            return result;
        }
        admission.confirm_database(&generation.database, |source| CommandError::Persistence {
            source: Box::new(source),
        })?;
        result
    }

    fn execute_current_admitted(
        &self,
        generation: &StoreGeneration,
        health_generation: crate::HomeGeneration,
        command: CurrentDomainCommand,
    ) -> Result<CommitReceipt, CommandError> {
        let fault_context = CommandFaultContext::current(&command);
        let CurrentDomainCommand {
            plan, cancellation, ..
        } = command;
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
        let snapshot = generation
            .database
            .snapshot()
            .map_err(|source| revision_snapshot_error(ReadStage::HomeRevision, source))?;
        let current_home = read_home_revision(&snapshot, generation.header_keyspace())
            .map_err(|source| CommandError::RevisionRead { source })?;
        let metadata = read_domain_metadata(&snapshot, generation.domains_keyspace(), plan.domain)
            .map_err(|source| CommandError::RevisionRead { source })?;
        if metadata != domain.metadata(metadata.revision) {
            return Err(CommandError::DomainRegistrationInvariant {
                domain: plan.domain,
            });
        }
        drop(snapshot);
        self.execute_admitted(
            generation,
            health_generation,
            HomeCommand {
                expected_home_revision: current_home,
                cancellation,
                participants: vec![DomainParticipant::Mutation(MutationContribution {
                    plan,
                    expected_revision: metadata.revision,
                })],
                sidecars: Vec::new(),
            },
            fault_context,
        )
    }

    fn acquire_writer(&self) -> Result<MutexGuard<'_, ()>, CommandError> {
        self.writer.lock().map_err(|_| CommandError::WriterPoisoned)
    }

    fn execute_admitted(
        &self,
        generation: &StoreGeneration,
        health_generation: crate::HomeGeneration,
        command: HomeCommand,
        fault_context: CommandFaultContext,
    ) -> Result<CommitReceipt, CommandError> {
        if command.participants.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let mutation_count = command
            .participants
            .iter()
            .filter(|participant| participant.is_mutation())
            .count();
        if mutation_count == 0 {
            return Err(CommandError::ValidationOnlyCommand);
        }
        if command.sidecars.iter().any(|sidecar| {
            sidecar.store != generation.instance_id || sidecar.generation != health_generation
        }) {
            return Err(CommandError::ForeignSidecar);
        }
        let snapshot = generation
            .database
            .snapshot()
            .map_err(|source| revision_snapshot_error(ReadStage::HomeRevision, source))?;
        let current_home = read_home_revision(&snapshot, generation.header_keyspace())
            .map_err(|source| CommandError::RevisionRead { source })?;
        let mut prepared = Vec::with_capacity(command.participants.len());
        let mut conflicts = Vec::new();

        if current_home != command.expected_home_revision {
            conflicts.push(RevisionConflict::Home {
                expected: command.expected_home_revision,
                current: current_home,
            });
        }

        for participant in &command.participants {
            if participant.store() != generation.instance_id {
                return Err(CommandError::ForeignDomain {
                    domain: participant.domain(),
                });
            }
            let domain = generation
                .registry
                .get(participant.slot())
                .filter(|domain| {
                    domain.name == participant.domain() && domain.owner == participant.owner()
                })
                .ok_or(CommandError::ForeignDomain {
                    domain: participant.domain(),
                })?;
            let metadata = read_domain_metadata(
                &snapshot,
                generation.domains_keyspace(),
                participant.domain(),
            )
            .map_err(|source| CommandError::RevisionRead { source })?;
            if metadata != domain.metadata(metadata.revision) {
                return Err(CommandError::DomainRegistrationInvariant {
                    domain: participant.domain(),
                });
            }
            if metadata.revision != participant.expected_revision() {
                conflicts.push(RevisionConflict::Domain {
                    domain: participant.domain(),
                    expected: participant.expected_revision(),
                    current: metadata.revision,
                });
            }
            prepared.push(PreparedParticipant {
                participant,
                domain,
                current_revision: metadata.revision,
            });
        }

        if !conflicts.is_empty() {
            conflicts.sort_by(|left, right| conflict_name(left).cmp(conflict_name(right)));
            return Err(CommandError::Conflict {
                conflicts_len: conflicts.len(),
                conflicts,
            });
        }

        for participant in &prepared {
            participant
                .participant
                .validate(&snapshot, participant.domain)
                .map_err(|source| {
                    callback_command_error(
                        participant.participant.domain(),
                        ContributorCallbackStage::Validation,
                        source,
                    )
                })?;
        }
        let mut mutations = Vec::with_capacity(mutation_count);
        for participant in prepared {
            let Some(pending) = participant
                .participant
                .assemble_mutation(&snapshot, participant.domain)
            else {
                continue;
            };
            let pending = pending.map_err(|source| {
                callback_command_error(
                    participant.participant.domain(),
                    ContributorCallbackStage::Contribution,
                    source,
                )
            })?;
            if pending.is_empty() {
                return Err(CommandError::EmptyContribution {
                    domain: participant.participant.domain(),
                });
            }
            mutations.push(PreparedMutation {
                participant: participant.participant,
                domain: participant.domain,
                current_revision: participant.current_revision,
                pending,
            });
        }
        drop(snapshot);

        self.commit_prepared(
            generation,
            health_generation,
            current_home,
            mutations,
            fault_context,
        )
    }

    fn commit_prepared(
        &self,
        generation: &StoreGeneration,
        health_generation: crate::HomeGeneration,
        current_home: beryl_model::HomeRevision,
        prepared: Vec<PreparedMutation<'_>>,
        fault_context: CommandFaultContext,
    ) -> Result<CommitReceipt, CommandError> {
        let assembled = assemble(generation, current_home, prepared)?;

        self.check_writer_fault(FaultPoint::BeforeCommit, fault_context)
            .map_err(|source| CommandError::Commit {
                source: Box::new(source),
            })?;
        assembled.batch.commit().map_err(commit_fjall_error)?;
        self.check_writer_fault(FaultPoint::AfterCommitBeforePersist, fault_context)
            .map_err(|source| CommandError::Persistence {
                source: Box::new(source),
            })?;
        generation
            .database
            .persist(PersistMode::SyncAll)
            .map_err(persistence_fjall_error)?;
        self.check_writer_fault(FaultPoint::AfterPersist, fault_context)
            .map_err(|source| CommandError::Persistence {
                source: Box::new(source),
            })?;

        Ok(CommitReceipt {
            store: generation.instance_id,
            generation: health_generation,
            home_revision: assembled.next_home,
            domains: assembled.domains,
        })
    }

    #[cfg(feature = "test-faults")]
    fn check_writer_fault(
        &self,
        point: FaultPoint,
        context: CommandFaultContext,
    ) -> std::io::Result<()> {
        match context.scope {
            Some(scope) => self.faults.check_current(point, scope),
            None => self.faults.check(point),
        }
    }

    #[cfg(not(feature = "test-faults"))]
    fn check_writer_fault(
        &self,
        point: FaultPoint,
        _context: CommandFaultContext,
    ) -> std::io::Result<()> {
        self.faults.check(point)
    }
}

fn conflict_name(conflict: &RevisionConflict) -> &'static str {
    match conflict {
        RevisionConflict::Home { .. } => "",
        RevisionConflict::Domain { domain, .. } => domain,
    }
}
