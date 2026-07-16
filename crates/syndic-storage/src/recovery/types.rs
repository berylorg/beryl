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
    /// Assemble the current pending selected turn's complete parent path after restart.
    PendingSelectedTurnParent,
}

/// Exact request to prepare one current thread's recovery prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryProjectionRequest {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    model_context_window_tokens: Option<u64>,
    scope: RecoveryProjectionScope,
}

impl RecoveryProjectionRequest {
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
        }
    }

    /// Creates a restart request for the complete parent of a pending selected turn.
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

/// Closed role of one lossless recovery item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryItemRole {
    User,
    Assistant,
}

/// Closed Responses API text shape of one lossless recovery item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryItemTextKind {
    InputText,
    OutputText,
}

/// One immutable ordered recovery item with no independently pairable role and text shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryItem {
    UserInputText(Box<str>),
    AssistantOutputText(Box<str>),
}

impl RecoveryItem {
    pub(super) fn user(text: Box<str>) -> Self {
        Self::UserInputText(text)
    }

    pub(super) fn assistant(text: Box<str>) -> Self {
        Self::AssistantOutputText(text)
    }

    #[must_use]
    pub const fn role(&self) -> RecoveryItemRole {
        match self {
            Self::UserInputText(_) => RecoveryItemRole::User,
            Self::AssistantOutputText(_) => RecoveryItemRole::Assistant,
        }
    }

    #[must_use]
    pub const fn text_kind(&self) -> RecoveryItemTextKind {
        match self {
            Self::UserInputText(_) => RecoveryItemTextKind::InputText,
            Self::AssistantOutputText(_) => RecoveryItemTextKind::OutputText,
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::UserInputText(text) | Self::AssistantOutputText(text) => text,
        }
    }
}

/// Complete bounded recovery prefix assembled under one stable Syndic domain revision.
///
/// Its source revision is read provenance only, not expected-revision authority for a later proof
/// publication. That publication must use the current domain revision together with its exact
/// current thread, selected-path, and binding expected revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryProjection {
    pub(super) version: RecoveryProjectionVersion,
    pub(super) thread_id: SyndicThreadId,
    pub(super) selected_path: SelectedPathProof,
    pub(super) represented_prefix: CasRepresentedPrefixProof,
    pub(super) items: Box<[RecoveryItem]>,
    pub(super) item_count: RecoveryItemCount,
    pub(super) utf8_bytes: RecoveryUtf8ByteCount,
    pub(super) sequence_digest: RecoveryItemSequenceDigest,
    pub(super) source_revision: DomainRevision,
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
    pub fn items(&self) -> &[RecoveryItem] {
        &self.items
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
