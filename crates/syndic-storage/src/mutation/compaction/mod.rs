use beryl_home_store::{
    CurrentDomainCommand, DomainMutation, DomainReader, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{
    BerylHomeId, BindingRevision, CasThreadId, CasTurnId, DomainRevision, InputGateRevision,
    ProjectionRevision, SyndicItemId, SyndicThreadId,
};

use crate::{
    ActiveCasTurnRecord, BindingLifecycle, BindingState, CasThreadBindingIndexRecord,
    CasTurnIndexRecord, CompactionAbandonmentReason, CompactionAttemptNonce,
    CompactionContinuationReceipt, CompactionMarkerLifecycle, CompactionMarkerObservation,
    CompactionOperationId, CompactionOperationRecord, CompactionOperationRevision,
    CompactionOperationState, CompactionOperationTarget, CompactionProviderSequence,
    CompactionRequestDisposition, CompactionSettlement, CompactionSettlementReceiptRecord,
    CompactionThreadStatus, ContentLifecycle, ContentManifestRecord, ConversationParent,
    ExecutionSnapshotKind, ExecutionSnapshotRecord, InputGateRecord, InputGateState,
    ProviderOperationKind, StopDispositionSource, StopMatchingTerminalWitness, StopOperationId,
    StopOperationRecord, StopOperationState, SyndicMutationError, SyndicStorage, SyndicTimestamp,
    TurnDepth, TurnEndStatus, TurnKind, TurnLifecycle, TurnRecord, TurnStateRecord,
    TurnStateRevision, codec::*, domain::SyndicDomain, root_turn_chain_digest,
};

use super::{point, required};

/// Exact immutable authority for admitting one context-compaction operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmitCompactionOperation {
    home_id: BerylHomeId,
    operation_id: CompactionOperationId,
    target: CompactionOperationTarget,
    expected_gate_revision: InputGateRevision,
    attempt: CompactionAttemptNonce,
    started_at: SyndicTimestamp,
}

impl AdmitCompactionOperation {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub(crate) const fn new(
        home_id: BerylHomeId,
        operation_id: CompactionOperationId,
        target: CompactionOperationTarget,
        expected_gate_revision: InputGateRevision,
        attempt: CompactionAttemptNonce,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            home_id,
            operation_id,
            target,
            expected_gate_revision,
            attempt,
            started_at,
        }
    }
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    #[must_use]
    pub const fn operation_id(&self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn target(&self) -> &CompactionOperationTarget {
        &self.target
    }
    #[must_use]
    pub const fn expected_gate_revision(&self) -> InputGateRevision {
        self.expected_gate_revision
    }
    #[must_use]
    pub const fn attempt(&self) -> CompactionAttemptNonce {
        self.attempt
    }
    #[must_use]
    pub const fn started_at(&self) -> SyndicTimestamp {
        self.started_at
    }
}

/// One-way claim of the sole compact-start request attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClaimCompactionDispatch {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    attempt: CompactionAttemptNonce,
}

impl ClaimCompactionDispatch {
    #[must_use]
    pub const fn new(
        operation_id: CompactionOperationId,
        expected_operation_revision: CompactionOperationRevision,
        attempt: CompactionAttemptNonce,
    ) -> Self {
        Self {
            operation_id,
            expected_operation_revision,
            attempt,
        }
    }
    #[must_use]
    pub const fn operation_id(self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn attempt(self) -> CompactionAttemptNonce {
        self.attempt
    }
}

/// Independently ordered result of the compact-start request itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishCompactionRequestDisposition {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    attempt: CompactionAttemptNonce,
    disposition: CompactionRequestDisposition,
}

impl PublishCompactionRequestDisposition {
    #[must_use]
    pub const fn new(
        operation_id: CompactionOperationId,
        expected_operation_revision: CompactionOperationRevision,
        attempt: CompactionAttemptNonce,
        disposition: CompactionRequestDisposition,
    ) -> Self {
        Self {
            operation_id,
            expected_operation_revision,
            attempt,
            disposition,
        }
    }
    #[must_use]
    pub const fn operation_id(self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn attempt(self) -> CompactionAttemptNonce {
        self.attempt
    }
    #[must_use]
    pub const fn disposition(self) -> CompactionRequestDisposition {
        self.disposition
    }
}

/// One exact provider observation in the compaction operation's contiguous order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionProviderEvent {
    ThreadStatus(CompactionThreadStatus),
    TurnStarted(CasTurnId),
    Marker {
        item_id: SyndicItemId,
        lifecycle: CompactionMarkerLifecycle,
    },
    Terminal(TurnEndStatus),
}

/// Publishes one exact provider observation and its durable operation frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishCompactionProviderEvent {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    sequence: CompactionProviderSequence,
    event: CompactionProviderEvent,
    observed_at: SyndicTimestamp,
}

impl PublishCompactionProviderEvent {
    #[must_use]
    pub const fn new(
        operation_id: CompactionOperationId,
        expected_operation_revision: CompactionOperationRevision,
        sequence: CompactionProviderSequence,
        event: CompactionProviderEvent,
        observed_at: SyndicTimestamp,
    ) -> Self {
        Self {
            operation_id,
            expected_operation_revision,
            sequence,
            event,
            observed_at,
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(&self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn sequence(&self) -> CompactionProviderSequence {
        self.sequence
    }
    #[must_use]
    pub const fn event(&self) -> &CompactionProviderEvent {
        &self.event
    }
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }
}

/// Explicit non-abandonment consumption of one exact compaction operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettleCompactionOperation {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    settlement: CompactionSettlement,
}

impl SettleCompactionOperation {
    #[must_use]
    pub const fn new(
        operation_id: CompactionOperationId,
        expected_operation_revision: CompactionOperationRevision,
        settlement: CompactionSettlement,
    ) -> Self {
        Self {
            operation_id,
            expected_operation_revision,
            settlement,
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(&self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn settlement(&self) -> &CompactionSettlement {
        &self.settlement
    }
}

/// Conservative consumption after classified compaction authority loss.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbandonCompactionOperation {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    reason: CompactionAbandonmentReason,
}

/// Exact final building frontier to seal as the fixed ownerless lifecycle content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealLifecycleContinuationContent {
    expected: ContentManifestRecord,
}

/// One serialized lifecycle settlement candidate; accepted-next work still wins atomically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettleLifecycleCompaction {
    operation_id: CompactionOperationId,
    expected_operation_revision: CompactionOperationRevision,
    content: crate::ContentReference,
    turn_id: beryl_model::SyndicTurnId,
    item_id: SyndicItemId,
    settled_at: SyndicTimestamp,
}

impl SettleLifecycleCompaction {
    #[must_use]
    pub fn new(
        operation: &CompactionOperationRecord,
        content: crate::ContentReference,
        settled_at: SyndicTimestamp,
    ) -> Self {
        let turn_id = crate::derive_lifecycle_continuation_turn_id(
            operation.home_id(),
            operation.id(),
            content.summary().digest(),
        );
        let item_id = crate::derive_lifecycle_continuation_item_id(
            operation.home_id(),
            operation.id(),
            content.summary().digest(),
        );
        Self {
            operation_id: operation.id(),
            expected_operation_revision: operation.revision(),
            content,
            turn_id,
            item_id,
            settled_at,
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(&self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn content(&self) -> crate::ContentReference {
        self.content
    }
    #[must_use]
    pub const fn turn_id(&self) -> beryl_model::SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn settled_at(&self) -> SyndicTimestamp {
        self.settled_at
    }
}

impl SealLifecycleContinuationContent {
    #[must_use]
    pub const fn new(expected: ContentManifestRecord) -> Self {
        Self { expected }
    }
    #[must_use]
    pub const fn expected(&self) -> &ContentManifestRecord {
        &self.expected
    }
}

impl AbandonCompactionOperation {
    #[must_use]
    pub const fn new(
        operation_id: CompactionOperationId,
        expected_operation_revision: CompactionOperationRevision,
        reason: CompactionAbandonmentReason,
    ) -> Self {
        Self {
            operation_id,
            expected_operation_revision,
            reason,
        }
    }
    #[must_use]
    pub const fn operation_id(self) -> CompactionOperationId {
        self.operation_id
    }
    #[must_use]
    pub const fn expected_operation_revision(self) -> CompactionOperationRevision {
        self.expected_operation_revision
    }
    #[must_use]
    pub const fn reason(self) -> CompactionAbandonmentReason {
        self.reason
    }
}

struct AdmitMutation(AdmitCompactionOperation);
struct ClaimMutation(ClaimCompactionDispatch);
struct RequestMutation(PublishCompactionRequestDisposition);
struct ProviderMutation(PublishCompactionProviderEvent);
struct SettleMutation(SettleCompactionOperation);
struct SealLifecycleContentMutation(SealLifecycleContinuationContent);
struct SettleLifecycleMutation(SettleLifecycleCompaction);

impl DomainMutation<SyndicDomain> for SealLifecycleContentMutation {
    type Error = SyndicMutationError;
    type Prepared = ContentManifestRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.successor(reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ContentManifestsCodec>(&prepared.id(), &prepared)?;
        Ok(())
    }
}

impl SealLifecycleContentMutation {
    fn successor(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<ContentManifestRecord, SyndicMutationError> {
        let current = required::<ContentManifestsFamily>(reader, &self.0.expected.id())?;
        let prepared = crate::prepare_lifecycle_continuation_content()?;
        if current != self.0.expected
            || current.lifecycle() != ContentLifecycle::Building
            || current.id() != prepared.id()
            || current.encoding() != prepared.encoding()
            || current.expected() != prepared.summary()
            || current.chunk_count() != prepared.summary().chunk_count()
            || current.encoded_bytes() != prepared.summary().encoded_bytes()
            || current.chain_digest() != prepared.summary().digest()
        {
            return Err(SyndicMutationError::ContentManifestConflict);
        }
        Ok(ContentManifestRecord::new(
            current.id(),
            current.revision().checked_next()?,
            current.encoding(),
            ContentLifecycle::Sealed,
            current.chunk_count(),
            current.encoded_bytes(),
            current.chain_digest(),
            current.expected(),
        ))
    }
}

mod admission;
mod api;
mod continuation;
mod observation;
mod settlement;

use admission::{live_operation, provider_event_operation, provider_terminal_stop};
use settlement::SettlementRecords;
