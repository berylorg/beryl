//! Bounded native CAS-lineage planning for one exact pending selected turn.

use beryl_home_store::{HomeStore, ReadError};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasNativeTurnCount, CasTurnId, ExecutionBinding,
    SyndicThreadId,
};
use thiserror::Error;

use crate::{
    CasRepresentedPrefixProof, InputGateState, SelectedPathProof, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage, TurnKind, TurnLifecycle, UsableCasBinding,
    empty_selected_path_digest,
};

mod classify;

/// Exact request to classify native CAS lineage for one pending selected turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeProjectionRequest {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    execution: ExecutionBinding,
    tool_profile: CasConversationToolProfile,
}

impl NativeProjectionRequest {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        selected_path: SelectedPathProof,
        execution: ExecutionBinding,
        tool_profile: CasConversationToolProfile,
    ) -> Self {
        Self {
            thread_id,
            selected_path,
            execution,
            tool_profile,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }

    /// Returns the exact conversation-tool profile required for native reuse.
    #[must_use]
    pub const fn tool_profile(&self) -> CasConversationToolProfile {
        self.tool_profile
    }
}

/// Stable facts shared by every native-lineage decision for one request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeProjectionBasis {
    thread_id: SyndicThreadId,
    expected_binding_revision: BindingRevision,
    selected_path: SelectedPathProof,
    represented_prefix: CasRepresentedPrefixProof,
    tool_profile: CasConversationToolProfile,
}

impl NativeProjectionBasis {
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn represented_prefix(self) -> CasRepresentedPrefixProof {
        self.represented_prefix
    }

    /// Returns the exact conversation-tool profile required by this decision.
    #[must_use]
    pub const fn tool_profile(self) -> CasConversationToolProfile {
        self.tool_profile
    }
}

/// Exact durable source binding selected for a native backend operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeProjectionSource {
    thread_id: SyndicThreadId,
    binding_revision: BindingRevision,
    selected_path: SelectedPathProof,
    binding: UsableCasBinding,
}

impl NativeProjectionSource {
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn binding_revision(&self) -> BindingRevision {
        self.binding_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn binding(&self) -> &UsableCasBinding {
        &self.binding
    }
}

/// Why no exact bounded native lineage was available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeProjectionUnavailable {
    MissingCasTurnCorrelation,
    SourceProjectionUnavailable,
    SourceExecutionMismatch,
    SourceToolProfileMismatch,
    SourcePrefixMismatch,
}

/// One exact native projection plan, or explicit authority to consider recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeProjectionPlan {
    Current {
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
    },
    Fresh {
        basis: NativeProjectionBasis,
    },
    Resume {
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
    },
    Fork {
        basis: NativeProjectionBasis,
        source: NativeProjectionSource,
        through_turn: Option<CasTurnId>,
        native_turn_count: CasNativeTurnCount,
    },
    Unavailable {
        basis: NativeProjectionBasis,
        /// Exact current target-thread reservation that must be retired before recovery.
        source: Option<NativeProjectionSource>,
        reason: NativeProjectionUnavailable,
    },
}

impl NativeProjectionPlan {
    #[must_use]
    pub const fn basis(&self) -> NativeProjectionBasis {
        match self {
            Self::Current { basis, .. }
            | Self::Fresh { basis }
            | Self::Resume { basis, .. }
            | Self::Fork { basis, .. }
            | Self::Unavailable { basis, .. } => *basis,
        }
    }
}

/// Failure to establish one stable native-lineage decision.
#[derive(Debug, Error)]
pub enum NativeProjectionError {
    #[error("the supplied selected path is not the thread's exact current selected path")]
    StaleSelectedPath,
    #[error("the selected tail is not one pending ordinary-user turn")]
    CurrentTailNotPendingOrdinaryUser,
    #[error("the pending turn requires the separately proven discussion-context projection")]
    DiscussionContextProjectionRequired,
    #[error("Syndic changed concurrently during native projection preparation")]
    ConcurrentChange,
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error("native projection invariant failed: {0}")]
    Invariant(&'static str),
}

impl From<SyndicReadError> for NativeProjectionError {
    fn from(source: SyndicReadError) -> Self {
        match source {
            SyndicReadError::Read(source) => Self::Read(source),
            SyndicReadError::ConcurrentChange { .. } => Self::ConcurrentChange,
            SyndicReadError::Invariant(message) => Self::Invariant(message),
            SyndicReadError::ContentTextRequiresSealed
            | SyndicReadError::ContentTextContainsImageMarkers { .. }
            | SyndicReadError::InvalidContentTextOffset { .. }
            | SyndicReadError::InvalidContentTextReadLimit { .. }
            | SyndicReadError::ContentTextReadLimitTooSmall { .. }
            | SyndicReadError::InvalidResourceRange { .. }
            | SyndicReadError::InvalidResourceReadLimit { .. }
            | SyndicReadError::ResourceHasNoTextBacking
            | SyndicReadError::CaptureItemHasNoTextContent => Self::Invariant(
                "native planning unexpectedly used a public content/resource range boundary",
            ),
        }
    }
}

impl SyndicStorage {
    /// Plans exact native lineage under one stable Syndic domain revision.
    ///
    /// The read is bounded by point lookups and deterministic skip-ancestor proof. It never scans
    /// binding history or asks CAS to enumerate or materialize historical turns.
    pub fn prepare_native_projection(
        &self,
        store: &HomeStore,
        request: &NativeProjectionRequest,
        limit: SyndicPointReadLimit,
    ) -> Result<NativeProjectionPlan, NativeProjectionError> {
        let before = self.revision(store)?;
        let thread = self.thread(store, request.thread_id, limit)?.ok_or(
            NativeProjectionError::Invariant("native projection thread is missing"),
        )?;
        let thread = thread.record();
        let selected_path = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        if selected_path != request.selected_path {
            return Err(NativeProjectionError::StaleSelectedPath);
        }
        if thread.context_owner_id().is_some() {
            return Err(NativeProjectionError::DiscussionContextProjectionRequired);
        }

        let current = self
            .current_binding(store, request.thread_id, limit)?
            .ok_or(NativeProjectionError::Invariant(
                "native projection binding is missing",
            ))?;
        if current.binding().selected_path() != selected_path {
            return Err(NativeProjectionError::Invariant(
                "current binding and thread selected path disagree",
            ));
        }
        let gate = self.input_gate(store, request.thread_id, limit)?.ok_or(
            NativeProjectionError::Invariant("native projection input gate is missing"),
        )?;
        let pending_id = selected_path
            .tail()
            .ok_or(NativeProjectionError::CurrentTailNotPendingOrdinaryUser)?;
        if gate.record().state() != &InputGateState::PendingTurn(pending_id) {
            return Err(NativeProjectionError::CurrentTailNotPendingOrdinaryUser);
        }
        let pending =
            self.turn(store, pending_id, limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "pending selected turn is missing",
                ))?;
        let pending_state =
            self.turn_state(store, pending_id, limit)?
                .ok_or(NativeProjectionError::Invariant(
                    "pending selected turn state is missing",
                ))?;
        if pending.record().kind() != TurnKind::OrdinaryUser
            || pending_state.record().lifecycle() != TurnLifecycle::Pending
        {
            return Err(NativeProjectionError::CurrentTailNotPendingOrdinaryUser);
        }

        let represented_prefix =
            match pending.record().parent().turn() {
                Some(parent_id) => {
                    let parent = self.turn(store, parent_id, limit)?.ok_or(
                        NativeProjectionError::Invariant("pending turn parent is missing"),
                    )?;
                    CasRepresentedPrefixProof::new(
                        Some(parent_id),
                        selected_path.thread_revision(),
                        parent.record().chain_digest(),
                    )
                }
                None => CasRepresentedPrefixProof::new(
                    None,
                    selected_path.thread_revision(),
                    empty_selected_path_digest(),
                ),
            };
        let basis = NativeProjectionBasis {
            thread_id: request.thread_id,
            expected_binding_revision: current.binding().revision(),
            selected_path,
            represented_prefix,
            tool_profile: request.tool_profile,
        };

        let plan = self.classify_native_projection(
            store,
            request,
            current.binding(),
            thread.parent_thread_id(),
            basis,
            limit,
        )?;
        if self.revision(store)? != before {
            return Err(NativeProjectionError::ConcurrentChange);
        }
        Ok(plan)
    }
}
