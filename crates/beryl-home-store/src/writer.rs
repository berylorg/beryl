use std::{cell::RefCell, sync::MutexGuard};

use fjall::PersistMode;

use crate::{
    CommandError, CommandOutcome, CommitReceipt, ContributorCallbackStage, CurrentDomainCommand,
    HomeCommand, MutationContribution, ReadError, ReadStage, ReconciliationCustody,
    RevisionConflict,
    command::{
        DomainParticipant, MaterializedDomainDescriptor, MaterializedRecordDescriptor,
        PendingAction, PendingMutation, ReconciliationReservationOutput,
    },
    domain::{RegisteredDomain, StoreInstanceId},
    fault::FaultPoint,
    health::FailureSeverity,
    read::{read_domain_metadata, read_home_revision},
    reconciliation::{ReconciliationReservationError, ReconciliationSlot},
    store::{HomeStore, StoreGeneration},
    successor::{
        SuccessorDescriptor, SuccessorProtocolIdentity, SuccessorRoleDescriptor,
        SuccessorRoleReservation,
    },
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

struct CommandReservation {
    slot: ReconciliationSlot,
    declarations: Vec<ReconciliationReservationOutput>,
    successor: Option<SuccessorReservation>,
    materialized: Option<(
        Vec<MaterializedDomainDescriptor>,
        CommitReceipt,
        Option<SuccessorDescriptor>,
    )>,
}

struct SuccessorReservation {
    identity: SuccessorProtocolIdentity,
    roles: Vec<(usize, SuccessorRoleReservation)>,
}

enum ExecutionOutcome {
    NotCommitted(CommandError),
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    Indeterminate(CommandError),
}

impl HomeStore {
    /// Executes one revision-checked command on the process-wide serialized writer.
    ///
    /// Cancellation is honored before writer admission only. After admission,
    /// validation, assembly, batch commit, and `SyncAll` complete as one
    /// synchronous result so a caller cannot abandon an indeterminate success.
    pub fn execute(&self, command: HomeCommand) -> CommandOutcome {
        let cancellation = command.cancellation.clone();
        if cancellation.is_cancelled() {
            return not_committed(CommandError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return not_committed(CommandError::ReentrantWriter);
        }
        if command.participants.is_empty() {
            return not_committed(CommandError::EmptyCommand);
        }
        if !command
            .participants
            .iter()
            .any(DomainParticipant::is_mutation)
        {
            return not_committed(CommandError::ValidationOnlyCommand);
        }
        let reservation = match self.reserve_home_command(&command) {
            Ok(reservation) => reservation,
            Err(error) => return not_committed(error),
        };
        self.execute_serialized(
            cancellation,
            reservation,
            |generation, health_generation, reservation| {
                self.execute_admitted(
                    generation,
                    health_generation,
                    command,
                    CommandFaultContext::unscoped(),
                    reservation,
                )
            },
        )
    }

    /// Executes one typed single-domain mutation after capturing its physical revisions under
    /// serialized writer admission.
    ///
    /// The mutation remains responsible for exact logical record-revision validation. This method
    /// removes only conflicts caused by unrelated commits between caller preparation and writer
    /// admission; it never retries a rejected mutation.
    pub fn execute_current(&self, command: CurrentDomainCommand) -> CommandOutcome {
        let cancellation = command.cancellation.clone();
        if cancellation.is_cancelled() {
            return not_committed(CommandError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return not_committed(CommandError::ReentrantWriter);
        }
        let reservation = match self.reserve_current_command(&command) {
            Ok(reservation) => reservation,
            Err(error) => return not_committed(error),
        };
        self.execute_serialized(
            cancellation,
            reservation,
            |generation, health_generation, reservation| {
                self.execute_current_admitted(generation, health_generation, command, reservation)
            },
        )
    }

    fn reserve_home_command(
        &self,
        command: &HomeCommand,
    ) -> Result<CommandReservation, CommandError> {
        let mut declarations = Vec::new();
        for participant in &command.participants {
            let Some(declaration) = participant.reserve_reconciliation() else {
                continue;
            };
            declarations.push(declaration.map_err(|source| {
                callback_command_error(
                    participant.domain(),
                    ContributorCallbackStage::Reservation,
                    source,
                )
            })?);
        }
        self.reserve_reconciliation(declarations)
    }

    fn reserve_current_command(
        &self,
        command: &CurrentDomainCommand,
    ) -> Result<CommandReservation, CommandError> {
        let declaration = command.plan.reserve_reconciliation().map_err(|source| {
            callback_command_error(
                command.plan.domain,
                ContributorCallbackStage::Reservation,
                source,
            )
        })?;
        self.reserve_reconciliation(vec![declaration])
    }

    fn reserve_reconciliation(
        &self,
        mut declarations: Vec<ReconciliationReservationOutput>,
    ) -> Result<CommandReservation, CommandError> {
        let successor = collect_successor_reservation(&mut declarations)?;
        let descriptor_bytes = declarations.iter().fold(0usize, |total, declaration| {
            total.saturating_add(declaration.descriptor_bytes)
        });
        let slot = self
            .reconciliation
            .reserve(descriptor_bytes)
            .map_err(|error| match error {
                ReconciliationReservationError::DescriptorTooLarge { requested, limit } => {
                    CommandError::ReconciliationDescriptorTooLarge { requested, limit }
                }
                ReconciliationReservationError::Capacity => CommandError::ReconciliationCapacity,
            })?;
        Ok(CommandReservation {
            slot,
            declarations,
            successor,
            materialized: None,
        })
    }

    fn execute_serialized(
        &self,
        cancellation: crate::CommandCancellation,
        mut reservation: CommandReservation,
        operation: impl FnOnce(
            &StoreGeneration,
            crate::HomeGeneration,
            &mut CommandReservation,
        ) -> ExecutionOutcome,
    ) -> CommandOutcome {
        if cancellation.is_cancelled() {
            return not_committed(CommandError::CancelledBeforeAdmission);
        }
        if ActiveWriter::already_active(self.writer_id) {
            return not_committed(CommandError::ReentrantWriter);
        }
        let _writer = match self.acquire_writer() {
            Ok(writer) => writer,
            Err(error) => {
                self.health.signal_failure(FailureSeverity::Structural);
                return not_committed(error);
            }
        };
        // Acquiring the mutex ends the wait; this terminal cancellation observation is the
        // admission handshake. No cancellation state is consulted after `ActiveWriter::enter`.
        if cancellation.is_cancelled() {
            return not_committed(CommandError::CancelledBeforeAdmission);
        }
        let _active = ActiveWriter::enter(self.writer_id);
        let admission = match self.health.admit() {
            Ok(admission) => admission,
            Err(error) => return not_committed(CommandError::HealthGate(error)),
        };
        let _fail_closed_on_panic = FailClosedOnWriterPanic {
            health: &self.health,
        };

        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return not_committed(CommandError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return not_committed(CommandError::GenerationPoisoned);
            }
        };
        let mut outcome = operation(generation, admission.generation(), &mut reservation);
        match &outcome {
            ExecutionOutcome::NotCommitted(error) => {
                if matches!(
                    command_failure_severity(error),
                    Some(FailureSeverity::Structural)
                ) {
                    admission.fail(FailureSeverity::Structural);
                }
            }
            ExecutionOutcome::Indeterminate(error) => {
                if matches!(
                    command_error::indeterminate_failure_severity(error),
                    Some(FailureSeverity::Structural)
                ) {
                    admission.fail(FailureSeverity::Structural);
                }
            }
            ExecutionOutcome::Committed {
                later_failure: Some(error),
                ..
            } if matches!(
                command_failure_severity(error),
                Some(FailureSeverity::Structural)
            ) =>
            {
                admission.fail(FailureSeverity::Structural);
            }
            _ => {}
        }
        if let ExecutionOutcome::Committed {
            later_failure: later_failure @ None,
            ..
        } = &mut outcome
        {
            if let Err(error) = admission.confirm_database(&generation.database, |source| {
                CommandError::Persistence {
                    source: Box::new(source),
                }
            }) {
                *later_failure = Some(error);
            }
        }
        finalize_outcome(outcome, reservation)
    }

    fn execute_current_admitted(
        &self,
        generation: &StoreGeneration,
        health_generation: crate::HomeGeneration,
        command: CurrentDomainCommand,
        reservation: &mut CommandReservation,
    ) -> ExecutionOutcome {
        let fault_context = CommandFaultContext::current(&command);
        let prepared = (|| -> Result<HomeCommand, CommandError> {
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
            let metadata =
                read_domain_metadata(&snapshot, generation.domains_keyspace(), plan.domain)
                    .map_err(|source| CommandError::RevisionRead { source })?;
            if metadata != domain.metadata(metadata.revision) {
                return Err(CommandError::DomainRegistrationInvariant {
                    domain: plan.domain,
                });
            }
            Ok(HomeCommand {
                expected_home_revision: current_home,
                cancellation,
                participants: vec![DomainParticipant::Mutation(MutationContribution {
                    plan,
                    expected_revision: metadata.revision,
                })],
                sidecars: Vec::new(),
            })
        })();
        let command = match prepared {
            Ok(command) => command,
            Err(error) => return ExecutionOutcome::NotCommitted(error),
        };
        self.execute_admitted(
            generation,
            health_generation,
            command,
            fault_context,
            reservation,
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
        reservation: &mut CommandReservation,
    ) -> ExecutionOutcome {
        let prepared_result = (|| {
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
            Ok((snapshot, current_home, mutations))
        })();
        let (snapshot, current_home, mutations) = match prepared_result {
            Ok(prepared) => prepared,
            Err(error) => return ExecutionOutcome::NotCommitted(error),
        };
        let receipt =
            match intended_receipt(generation, health_generation, current_home, &mutations) {
                Ok(receipt) => receipt,
                Err(error) => return ExecutionOutcome::NotCommitted(error),
            };
        if let Err(error) =
            materialize_reservation(&snapshot, &mutations, reservation, receipt.clone())
        {
            return ExecutionOutcome::NotCommitted(error);
        }
        drop(snapshot);

        self.commit_prepared(generation, receipt, mutations, fault_context)
    }

    fn commit_prepared(
        &self,
        generation: &StoreGeneration,
        receipt: CommitReceipt,
        prepared: Vec<PreparedMutation<'_>>,
        fault_context: CommandFaultContext,
    ) -> ExecutionOutcome {
        let assembled = match assemble(generation, &receipt, prepared) {
            Ok(assembled) => assembled,
            Err(error) => return ExecutionOutcome::NotCommitted(error),
        };

        if let Err(source) = self.check_writer_fault(FaultPoint::BeforeCommit, fault_context) {
            return ExecutionOutcome::NotCommitted(CommandError::Commit {
                source: Box::new(source),
            });
        }
        let mut committed_failure = None;
        if let Err(source) = assembled.batch.commit() {
            let state = source.commit_state();
            let error = commit_fjall_error(source);
            match state {
                Some(fjall::CommitState::NotCommitted) => {
                    return ExecutionOutcome::NotCommitted(error);
                }
                Some(fjall::CommitState::Committed) => committed_failure = Some(error),
                Some(fjall::CommitState::Indeterminate) | None => {
                    return ExecutionOutcome::Indeterminate(error);
                }
                Some(_) => return ExecutionOutcome::Indeterminate(error),
            }
        }
        if let Err(source) =
            self.check_writer_fault(FaultPoint::AfterCommitBeforePersist, fault_context)
        {
            return ExecutionOutcome::Indeterminate(with_committed_failure(
                committed_failure,
                CommandError::Persistence {
                    source: Box::new(source),
                },
            ));
        }
        if let Err(source) = generation.database.persist(PersistMode::SyncAll) {
            let error = with_committed_failure(committed_failure, persistence_fjall_error(source));
            return ExecutionOutcome::Indeterminate(error);
        }
        if let Err(source) = self.check_writer_fault(FaultPoint::AfterPersist, fault_context) {
            let failure = with_committed_failure(
                committed_failure,
                CommandError::Persistence {
                    source: Box::new(source),
                },
            );
            return ExecutionOutcome::Committed {
                receipt,
                later_failure: Some(failure),
            };
        }

        ExecutionOutcome::Committed {
            receipt,
            later_failure: committed_failure,
        }
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

fn intended_receipt(
    generation: &StoreGeneration,
    health_generation: crate::HomeGeneration,
    current_home: beryl_model::HomeRevision,
    prepared: &[PreparedMutation<'_>],
) -> Result<CommitReceipt, CommandError> {
    let home_revision =
        current_home
            .checked_next()
            .map_err(|source| CommandError::RevisionExhausted {
                scope: "home".to_owned(),
                source,
            })?;
    let mut domains = Vec::with_capacity(prepared.len());
    for participant in prepared {
        let revision = participant
            .current_revision
            .checked_next()
            .map_err(|source| CommandError::RevisionExhausted {
                scope: format!("domain `{}`", participant.domain.name),
                source,
            })?;
        domains.push((participant.participant.slot(), revision));
    }
    Ok(CommitReceipt {
        store: generation.instance_id,
        generation: health_generation,
        home_revision,
        domains,
    })
}

fn materialize_reservation(
    snapshot: &fjall::Snapshot,
    prepared: &[PreparedMutation<'_>],
    reservation: &mut CommandReservation,
    receipt: CommitReceipt,
) -> Result<(), CommandError> {
    if prepared.len() != reservation.declarations.len() {
        let participant = prepared
            .first()
            .map(|participant| participant.domain.name)
            .unwrap_or("unknown");
        return Err(CommandError::ReconciliationReservationMismatch {
            domain: participant,
            family: "unknown",
            reserved: reservation.declarations.len(),
            actual: prepared.len(),
        });
    }

    let mut domains = Vec::with_capacity(prepared.len());
    for (participant, declaration) in prepared.iter().zip(&reservation.declarations) {
        if declaration.domain != participant.domain.name {
            return Err(CommandError::ReconciliationReservationMismatch {
                domain: participant.domain.name,
                family: "unknown",
                reserved: 0,
                actual: participant.pending.len(),
            });
        }
        let intended_revision = receipt
            .domains
            .iter()
            .find_map(|(slot, revision)| {
                (*slot == participant.participant.slot()).then_some(*revision)
            })
            .expect("intended receipt contains every materialized domain");
        let mut actual = vec![0usize; declaration.quotas.len()];
        let mut records = Vec::with_capacity(participant.pending.len());

        for pending in &participant.pending {
            let family = participant
                .domain
                .families
                .get(pending.family_slot)
                .expect("typed mutation family slot was resolved during contribution");
            let Some(quota_index) = declaration.quotas.iter().position(|quota| {
                quota.family == family.logical_name && quota.codec_type == family.codec_type
            }) else {
                return Err(CommandError::ReconciliationReservationMismatch {
                    domain: participant.domain.name,
                    family: family.logical_name,
                    reserved: 0,
                    actual: 1,
                });
            };
            actual[quota_index] = actual[quota_index].saturating_add(1);
            if actual[quota_index] > declaration.quotas[quota_index].count {
                return Err(CommandError::ReconciliationReservationMismatch {
                    domain: participant.domain.name,
                    family: family.logical_name,
                    reserved: declaration.quotas[quota_index].count,
                    actual: actual[quota_index],
                });
            }

            let old = match snapshot
                .point(&family.keyspace, &pending.key)
                .map_err(|source| revision_snapshot_error(ReadStage::PointSize, source))?
            {
                Some(point) => {
                    let actual = usize::try_from(point.stored_value_len())
                        .expect("u32 always fits usize on supported targets");
                    if actual > family.max_stored_value_bytes {
                        return Err(CommandError::RevisionRead {
                            source: ReadError::InvalidStoredValueSize {
                                domain: participant.domain.name,
                                family: family.logical_name,
                                maximum: family.max_stored_value_bytes,
                                actual,
                            },
                        });
                    }
                    let pair = point
                        .acquire()
                        .map_err(|source| revision_snapshot_error(ReadStage::PointValue, source))?;
                    Some(pair.value().to_vec().into_boxed_slice())
                }
                None => None,
            };
            let new = match &pending.action {
                PendingAction::Put(value) => Some(value.clone().into_boxed_slice()),
                PendingAction::Delete => None,
            };
            records.push(MaterializedRecordDescriptor {
                family_slot: pending.family_slot,
                key: pending.key.clone().into_boxed_slice(),
                old,
                new,
            });
        }

        domains.push(MaterializedDomainDescriptor {
            domain_slot: participant.participant.slot(),
            intended_revision,
            records,
        });
    }
    let successor = reservation
        .successor
        .take()
        .map(|successor| SuccessorDescriptor {
            identity: successor.identity,
            roles: successor
                .roles
                .into_iter()
                .map(|(participant_index, role)| SuccessorRoleDescriptor {
                    domain_slot: prepared[participant_index].participant.slot(),
                    role,
                })
                .collect(),
        });
    reservation.materialized = Some((domains, receipt, successor));
    Ok(())
}

fn with_committed_failure(
    committed: Option<CommandError>,
    persistence: CommandError,
) -> CommandError {
    match committed {
        Some(commit) => CommandError::PersistenceAfterCommitFailure {
            commit: Box::new(commit),
            persistence: Box::new(persistence),
        },
        None => persistence,
    }
}

fn not_committed(evidence: CommandError) -> CommandOutcome {
    CommandOutcome::NotCommitted { evidence }
}

fn finalize_outcome(outcome: ExecutionOutcome, reservation: CommandReservation) -> CommandOutcome {
    match outcome {
        ExecutionOutcome::NotCommitted(evidence) => {
            drop(reservation);
            CommandOutcome::NotCommitted { evidence }
        }
        ExecutionOutcome::Committed {
            receipt,
            later_failure,
        } => {
            drop(reservation);
            CommandOutcome::Committed {
                receipt,
                later_failure,
            }
        }
        ExecutionOutcome::Indeterminate(failure) => {
            let CommandReservation {
                slot, materialized, ..
            } = reservation;
            let (domains, receipt, successor) = materialized
                .expect("every indeterminate mutation materialized its descriptor before Fjall");
            CommandOutcome::Indeterminate {
                failure,
                reconciliation: ReconciliationCustody::new(slot, domains, receipt, successor),
            }
        }
    }
}

fn collect_successor_reservation(
    declarations: &mut [ReconciliationReservationOutput],
) -> Result<Option<SuccessorReservation>, CommandError> {
    let mut identity = None;
    let mut source_count = 0usize;
    let mut roles = Vec::new();
    for (participant_index, declaration) in declarations.iter_mut().enumerate() {
        let Some(role) = declaration.successor.take() else {
            continue;
        };
        let role_identity = role.identity();
        if identity
            .is_some_and(|identity: SuccessorProtocolIdentity| !identity.matches(role_identity))
        {
            return Err(CommandError::InvalidSuccessorProtocol);
        }
        identity = Some(role_identity);
        if role.is_source() {
            source_count += 1;
        }
        roles.push((participant_index, role));
    }
    let Some(identity) = identity else {
        return Ok(None);
    };
    if source_count != 1 {
        return Err(CommandError::InvalidSuccessorProtocol);
    }
    Ok(Some(SuccessorReservation { identity, roles }))
}

fn conflict_name(conflict: &RevisionConflict) -> &'static str {
    match conflict {
        RevisionConflict::Home { .. } => "",
        RevisionConflict::Domain { domain, .. } => domain,
    }
}
