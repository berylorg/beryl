use beryl_home_store::HomeStore;
use beryl_model::{DomainRevision, ExecutionBinding, SyndicThreadId, SyndicTurnId};

use crate::draft_piece::{
    DraftEditHistoryFrontiersFamily, DraftPieceRootsFamily,
    draft_edit_history_frontier_is_authenticated_v1,
};
use crate::{
    CreateThread, DraftByThreadRecord, DraftRecord, HistorySummaryRecord, SelectedPathProof,
    SyndicReadError, SyndicTimestamp, ThreadCatalogTitle, ThreadCreationStatus, ThreadRecord,
    catalog_title::{
        HistoryTitlePath, HistoryTitleReadError, StoreTitleSnapshot, derive_history_title,
    },
    codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

/// One index-stabilized current thread/draft pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCurrentDraft {
    thread: ThreadRecord,
    draft: DraftRecord,
    root: crate::DraftPieceRootRecordV1,
}

impl SyndicCurrentDraft {
    #[must_use]
    pub const fn thread(&self) -> &ThreadRecord {
        &self.thread
    }

    #[must_use]
    pub const fn draft(&self) -> &DraftRecord {
        &self.draft
    }

    #[must_use]
    pub const fn root(&self) -> &crate::DraftPieceRootRecordV1 {
        &self.root
    }
}

/// Exact source-thread selected-tail and activity proof captured in one domain revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicThreadTail {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    execution: ExecutionBinding,
    last_activity_at: SyndicTimestamp,
    complete: bool,
    entire_selected_path_title: Option<ThreadCatalogTitle>,
}

impl SyndicThreadTail {
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

    #[must_use]
    pub const fn last_activity_at(&self) -> SyndicTimestamp {
        self.last_activity_at
    }

    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }

    /// Returns the fallback candidate for the entire inherited selected path.
    #[must_use]
    pub const fn entire_selected_path_title(&self) -> Option<&ThreadCatalogTitle> {
        self.entire_selected_path_title.as_ref()
    }
}

impl SyndicStorage {
    /// Reads one thread's exact current draft through an index/draft/index stability proof.
    pub fn current_draft(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicCurrentDraft>, SyndicReadError> {
        let Some(first) = self.point::<DraftByThreadFamily>(store, thread_id, limit)? else {
            return match self.point::<DraftByThreadFamily>(store, thread_id, limit)? {
                None => Ok(None),
                Some(_) => Err(concurrent("current-draft read")),
            };
        };
        let index = first.clone();
        let thread = required(
            self.point::<ThreadsFamily>(store, thread_id, limit)?,
            "current draft owner thread is missing",
        )?;
        let draft = required(
            self.point::<DraftsFamily>(store, index.draft_id(), limit)?,
            "current draft record is missing",
        )?;
        let root = required(
            self.point::<DraftPieceRootsFamily>(store, draft.piece_root().key(), limit)?,
            "current draft piece root is missing",
        )?;
        let history = required(
            self.point::<DraftEditHistoryFrontiersFamily>(store, draft.history().key(), limit)?,
            "current draft edit history is missing",
        )?;
        let second = required(
            self.point::<DraftByThreadFamily>(store, thread_id, limit)?,
            "current draft changed during stabilized read",
        )?;
        if second != index {
            return Err(concurrent("current-draft read"));
        }
        validate_current(&thread, &draft, &root, &index)?;
        if history.reference() != draft.history()
            || !draft_edit_history_frontier_is_authenticated_v1(self, store, &history)?
        {
            return Err(SyndicReadError::Invariant(
                "current draft edit history closure is invalid",
            ));
        }
        Ok(Some(SyndicCurrentDraft {
            thread,
            draft,
            root,
        }))
    }

    /// Captures one exact selected-tail and activity proof under a stable domain revision.
    pub fn thread_tail(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicThreadTail>, SyndicReadError> {
        let before = self.revision(store)?;
        let Some(thread) = self.point::<ThreadsFamily>(store, thread_id, limit)? else {
            return stable_missing(self, store, before, "thread-tail read");
        };
        let summary = required(
            self.point::<HistorySummariesFamily>(store, thread_id, limit)?,
            "thread history summary is missing",
        )?;
        let execution = required(
            self.point::<ThreadExecutionsFamily>(store, thread_id, limit)?,
            "thread execution is missing",
        )?;
        let entire_selected_path_title = derive_history_title(
            &StoreTitleSnapshot {
                storage: *self,
                store,
            },
            &thread,
            HistoryTitlePath::EntireSelectedPath,
        )
        .map_err(map_title_error)?;
        let after = self.revision(store)?;
        if before != after {
            return Err(concurrent("thread-tail read"));
        }
        validate_summary(&thread, &summary)?;
        if execution.thread_id() != thread_id {
            return Err(SyndicReadError::Invariant(
                "thread execution key and record disagree",
            ));
        }
        Ok(Some(SyndicThreadTail {
            thread_id,
            selected_path: SelectedPathProof::new(
                thread.committed_tail(),
                thread.revision(),
                thread.selected_path_digest(),
            ),
            execution: execution.execution().clone(),
            last_activity_at: summary.last_activity_at(),
            complete: summary.complete(),
            entire_selected_path_title,
        }))
    }

    /// Reconciles the exact natural identities and initial records of one creation intent.
    pub fn thread_creation_status(
        &self,
        store: &HomeStore,
        creation: &CreateThread,
        limit: SyndicPointReadLimit,
    ) -> Result<ThreadCreationStatus, SyndicReadError> {
        let before = self.revision(store)?;
        let expected = creation.records();
        let thread = self.point::<ThreadsFamily>(store, creation.thread_id(), limit)?;
        let execution = self.point::<ThreadExecutionsFamily>(store, creation.thread_id(), limit)?;
        let attributes =
            self.point::<ThreadAttributesFamily>(store, creation.thread_id(), limit)?;
        let usage = self.point::<ThreadUsageFamily>(store, creation.thread_id(), limit)?;
        let catalog_summary =
            self.point::<ThreadCatalogSummariesFamily>(store, creation.thread_id(), limit)?;
        let draft = self.point::<DraftsFamily>(store, creation.draft_id(), limit)?;
        let root = self.point::<DraftPieceRootsFamily>(
            store,
            expected.draft_piece_root.reference().key(),
            limit,
        )?;
        let draft_edit_history = self.point::<DraftEditHistoryFrontiersFamily>(
            store,
            expected.draft_edit_history.reference().key(),
            limit,
        )?;
        let index = self.point::<DraftByThreadFamily>(store, creation.thread_id(), limit)?;
        let head = self.point::<TranscriptHeadsFamily>(store, creation.thread_id(), limit)?;
        let transcript_build = match &expected.transcript_build {
            Some(build) => self.point::<TranscriptBuildsFamily>(
                store,
                ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                limit,
            )?,
            None => None,
        };
        let summary = self.point::<HistorySummariesFamily>(store, creation.thread_id(), limit)?;
        let input_gate = self.point::<InputGatesFamily>(store, creation.thread_id(), limit)?;
        let activity_head =
            self.point::<ActivityQueryHeadsFamily>(store, creation.thread_id(), limit)?;
        let binding = self.point::<BindingsFamily>(
            store,
            BindingKey {
                thread: creation.thread_id(),
                revision: expected.binding.revision(),
            },
            limit,
        )?;
        let binding_head = self.point::<BindingHeadsFamily>(store, creation.thread_id(), limit)?;
        let consumed = self.point::<TurnsFamily>(
            store,
            SyndicTurnId::from_bytes(*creation.draft_id().as_bytes()),
            limit,
        )?;
        let accepted = self.point::<AcceptedInputsFamily>(
            store,
            creation.draft_id().accepted_input_id(),
            limit,
        )?;
        let after = self.revision(store)?;
        if before != after {
            return Err(concurrent("thread-creation reconciliation"));
        }
        let absent = thread.is_none()
            && execution.is_none()
            && attributes.is_none()
            && usage.is_none()
            && catalog_summary.is_none()
            && draft.is_none()
            && root.is_none()
            && draft_edit_history.is_none()
            && index.is_none()
            && head.is_none()
            && transcript_build.is_none()
            && summary.is_none()
            && input_gate.is_none()
            && activity_head.is_none()
            && binding.is_none()
            && binding_head.is_none()
            && consumed.is_none()
            && accepted.is_none();
        if absent {
            return Ok(ThreadCreationStatus::Absent);
        }
        let exact = matches_record(thread, &expected.thread)
            && matches_record(execution, &expected.execution)
            && matches_record(attributes, &expected.attributes)
            && matches_record(usage, &expected.usage)
            && matches_record(catalog_summary, &expected.catalog_summary)
            && matches_record(draft, &expected.draft)
            && matches_record(root, &expected.draft_piece_root)
            && matches_record(draft_edit_history, &expected.draft_edit_history)
            && matches_record(index, &expected.draft_index)
            && matches_record(head, &expected.transcript_head)
            && match (&expected.transcript_build, transcript_build) {
                (Some(expected), Some(stored)) => &stored == expected,
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
            && matches_record(summary, &expected.summary)
            && matches_record(input_gate, &expected.input_gate)
            && matches_record(activity_head, &expected.activity_head)
            && matches_record(binding, &expected.binding)
            && matches_record(binding_head, &expected.binding_head)
            && consumed.is_none()
            && accepted.is_none();
        Ok(if exact {
            ThreadCreationStatus::Exact
        } else {
            ThreadCreationStatus::Collision
        })
    }
}

fn validate_current(
    thread: &ThreadRecord,
    draft: &DraftRecord,
    root: &crate::DraftPieceRootRecordV1,
    index: &DraftByThreadRecord,
) -> Result<(), SyndicReadError> {
    if thread.current_draft_id() != draft.id()
        || draft.thread_id() != thread.id()
        || index.thread_id() != thread.id()
        || index.draft_id() != draft.id()
        || index.draft_revision() != draft.revision()
        || index.thread_revision() != thread.revision()
        || root.reference() != draft.piece_root()
    {
        return Err(SyndicReadError::Invariant(
            "thread, current draft, and reverse index disagree",
        ));
    }
    Ok(())
}

fn validate_summary(
    thread: &ThreadRecord,
    summary: &HistorySummaryRecord,
) -> Result<(), SyndicReadError> {
    if summary.thread_id() != thread.id()
        || summary.thread_revision() != thread.revision()
        || summary.committed_tail() != thread.committed_tail()
        || summary.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicReadError::Invariant(
            "thread and history summary disagree",
        ));
    }
    Ok(())
}

fn required<T>(record: Option<T>, message: &'static str) -> Result<T, SyndicReadError> {
    record.ok_or(SyndicReadError::Invariant(message))
}

fn matches_record<T: Eq>(record: Option<T>, expected: &T) -> bool {
    record.as_ref().is_some_and(|record| record == expected)
}

fn stable_missing(
    storage: &SyndicStorage,
    store: &HomeStore,
    before: DomainRevision,
    operation: &'static str,
) -> Result<Option<SyndicThreadTail>, SyndicReadError> {
    if storage.revision(store)? == before {
        Ok(None)
    } else {
        Err(concurrent(operation))
    }
}

fn concurrent(operation: &'static str) -> SyndicReadError {
    SyndicReadError::ConcurrentChange { operation }
}

fn map_title_error(error: HistoryTitleReadError) -> SyndicReadError {
    match error {
        HistoryTitleReadError::Read(source) => SyndicReadError::Read(source),
        HistoryTitleReadError::Invariant(message) => SyndicReadError::Invariant(message),
    }
}
