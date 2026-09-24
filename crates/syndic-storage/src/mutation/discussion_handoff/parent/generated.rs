use super::*;
use beryl_model::{
    ContentRevision, ProjectionRevision, SyndicAcceptedInputId, SyndicDraftId, SyndicItemId,
    SyndicTurnId,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod changes;
mod content;
mod outcome;
mod records;
use changes::Change;

#[derive(Clone, Debug)]
pub struct GeneratedDiscussionInput {
    pub parent_turn_id: SyndicTurnId,
    pub canonical_item_id: SyndicItemId,
    pub resolution: String,
    pub admitted_at: SyndicTimestamp,
}

#[derive(Clone, Debug)]
pub struct GeneratedDiscussionInputLookup {
    pub home_id: BerylHomeId,
    pub parent_thread_id: SyndicThreadId,
    pub child_thread_id: SyndicThreadId,
    pub intent_id: ResolutionIntentId,
    pub job_id: JobId,
    pub context_owner: DiscussionContextOwnerId,
    pub context_digest: DiscussionContextDigest,
    pub resolving_turn_id: SyndicTurnId,
    pub parent_turn_id: SyndicTurnId,
    pub canonical_item_id: SyndicItemId,
    pub resolution: String,
}

#[derive(Clone)]
pub struct GeneratedDiscussionInputIntent {
    home_id: BerylHomeId,
    input: AcceptedInputRecord,
    changes: Arc<[Change]>,
}

impl GeneratedDiscussionInputIntent {
    pub fn input(&self) -> &AcceptedInputRecord {
        &self.input
    }
}

pub struct PreparedGeneratedDiscussionInput {
    parent: PreparedDiscussionParent,
    intent: GeneratedDiscussionInputIntent,
}

impl PreparedGeneratedDiscussionInput {
    pub fn intent(&self) -> GeneratedDiscussionInputIntent {
        self.intent.clone()
    }
    pub fn into_contribution(self) -> MutationContribution {
        self.parent
            .handle
            .clone()
            .contribution(self.parent.revision, self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedDiscussionInputStatus {
    ExactOld,
    ExactNew,
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedDiscussionInputDiscovery {
    Absent,
    Exact,
    Collision,
}

impl SyndicStorage {
    pub fn prepare_generated_discussion_input(
        &self,
        store: &HomeStore,
        parent: PreparedDiscussionParent,
        request: GeneratedDiscussionInput,
    ) -> Result<PreparedGeneratedDiscussionInput, SyndicMutationError> {
        if parent.home_id != store.home_id()
            || parent.disposition != DiscussionParentDisposition::Ready
            || store.domain_revision(&parent.handle)? != parent.revision
            || self.revision(store)? != parent.revision
        {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        let reader = ParentRead {
            storage: self,
            access: ReadAccess::Ordinary(store),
        };
        if probe::classify(&reader, parent.request)? != Some(DiscussionParentDisposition::Ready) {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        let content = prepare_resolution_content(&request.resolution)?;
        let resolution_digest = Sha256::digest(request.resolution.as_bytes()).into();
        let (input, changes) =
            records::prepare(&reader, &parent, &request, resolution_digest, content)?;
        if store.domain_revision(&parent.handle)? != parent.revision
            || self.revision(store)? != parent.revision
        {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(PreparedGeneratedDiscussionInput {
            parent,
            intent: GeneratedDiscussionInputIntent {
                home_id: store.home_id(),
                input,
                changes: changes.into(),
            },
        })
    }
}

fn prepare_resolution_content(resolution: &str) -> Result<PreparedContent, SyndicMutationError> {
    if resolution.is_empty() || resolution.len() > 262_144 || resolution.chars().count() > 65_536 {
        return Err(SyndicRecordError::InvalidGeneratedHandoffInput.into());
    }
    Ok(PreparedContent::composer(&ComposerPayload::new(vec![
        ComposerAtom::text(format!("Discussion resolution:\n\n{resolution}"))?,
    ])?)?)
}

impl DomainMutation<SyndicDomain> for PreparedGeneratedDiscussionInput {
    type Error = SyndicMutationError;
    type Prepared = Self;
    fn prepare(self, reader: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        self.parent.validate_parent(reader)?;
        for change in self.intent.changes.iter() {
            if !change.matches(reader)?.0 {
                return Err(SyndicMutationError::DiscussionHandoffConflict);
            }
        }
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        Change::reserve_all(&self.intent.changes, reservation)
    }
    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for change in prepared.intent.changes.iter() {
            change.contribute(mutations)?;
        }
        Ok(())
    }
}

fn required<F: Family>(
    reader: &impl TerminalHistoryReader,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError> {
    reader
        .read::<F>(key)?
        .ok_or(SyndicMutationError::DiscussionHandoffConflict)
}
