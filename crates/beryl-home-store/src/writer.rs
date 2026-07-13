use std::{cell::RefCell, sync::MutexGuard};

use fjall::PersistMode;

use crate::{
    CommandError, CommitReceipt, HomeCommand, RevisionConflict,
    command::{PendingAction, PendingMutation},
    domain::{RegisteredDomain, StoreInstanceId},
    fault::FaultPoint,
    health::FailureSeverity,
    metadata::{HOME_REVISION_KEY, encode_home_revision},
    read::{read_domain_metadata, read_home_revision},
    store::{HomeStore, StoreGeneration},
};

thread_local! {
    static ACTIVE_WRITERS: RefCell<Vec<StoreInstanceId>> = const { RefCell::new(Vec::new()) };
}

struct ActiveWriter {
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
    fn already_active(store: StoreInstanceId) -> bool {
        ACTIVE_WRITERS.with(|active| active.borrow().contains(&store))
    }

    fn enter(store: StoreInstanceId) -> Self {
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

struct PreparedContribution<'a> {
    contribution: &'a crate::MutationContribution,
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
        if command.cancellation.is_cancelled() {
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
        if command.cancellation.is_cancelled() {
            return Err(CommandError::CancelledBeforeAdmission);
        }
        let _active = ActiveWriter::enter(self.writer_id);
        let admission = self.health.admit()?;
        let _fail_closed_on_panic = FailClosedOnWriterPanic {
            health: &self.health,
        };

        if command.contributions.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
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
        let result = self.execute_admitted(generation, admission.generation(), command);
        if let Err(error) = &result {
            if let Some(severity) = command_failure_severity(error) {
                admission.fail(severity);
            }
            return result;
        }
        admission.confirm()?;
        result
    }

    fn acquire_writer(&self) -> Result<MutexGuard<'_, ()>, CommandError> {
        self.writer.lock().map_err(|_| CommandError::WriterPoisoned)
    }

    fn execute_admitted(
        &self,
        generation: &StoreGeneration,
        health_generation: crate::HomeGeneration,
        command: HomeCommand,
    ) -> Result<CommitReceipt, CommandError> {
        if command.sidecars.iter().any(|sidecar| {
            sidecar.store != generation.instance_id || sidecar.generation != health_generation
        }) {
            return Err(CommandError::ForeignSidecar);
        }
        let snapshot = generation.database.snapshot();
        let current_home = read_home_revision(&snapshot, generation.header_keyspace())
            .map_err(|source| CommandError::RevisionRead { source })?;
        let mut prepared = Vec::with_capacity(command.contributions.len());
        let mut conflicts = Vec::new();

        if current_home != command.expected_home_revision {
            conflicts.push(RevisionConflict::Home {
                expected: command.expected_home_revision,
                current: current_home,
            });
        }

        for contribution in &command.contributions {
            if contribution.store != generation.instance_id {
                return Err(CommandError::ForeignDomain {
                    domain: contribution.domain,
                });
            }
            let domain = generation
                .registry
                .get(contribution.slot)
                .filter(|domain| domain.name == contribution.domain)
                .ok_or(CommandError::ForeignDomain {
                    domain: contribution.domain,
                })?;
            let metadata = read_domain_metadata(
                &snapshot,
                generation.domains_keyspace(),
                contribution.domain,
            )
            .map_err(|source| CommandError::RevisionRead { source })?;
            if metadata != domain.metadata(metadata.revision) {
                return Err(CommandError::DomainRegistrationInvariant {
                    domain: contribution.domain,
                });
            }
            if metadata.revision != contribution.expected_revision {
                conflicts.push(RevisionConflict::Domain {
                    domain: contribution.domain,
                    expected: contribution.expected_revision,
                    current: metadata.revision,
                });
            }
            prepared.push(PreparedContribution {
                contribution,
                domain,
                current_revision: metadata.revision,
                pending: Vec::new(),
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
            participant.domain.validate(&snapshot).map_err(|source| {
                CommandError::DomainValidation {
                    domain: participant.contribution.domain,
                    source,
                }
            })?;
            participant
                .contribution
                .mutation
                .validate(&snapshot, participant.domain)
                .map_err(|source| CommandError::ContributorValidation {
                    domain: participant.contribution.domain,
                    source,
                })?;
        }
        for participant in &mut prepared {
            participant.pending = participant
                .contribution
                .mutation
                .assemble(&snapshot, participant.domain)
                .map_err(|source| CommandError::ContributorAssembly {
                    domain: participant.contribution.domain,
                    source,
                })?;
            if participant.pending.is_empty() {
                return Err(CommandError::EmptyContribution {
                    domain: participant.contribution.domain,
                });
            }
        }
        drop(snapshot);

        self.commit_prepared(generation, current_home, prepared)
    }

    fn commit_prepared(
        &self,
        generation: &StoreGeneration,
        current_home: beryl_model::HomeRevision,
        prepared: Vec<PreparedContribution<'_>>,
    ) -> Result<CommitReceipt, CommandError> {
        let next_home =
            current_home
                .checked_next()
                .map_err(|source| CommandError::RevisionExhausted {
                    scope: "home".to_owned(),
                    source,
                })?;
        let mut next_domains = Vec::with_capacity(prepared.len());
        for participant in &prepared {
            let next = participant
                .current_revision
                .checked_next()
                .map_err(|source| CommandError::RevisionExhausted {
                    scope: format!("domain `{}`", participant.domain.name),
                    source,
                })?;
            next_domains.push(next);
        }

        let mutation_count = prepared
            .iter()
            .map(|participant| participant.pending.len() + 1)
            .sum::<usize>()
            + 1;
        let mut batch =
            fjall::OwnedWriteBatch::with_capacity(generation.database.clone(), mutation_count)
                .durability(Some(PersistMode::Buffer));

        for (participant, next_revision) in prepared.iter().zip(&next_domains) {
            for mutation in &participant.pending {
                let family = participant
                    .domain
                    .families
                    .get(mutation.family_slot)
                    .expect("typed mutation family slot was resolved before assembly");
                match &mutation.action {
                    PendingAction::Put(value) => {
                        batch.insert(&family.keyspace, mutation.key.clone(), value.clone());
                    }
                    PendingAction::Delete => {
                        batch.remove(&family.keyspace, mutation.key.clone());
                    }
                }
            }

            let metadata = participant
                .domain
                .metadata(*next_revision)
                .encode()
                .map_err(|source| CommandError::Metadata {
                    source: Box::new(source),
                })?;
            batch.insert(
                generation.domains_keyspace(),
                participant.domain.name.as_bytes(),
                metadata,
            );
        }
        batch.insert(
            generation.header_keyspace(),
            HOME_REVISION_KEY,
            encode_home_revision(next_home),
        );

        self.faults
            .check(FaultPoint::BeforeCommit)
            .map_err(|source| CommandError::Commit {
                source: Box::new(source),
            })?;
        batch.commit().map_err(|source| CommandError::Commit {
            source: Box::new(source),
        })?;
        self.faults
            .check(FaultPoint::AfterCommitBeforePersist)
            .map_err(|source| CommandError::Persistence {
                source: Box::new(source),
            })?;
        generation
            .database
            .persist(PersistMode::SyncAll)
            .map_err(|source| CommandError::Persistence {
                source: Box::new(source),
            })?;
        self.faults
            .check(FaultPoint::AfterPersist)
            .map_err(|source| CommandError::Persistence {
                source: Box::new(source),
            })?;

        Ok(CommitReceipt {
            store: generation.instance_id,
            home_revision: next_home,
            domains: prepared
                .iter()
                .zip(next_domains)
                .map(|(participant, revision)| (participant.contribution.slot, revision))
                .collect(),
        })
    }
}

fn conflict_name(conflict: &RevisionConflict) -> &'static str {
    match conflict {
        RevisionConflict::Home { .. } => "",
        RevisionConflict::Domain { domain, .. } => domain,
    }
}

fn command_failure_severity(error: &CommandError) -> Option<FailureSeverity> {
    match error {
        CommandError::HealthGate(_)
        | CommandError::CancelledBeforeAdmission
        | CommandError::ReentrantWriter
        | CommandError::EmptyCommand
        | CommandError::ForeignDomain { .. }
        | CommandError::ForeignSidecar
        | CommandError::Conflict { .. }
        | CommandError::ContributorValidation { .. }
        | CommandError::ContributorAssembly { .. }
        | CommandError::EmptyContribution { .. }
        | CommandError::RevisionExhausted { .. }
        | CommandError::Metadata { .. } => None,
        CommandError::Commit { .. }
        | CommandError::Persistence { .. }
        | CommandError::RevisionRead { .. } => Some(FailureSeverity::Verify),
        CommandError::WriterPoisoned
        | CommandError::GenerationPoisoned
        | CommandError::DomainRegistrationInvariant { .. }
        | CommandError::DomainValidation { .. } => Some(FailureSeverity::Structural),
    }
}
