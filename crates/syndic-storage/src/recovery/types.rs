use beryl_model::{DomainRevision, RecoveryItemSequenceDigest, SyndicThreadId};

use crate::{
    CasRepresentedPrefixProof, RecoveryItemCount, RecoveryProjectionVersion, RecoveryUtf8ByteCount,
    SelectedPathProof,
};

/// Which exact selected-path prefix recovery assembly must include.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryProjectionScope {
    /// Assemble the complete current selected path before input admission.
    CurrentSelectedPath,
    /// Assemble the eligible parent context of the current pending selected turn after restart.
    ///
    /// The immediate parent may be exact authority-lost tail context even though it remains
    /// durably incomplete. Earlier ancestors must still be recovery-complete.
    PendingSelectedTurnParent,
}

/// Exact request to prepare one current thread's recovery prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryProjectionRequest {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    model_context_window_tokens: Option<u64>,
    scope: RecoveryProjectionScope,
    pub(crate) context_tool_profile: Option<beryl_model::CasConversationToolProfile>,
}

impl RecoveryProjectionRequest {
    pub const fn with_tool_profile(
        mut self,
        profile: beryl_model::CasConversationToolProfile,
    ) -> Self {
        self.context_tool_profile = Some(profile);
        self
    }
    /// Creates a pre-admission request for the complete current selected path.
    ///
    /// `None` deliberately represents missing metadata, while `Some(0)` deliberately retains an
    /// invalid zero value so preparation can reject the two cases independently. An empty current
    /// path needs no recovery items and therefore needs no model metadata.
    #[must_use]
    pub const fn for_current_selected_path(
        thread_id: SyndicThreadId,
        selected_path: SelectedPathProof,
        model_context_window_tokens: Option<u64>,
    ) -> Self {
        Self {
            thread_id,
            selected_path,
            model_context_window_tokens,
            scope: RecoveryProjectionScope::CurrentSelectedPath,
            context_tool_profile: None,
        }
    }

    /// Creates a restart request for the eligible parent context of a pending selected turn.
    ///
    /// `None` deliberately represents missing metadata, while `Some(0)` deliberately retains an
    /// invalid zero value so preparation can reject the two cases independently. A root pending
    /// turn has an empty parent path and therefore needs no model metadata.
    #[must_use]
    pub const fn for_pending_selected_turn_parent(
        thread_id: SyndicThreadId,
        selected_path: SelectedPathProof,
        model_context_window_tokens: Option<u64>,
    ) -> Self {
        Self {
            thread_id,
            selected_path,
            model_context_window_tokens,
            scope: RecoveryProjectionScope::PendingSelectedTurnParent,
            context_tool_profile: None,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn model_context_window_tokens(self) -> Option<u64> {
        self.model_context_window_tokens
    }

    #[must_use]
    pub const fn scope(self) -> RecoveryProjectionScope {
        self.scope
    }
}

/// Compact proof of one replayable recovery-context prefix under a stable Syndic domain revision.
///
/// Its source revision is read provenance only, not expected-revision authority for a later proof
/// publication. That publication must use the current domain revision together with its exact
/// current thread, selected-path, and binding expected revisions.
///
/// A pending-parent proof may include one exact authority-lost immediate tail without making that
/// turn recovery-complete. The selected-path and represented-prefix relationship preserves which
/// eligibility contract produced the proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryProjection {
    pub(crate) context: Option<crate::ContextProjectionBasis>,
    pub(crate) context_classification: Option<crate::context_projection::ContextClassification>,
    pub(crate) seed_only: bool,
    pub(crate) version: RecoveryProjectionVersion,
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) selected_path: SelectedPathProof,
    pub(crate) represented_prefix: CasRepresentedPrefixProof,
    pub(crate) item_count: RecoveryItemCount,
    pub(crate) utf8_bytes: RecoveryUtf8ByteCount,
    pub(crate) sequence_digest: RecoveryItemSequenceDigest,
    pub(crate) source_revision: DomainRevision,
}

/// Stable storage decision between native Fresh lineage and a nonempty recovery sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryAssembly {
    NativeEmptyPrefix {
        thread_id: SyndicThreadId,
        selected_path: SelectedPathProof,
        source_revision: DomainRevision,
    },
    Ready(RecoveryProjection),
}

impl RecoveryAssembly {
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        match self {
            Self::NativeEmptyPrefix { thread_id, .. } => *thread_id,
            Self::Ready(projection) => projection.thread_id(),
        }
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        match self {
            Self::NativeEmptyPrefix { selected_path, .. } => *selected_path,
            Self::Ready(projection) => projection.selected_path(),
        }
    }

    /// Stable domain revision under which storage assembled this result.
    ///
    /// This is assembly provenance only, not later mutation authority.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        match self {
            Self::NativeEmptyPrefix {
                source_revision, ..
            } => *source_revision,
            Self::Ready(projection) => projection.source_revision(),
        }
    }
}

impl RecoveryProjection {
    pub const fn context(self) -> Option<crate::ContextProjectionBasis> {
        self.context
    }
    pub fn completed_lineage(
        self,
        completed_at: crate::SyndicTimestamp,
        generation: beryl_model::CasLoadedSessionGeneration,
    ) -> Result<crate::CasLineageProof, crate::SyndicValueError> {
        if let Some(context) = self.context {
            context.complete_replay(
                generation,
                crate::ContextInjectionObservation::new(
                    self.item_count,
                    self.utf8_bytes,
                    self.sequence_digest,
                    completed_at,
                )?,
            )
        } else {
            crate::RecoveredInjectionProof::new(
                self.version,
                self.represented_prefix,
                self.sequence_digest,
                self.item_count,
                self.utf8_bytes,
                completed_at,
                generation,
            )
            .map(crate::CasLineageProof::recovered)
        }
    }
    pub fn transport_source_identity(
        self,
        home_generation: u64,
        service_generation: u64,
    ) -> Option<[u8; 32]> {
        use sha2::{Digest, Sha256};
        let context = self.context?;
        let mut hash = Sha256::new();
        hash.update(b"beryl-context-projection-source-v1\0");
        hash.update([u8::from(!self.seed_only)]);
        hash.update(context.home.as_bytes());
        hash.update(home_generation.to_be_bytes());
        hash.update(service_generation.to_be_bytes());
        hash.update(self.thread_id.as_bytes());
        let mut encoded = crate::codec::parts::Encoder::new();
        crate::codec::parts::enc_selected_path(&mut encoded, self.selected_path);
        crate::codec::parts::enc_represented_prefix(&mut encoded, self.represented_prefix);
        hash.update(encoded.finish());
        hash.update(self.source_revision.get().to_be_bytes());
        hash.update(context.projection_digest);
        hash.update(u64::from(self.item_count.get()).to_be_bytes());
        hash.update(self.utf8_bytes.get().to_be_bytes());
        hash.update(self.sequence_digest.as_bytes());
        Some(hash.finalize().into())
    }
    #[must_use]
    pub const fn version(&self) -> RecoveryProjectionVersion {
        self.version
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
    pub const fn represented_prefix(&self) -> CasRepresentedPrefixProof {
        self.represented_prefix
    }

    #[must_use]
    pub const fn item_count(&self) -> RecoveryItemCount {
        self.item_count
    }

    #[must_use]
    pub const fn utf8_bytes(&self) -> RecoveryUtf8ByteCount {
        self.utf8_bytes
    }

    #[must_use]
    pub const fn sequence_digest(&self) -> RecoveryItemSequenceDigest {
        self.sequence_digest
    }

    /// Stable domain revision under which storage assembled this result.
    ///
    /// This is assembly provenance only. A later proof publication must use the then-current
    /// domain revision plus exact current thread, selected-path, and binding expected revisions;
    /// it must not reuse this older global revision as mutation authority.
    #[must_use]
    pub const fn source_revision(&self) -> DomainRevision {
        self.source_revision
    }
}
