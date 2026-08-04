use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration, CasNativeTurnCount,
    CasThreadId, CasTurnId, ExecutionBinding, InputGateRevision, SyndicExecutionSnapshotId,
    SyndicThreadId, SyndicTurnId, ThreadRevision,
};

use crate::{
    BindingLifecycle, CasLineageProof, CasRepresentedPrefixProof, SelectedPathProof,
    SyndicRecordError, SyndicTimestamp,
};

use super::{MAX_REASON_BYTES, validate_text};

/// CAS-thread facts shared by valid and active projection bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UsableCasBinding {
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
    represented_prefix: CasRepresentedPrefixProof,
    native_turn_count: CasNativeTurnCount,
    tool_profile: CasConversationToolProfile,
    lineage: CasLineageProof,
}

impl UsableCasBinding {
    #[must_use]
    pub const fn new(
        execution: ExecutionBinding,
        cas_thread_id: CasThreadId,
        represented_prefix: CasRepresentedPrefixProof,
        native_turn_count: CasNativeTurnCount,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
    ) -> Self {
        Self {
            execution,
            cas_thread_id,
            represented_prefix,
            native_turn_count,
            tool_profile,
            lineage,
        }
    }
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn represented_prefix(&self) -> CasRepresentedPrefixProof {
        self.represented_prefix
    }
    /// Returns the exact number of actual CAS model turns represented by the prefix.
    #[must_use]
    pub const fn native_turn_count(&self) -> CasNativeTurnCount {
        self.native_turn_count
    }
    /// Returns the exact canonical conversation-tool profile carried by this CAS lineage.
    #[must_use]
    pub const fn tool_profile(&self) -> CasConversationToolProfile {
        self.tool_profile
    }
    #[must_use]
    pub const fn lineage(&self) -> CasLineageProof {
        self.lineage
    }

    pub(crate) fn advance_represented_source_revision(
        &self,
        source_thread_revision: ThreadRevision,
    ) -> Option<Self> {
        let represented = self.represented_prefix;
        if source_thread_revision < represented.source_thread_revision() {
            return None;
        }
        Some(Self::new(
            self.execution.clone(),
            self.cas_thread_id.clone(),
            CasRepresentedPrefixProof::new(
                represented.tail(),
                source_thread_revision,
                represented.digest(),
            ),
            self.native_turn_count,
            self.tool_profile,
            self.lineage,
        ))
    }
}

/// Additional immutable correlation accepted when one binding became active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveCasBinding {
    usable: UsableCasBinding,
    snapshot_id: SyndicExecutionSnapshotId,
    turn_id: SyndicTurnId,
    activation_gate_revision: InputGateRevision,
    started_at: SyndicTimestamp,
}

impl ActiveCasBinding {
    #[must_use]
    pub const fn new(
        usable: UsableCasBinding,
        snapshot_id: SyndicExecutionSnapshotId,
        turn_id: SyndicTurnId,
        activation_gate_revision: InputGateRevision,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            usable,
            snapshot_id,
            turn_id,
            activation_gate_revision,
            started_at,
        }
    }
    #[must_use]
    pub const fn usable(&self) -> &UsableCasBinding {
        &self.usable
    }
    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn activation_gate_revision(&self) -> InputGateRevision {
        self.activation_gate_revision
    }
    #[must_use]
    pub const fn started_at(&self) -> SyndicTimestamp {
        self.started_at
    }
}

/// Bounded non-authorizing provenance for one unusable or abandoned CAS thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaleCasBinding {
    execution: ExecutionBinding,
    cas_thread_id: CasThreadId,
    observed_tool_profile: Option<CasConversationToolProfile>,
    observed_prefix: Option<CasRepresentedPrefixProof>,
    observed_lineage: Option<CasLineageProof>,
    observed_native_turn_count: Option<CasNativeTurnCount>,
    loaded_generation: Option<CasLoadedSessionGeneration>,
    reason: Box<str>,
    observed_at: SyndicTimestamp,
}

impl StaleCasBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        execution: ExecutionBinding,
        cas_thread_id: CasThreadId,
        observed_tool_profile: Option<CasConversationToolProfile>,
        observed_prefix: Option<CasRepresentedPrefixProof>,
        observed_lineage: Option<CasLineageProof>,
        observed_native_turn_count: Option<CasNativeTurnCount>,
        loaded_generation: Option<CasLoadedSessionGeneration>,
        reason: impl AsRef<str>,
        observed_at: SyndicTimestamp,
    ) -> Result<Self, SyndicRecordError> {
        Ok(Self {
            execution,
            cas_thread_id,
            observed_tool_profile,
            observed_prefix,
            observed_lineage,
            observed_native_turn_count,
            loaded_generation,
            reason: validate_text("stale reason", reason.as_ref(), MAX_REASON_BYTES, false)?,
            observed_at,
        })
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    /// Returns the exact observed tool profile retained as non-authorizing provenance.
    #[must_use]
    pub const fn observed_tool_profile(&self) -> Option<CasConversationToolProfile> {
        self.observed_tool_profile
    }
    #[must_use]
    pub const fn observed_prefix(&self) -> Option<CasRepresentedPrefixProof> {
        self.observed_prefix
    }
    #[must_use]
    pub const fn observed_lineage(&self) -> Option<CasLineageProof> {
        self.observed_lineage
    }
    /// Returns the exact native turn count retained as non-authorizing provenance.
    #[must_use]
    pub const fn observed_native_turn_count(&self) -> Option<CasNativeTurnCount> {
        self.observed_native_turn_count
    }
    #[must_use]
    pub const fn loaded_generation(&self) -> Option<CasLoadedSessionGeneration> {
        self.loaded_generation
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    #[must_use]
    pub const fn observed_at(&self) -> SyndicTimestamp {
        self.observed_at
    }
}

/// Immutable state carried by one revision in binding history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingState {
    Unbound { reason: Box<str> },
    Valid(UsableCasBinding),
    Active(ActiveCasBinding),
    Stale(StaleCasBinding),
}

impl BindingState {
    pub fn unbound(reason: impl AsRef<str>) -> Result<Self, SyndicRecordError> {
        Ok(Self::Unbound {
            reason: validate_text("unbound reason", reason.as_ref(), MAX_REASON_BYTES, false)?,
        })
    }
    #[must_use]
    pub const fn valid(binding: UsableCasBinding) -> Self {
        Self::Valid(binding)
    }
    #[must_use]
    pub const fn active(binding: ActiveCasBinding) -> Self {
        Self::Active(binding)
    }
    pub const fn stale(binding: StaleCasBinding) -> Self {
        Self::Stale(binding)
    }
    #[must_use]
    pub const fn lifecycle(&self) -> BindingLifecycle {
        match self {
            Self::Unbound { .. } => BindingLifecycle::Unbound,
            Self::Valid(_) => BindingLifecycle::Valid,
            Self::Active(_) => BindingLifecycle::Active,
            Self::Stale(_) => BindingLifecycle::Stale,
        }
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> Option<&CasThreadId> {
        match self {
            Self::Valid(value) => Some(value.cas_thread_id()),
            Self::Active(value) => Some(value.usable().cas_thread_id()),
            Self::Stale(value) => Some(value.cas_thread_id()),
            Self::Unbound { .. } => None,
        }
    }

    #[must_use]
    pub const fn execution(&self) -> Option<&ExecutionBinding> {
        match self {
            Self::Valid(value) => Some(value.execution()),
            Self::Active(value) => Some(value.usable().execution()),
            Self::Stale(value) => Some(value.execution()),
            Self::Unbound { .. } => None,
        }
    }
}

/// One immutable revision in a thread's CAS projection-binding history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingRecord {
    thread_id: SyndicThreadId,
    revision: BindingRevision,
    selected_path: SelectedPathProof,
    state: BindingState,
}

impl BindingRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        revision: BindingRevision,
        selected_path: SelectedPathProof,
        state: BindingState,
    ) -> Self {
        Self {
            thread_id,
            revision,
            selected_path,
            state,
        }
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> BindingRevision {
        self.revision
    }
    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }
    #[must_use]
    pub const fn state(&self) -> &BindingState {
        &self.state
    }
}

/// Immutable exact execution facts accepted for one active binding revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionSnapshotRecord {
    kind: ExecutionSnapshotKind,
    id: SyndicExecutionSnapshotId,
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    activation_gate_revision: InputGateRevision,
    active_turn_id: SyndicTurnId,
    cas_thread_id: CasThreadId,
    selected_path: SelectedPathProof,
    represented_base_prefix: CasRepresentedPrefixProof,
    represented_base_native_turn_count: CasNativeTurnCount,
    tool_profile: CasConversationToolProfile,
    lineage: CasLineageProof,
    execution: ExecutionBinding,
    loaded_generation: CasLoadedSessionGeneration,
    started_at: SyndicTimestamp,
}

/// Closed shape of one immutable execution snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionSnapshotKind {
    OrdinaryConversation,
    ProviderOperation(crate::ProviderOperationKind),
}

impl ExecutionSnapshotRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SyndicExecutionSnapshotId,
        thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
        activation_gate_revision: InputGateRevision,
        active_turn_id: SyndicTurnId,
        cas_thread_id: CasThreadId,
        selected_path: SelectedPathProof,
        represented_base_prefix: CasRepresentedPrefixProof,
        represented_base_native_turn_count: CasNativeTurnCount,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
        execution: ExecutionBinding,
        loaded_generation: CasLoadedSessionGeneration,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            kind: ExecutionSnapshotKind::OrdinaryConversation,
            id,
            thread_id,
            binding_revision,
            activation_gate_revision,
            active_turn_id,
            cas_thread_id,
            selected_path,
            represented_base_prefix,
            represented_base_native_turn_count,
            tool_profile,
            lineage,
            execution,
            loaded_generation,
            started_at,
        }
    }
    /// Captures one provider operation without activating the ordinary binding.
    #[allow(clippy::too_many_arguments)]
    pub fn provider_operation(
        id: SyndicExecutionSnapshotId,
        thread_id: SyndicThreadId,
        binding_revision: BindingRevision,
        source_gate_revision: InputGateRevision,
        provider_turn_id: SyndicTurnId,
        cas_thread_id: CasThreadId,
        selected_path: SelectedPathProof,
        represented_base_prefix: CasRepresentedPrefixProof,
        represented_base_native_turn_count: CasNativeTurnCount,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
        execution: ExecutionBinding,
        loaded_generation: CasLoadedSessionGeneration,
        started_at: SyndicTimestamp,
    ) -> Self {
        Self {
            kind: ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            ),
            id,
            thread_id,
            binding_revision,
            activation_gate_revision: source_gate_revision,
            active_turn_id: provider_turn_id,
            cas_thread_id,
            selected_path,
            represented_base_prefix,
            represented_base_native_turn_count,
            tool_profile,
            lineage,
            execution,
            loaded_generation,
            started_at,
        }
    }
    #[must_use]
    pub const fn kind(&self) -> ExecutionSnapshotKind {
        self.kind
    }
    #[must_use]
    pub const fn id(&self) -> SyndicExecutionSnapshotId {
        self.id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn activation_gate_revision(&self) -> InputGateRevision {
        self.activation_gate_revision
    }
    #[must_use]
    pub const fn active_turn_id(&self) -> SyndicTurnId {
        self.active_turn_id
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }
    #[must_use]
    pub const fn represented_base_prefix(&self) -> CasRepresentedPrefixProof {
        self.represented_base_prefix
    }
    /// Returns the exact native turn count at the immutable execution base.
    #[must_use]
    pub const fn represented_base_native_turn_count(&self) -> CasNativeTurnCount {
        self.represented_base_native_turn_count
    }
    /// Returns the exact canonical conversation-tool profile fixed for this execution.
    #[must_use]
    pub const fn tool_profile(&self) -> CasConversationToolProfile {
        self.tool_profile
    }
    #[must_use]
    pub const fn lineage(&self) -> CasLineageProof {
        self.lineage
    }
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }
    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
    #[must_use]
    pub const fn started_at(&self) -> SyndicTimestamp {
        self.started_at
    }
}

/// One-way publication of the exact CAS turn returned for an immutable snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveCasTurnRecord {
    snapshot_id: SyndicExecutionSnapshotId,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    binding_revision: BindingRevision,
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
    published_at: SyndicTimestamp,
}

impl ActiveCasTurnRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        snapshot_id: SyndicExecutionSnapshotId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        binding_revision: BindingRevision,
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        published_at: SyndicTimestamp,
    ) -> Self {
        Self {
            snapshot_id,
            thread_id,
            turn_id,
            binding_revision,
            cas_thread_id,
            cas_turn_id,
            published_at,
        }
    }

    #[must_use]
    pub const fn snapshot_id(&self) -> SyndicExecutionSnapshotId {
        self.snapshot_id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }
    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }
    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }
    #[must_use]
    pub const fn published_at(&self) -> SyndicTimestamp {
        self.published_at
    }
}
