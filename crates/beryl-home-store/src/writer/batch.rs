use beryl_model::{DomainRevision, HomeRevision};

use crate::{
    CommandError,
    command::{DomainParticipant, PendingAction, PendingMutation},
    domain::RegisteredDomain,
    metadata::{HOME_REVISION_KEY, encode_home_revision},
    store::StoreGeneration,
};

use super::{
    PreparedMutation,
    command_error::{batch_accounting_overflow, commit_fjall_error},
};

pub(super) struct AssembledBatch {
    pub(super) batch: fjall::WriteBatch,
    pub(super) next_home: HomeRevision,
    pub(super) domains: Vec<(usize, DomainRevision)>,
}

struct BatchParticipant<'a> {
    participant: &'a DomainParticipant,
    domain: &'a RegisteredDomain,
    pending: Vec<PendingMutation>,
    next_revision: DomainRevision,
    encoded_metadata: Vec<u8>,
}

#[derive(Default)]
struct BatchTotals {
    records: u64,
    encoded_key_bytes: u64,
    encoded_value_bytes: u64,
}

impl BatchTotals {
    fn include(&mut self, key_bytes: usize, value_bytes: usize) -> Result<(), CommandError> {
        let key_bytes = u64::try_from(key_bytes).map_err(|_| batch_accounting_overflow())?;
        let value_bytes = u64::try_from(value_bytes).map_err(|_| batch_accounting_overflow())?;
        self.records = self
            .records
            .checked_add(1)
            .ok_or_else(batch_accounting_overflow)?;
        self.encoded_key_bytes = self
            .encoded_key_bytes
            .checked_add(key_bytes)
            .ok_or_else(batch_accounting_overflow)?;
        self.encoded_value_bytes = self
            .encoded_value_bytes
            .checked_add(value_bytes)
            .ok_or_else(batch_accounting_overflow)?;
        Ok(())
    }
}

pub(super) fn assemble(
    generation: &StoreGeneration,
    current_home: HomeRevision,
    prepared: Vec<PreparedMutation<'_>>,
) -> Result<AssembledBatch, CommandError> {
    let next_home =
        current_home
            .checked_next()
            .map_err(|source| CommandError::RevisionExhausted {
                scope: "home".to_owned(),
                source,
            })?;
    let mut participants = Vec::with_capacity(prepared.len());
    for participant in prepared {
        let next_revision = participant
            .current_revision
            .checked_next()
            .map_err(|source| CommandError::RevisionExhausted {
                scope: format!("domain `{}`", participant.domain.name),
                source,
            })?;
        let encoded_metadata = participant
            .domain
            .metadata(next_revision)
            .encode()
            .map_err(|source| CommandError::Metadata {
                source: Box::new(source),
            })?;
        participants.push(BatchParticipant {
            participant: participant.participant,
            domain: participant.domain,
            pending: participant.pending,
            next_revision,
            encoded_metadata,
        });
    }

    let encoded_home_revision = encode_home_revision(next_home);
    let mut totals = BatchTotals::default();
    for participant in &participants {
        for mutation in &participant.pending {
            let value_bytes = match &mutation.action {
                PendingAction::Put(value) => value.len(),
                PendingAction::Delete => 0,
            };
            totals.include(mutation.key.len(), value_bytes)?;
        }
        totals.include(
            participant.domain.name.len(),
            participant.encoded_metadata.len(),
        )?;
    }
    totals.include(HOME_REVISION_KEY.len(), encoded_home_revision.len())?;

    let capacity = generation
        .database
        .storage_policy()
        .batch_capacity(
            totals.records,
            totals.encoded_key_bytes,
            totals.encoded_value_bytes,
        )
        .map_err(commit_fjall_error)?;
    let mut batch = generation
        .database
        .batch(capacity, fjall::PersistMode::Buffer)
        .map_err(commit_fjall_error)?;
    let mut domains = Vec::with_capacity(participants.len());

    for participant in participants {
        for mutation in participant.pending {
            let family = participant
                .domain
                .families
                .get(mutation.family_slot)
                .expect("typed mutation family slot was resolved before assembly");
            match mutation.action {
                PendingAction::Put(value) => batch
                    .insert(
                        &family.keyspace,
                        mutation.key.into_boxed_slice(),
                        value.into_boxed_slice(),
                    )
                    .map_err(commit_fjall_error)?,
                PendingAction::Delete => batch
                    .remove(&family.keyspace, mutation.key.into_boxed_slice())
                    .map_err(commit_fjall_error)?,
            }
        }
        batch
            .insert(
                generation.domains_keyspace(),
                participant
                    .domain
                    .name
                    .as_bytes()
                    .to_vec()
                    .into_boxed_slice(),
                participant.encoded_metadata.into_boxed_slice(),
            )
            .map_err(commit_fjall_error)?;
        domains.push((participant.participant.slot(), participant.next_revision));
    }
    batch
        .insert(
            generation.header_keyspace(),
            HOME_REVISION_KEY.to_vec().into_boxed_slice(),
            encoded_home_revision.into_boxed_slice(),
        )
        .map_err(commit_fjall_error)?;

    Ok(AssembledBatch {
        batch,
        next_home,
        domains,
    })
}
