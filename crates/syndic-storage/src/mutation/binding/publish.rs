use super::*;

/// Exact immutable inputs for publishing one usable CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishValidBinding {
    pub(super) thread_id: SyndicThreadId,
    pub(super) expected_binding_revision: BindingRevision,
    pub(super) selected_path: SelectedPathProof,
    pub(super) execution: ExecutionBinding,
    pub(super) cas_thread_id: CasThreadId,
    pub(super) represented_prefix: CasRepresentedPrefixProof,
    pub(super) native_turn_count: CasNativeTurnCount,
    pub(super) tool_profile: CasConversationToolProfile,
    pub(super) lineage: CasLineageProof,
    pub(super) native_basis: Option<crate::NativeProjectionBasis>,
}

impl PublishValidBinding {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        selected_path: SelectedPathProof,
        execution: ExecutionBinding,
        cas_thread_id: CasThreadId,
        represented_prefix: CasRepresentedPrefixProof,
        native_turn_count: CasNativeTurnCount,
        tool_profile: CasConversationToolProfile,
        lineage: CasLineageProof,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            selected_path,
            execution,
            cas_thread_id,
            represented_prefix,
            native_turn_count,
            tool_profile,
            lineage,
            native_basis: None,
        }
    }

    #[must_use]
    pub fn from_native(
        basis: crate::NativeProjectionBasis,
        execution: ExecutionBinding,
        cas_thread_id: CasThreadId,
        native_turn_count: CasNativeTurnCount,
        lineage: CasLineageProof,
    ) -> Self {
        let mut request = Self::new(
            basis.thread_id(),
            basis.expected_binding_revision(),
            basis.selected_path(),
            execution,
            cas_thread_id,
            basis.represented_prefix(),
            native_turn_count,
            basis.tool_profile(),
            lineage,
        );
        request.native_basis = Some(basis);
        request
    }

    #[must_use]
    pub fn from_native_recovery(
        basis: crate::NativeProjectionRecoveryBasis,
        cas_thread_id: CasThreadId,
        native_turn_count: CasNativeTurnCount,
        lineage: CasLineageProof,
    ) -> Self {
        Self::from_native(
            basis.native_basis(),
            basis.execution().clone(),
            cas_thread_id,
            native_turn_count,
            lineage,
        )
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
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

    /// Returns the coordinator-proven exact native CAS turn count for the prefix.
    #[must_use]
    pub const fn native_turn_count(&self) -> CasNativeTurnCount {
        self.native_turn_count
    }

    /// Returns the exact canonical conversation-tool profile established on the CAS thread.
    #[must_use]
    pub const fn tool_profile(&self) -> CasConversationToolProfile {
        self.tool_profile
    }

    #[must_use]
    pub const fn lineage(&self) -> CasLineageProof {
        self.lineage
    }
}

/// Exact immutable inputs for retaining one stale or abandoned CAS projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishStaleBinding {
    pub(super) thread_id: SyndicThreadId,
    pub(super) expected_binding_revision: BindingRevision,
    pub(super) selected_path: SelectedPathProof,
    pub(super) stale: StaleCasBinding,
}

impl PublishStaleBinding {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_binding_revision: BindingRevision,
        selected_path: SelectedPathProof,
        stale: StaleCasBinding,
    ) -> Self {
        Self {
            thread_id,
            expected_binding_revision,
            selected_path,
            stale,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn expected_binding_revision(&self) -> BindingRevision {
        self.expected_binding_revision
    }

    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn stale(&self) -> &StaleCasBinding {
        &self.stale
    }
}
