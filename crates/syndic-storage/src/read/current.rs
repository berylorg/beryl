use beryl_home_store::HomeStore;
use beryl_model::{DomainRevision, SyndicThreadId, SyndicTurnId};

use crate::{
    AcceptedInputAdmission, CanonicalItemKind, ContentLifecycle, ContentManifestRecord,
    CreateThread, DraftByThreadRecord, DraftRecord, HistorySummaryRecord, IdleSubmission,
    InputAdmissionStatus, InputMarkerOwner, SelectedPathProof, SyndicReadError, SyndicTimestamp,
    ThreadCreationStatus, ThreadRecord, TurnItemOrdinal, TurnKind, codec::*, domain::SyndicStorage,
};

use super::{SyndicPointReadLimit, SyndicStoredRecord, admission::*};

/// One index-stabilized current thread/draft pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicCurrentDraft {
    thread: ThreadRecord,
    draft: DraftRecord,
    content: ContentManifestRecord,
    stored_bytes: usize,
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
    pub const fn content(&self) -> &ContentManifestRecord {
        &self.content
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

/// Exact source-thread selected-tail and activity proof captured in one domain revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicThreadTail {
    thread_id: SyndicThreadId,
    selected_path: SelectedPathProof,
    last_activity_at: SyndicTimestamp,
    complete: bool,
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
    pub const fn last_activity_at(&self) -> SyndicTimestamp {
        self.last_activity_at
    }

    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
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
        let index = first.record().clone();
        let thread = required(
            self.point::<ThreadsFamily>(store, thread_id, limit)?,
            "current draft owner thread is missing",
        )?;
        let draft = required(
            self.point::<DraftsFamily>(store, index.draft_id(), limit)?,
            "current draft record is missing",
        )?;
        let content = required(
            self.point::<ContentManifestsFamily>(store, draft.record().content().id(), limit)?,
            "current draft content manifest is missing",
        )?;
        let second = required(
            self.point::<DraftByThreadFamily>(store, thread_id, limit)?,
            "current draft changed during stabilized read",
        )?;
        if second.record() != &index {
            return Err(concurrent("current-draft read"));
        }
        validate_current(&thread.record, &draft.record, &content.record, &index)?;
        let stored_bytes = sum_bytes(&[
            first.stored_bytes(),
            thread.stored_bytes(),
            draft.stored_bytes(),
            content.stored_bytes(),
            second.stored_bytes(),
        ])?;
        Ok(Some(SyndicCurrentDraft {
            thread: thread.record,
            draft: draft.record,
            content: content.record,
            stored_bytes,
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
        let after = self.revision(store)?;
        if before != after {
            return Err(concurrent("thread-tail read"));
        }
        validate_summary(thread.record(), summary.record())?;
        Ok(Some(SyndicThreadTail {
            thread_id,
            selected_path: SelectedPathProof::new(
                thread.record().committed_tail(),
                thread.record().revision(),
                thread.record().selected_path_digest(),
            ),
            last_activity_at: summary.record().last_activity_at(),
            complete: summary.record().complete(),
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
        let draft = self.point::<DraftsFamily>(store, creation.draft_id(), limit)?;
        let content =
            self.point::<ContentManifestsFamily>(store, expected.content_manifest.id(), limit)?;
        let mut content_chunks = Vec::with_capacity(expected.content_chunks.len());
        for chunk in &expected.content_chunks {
            content_chunks.push(self.point::<ContentChunksFamily>(
                store,
                ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                limit,
            )?);
        }
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
            && draft.is_none()
            && index.is_none()
            && head.is_none()
            && transcript_build.is_none()
            && summary.is_none()
            && input_gate.is_none()
            && binding.is_none()
            && binding_head.is_none()
            && consumed.is_none()
            && accepted.is_none();
        if absent {
            return Ok(ThreadCreationStatus::Absent);
        }
        let exact = matches_record(thread, &expected.thread)
            && matches_record(draft, &expected.draft)
            && matches_record(content, &expected.content_manifest)
            && content_chunks
                .into_iter()
                .zip(&expected.content_chunks)
                .all(|(stored, expected)| matches_record(stored, expected))
            && matches_record(index, &expected.draft_index)
            && matches_record(head, &expected.transcript_head)
            && match (&expected.transcript_build, transcript_build) {
                (Some(expected), Some(stored)) => stored.record() == expected,
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
            && matches_record(summary, &expected.summary)
            && matches_record(input_gate, &expected.input_gate)
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

    /// Reconciles one idle submission through its draft-derived immutable identities.
    pub fn idle_submission_status(
        &self,
        store: &HomeStore,
        submission: &IdleSubmission,
        limit: SyndicPointReadLimit,
    ) -> Result<InputAdmissionStatus, SyndicReadError> {
        let before = self.revision(store)?;
        let draft = self.point::<DraftsFamily>(store, submission.draft_id(), limit)?;
        let turn = self.point::<TurnsFamily>(store, submission.submitted_turn_id(), limit)?;
        let accepted = self.point::<AcceptedInputsFamily>(
            store,
            submission.draft_id().accepted_input_id(),
            limit,
        )?;
        let item = self.point::<CanonicalItemsFamily>(store, submission.user_item_id(), limit)?;
        let current = self.current_draft(store, submission.thread_id(), limit)?;
        let gate = self.input_gate(store, submission.thread_id(), limit)?;
        let marker_status = marker_status(
            self,
            store,
            InputMarkerOwner::CanonicalItem(submission.user_item_id()),
            submission.markers(),
            limit,
        )?;
        if before != self.revision(store)? {
            return Err(concurrent("idle-submission reconciliation"));
        }
        if draft_matches_submission(draft.as_ref(), submission)
            && turn.is_none()
            && accepted.is_none()
            && item.is_none()
            && marker_status == MarkerStatus::Absent
        {
            return Ok(InputAdmissionStatus::Absent);
        }
        let exact_turn = turn.as_ref().is_some_and(|stored| {
            let turn = stored.record();
            turn.id() == submission.submitted_turn_id()
                && turn.origin_thread_id() == submission.thread_id()
                && turn.kind() == TurnKind::OrdinaryUser
                && turn.submitted_at() == submission.admitted_at()
        });
        let exact_item = item.as_ref().is_some_and(|stored| {
            let item = stored.record();
            item.id() == submission.user_item_id()
                && item.turn_id() == submission.submitted_turn_id()
                && item.ordinal() == TurnItemOrdinal::FIRST
                && item.kind() == CanonicalItemKind::UserInput
                && item.source_event().is_none()
                && item.cas_source().is_none()
                && item.payload().content() == Some(submission.expected_content())
                && item.payload().marker_count()
                    == u64::try_from(submission.markers().markers().len()).unwrap_or(u64::MAX)
        });
        let exact_current = current.as_ref().is_some_and(|current| {
            replacement_draft_matches(
                current,
                submission.expected_thread_revision(),
                submission.next_draft_id(),
                submission.admitted_at(),
            ) && current.thread().committed_tail() == Some(submission.submitted_turn_id())
                && current.draft().parent()
                    == crate::ConversationParent::Turn(submission.submitted_turn_id())
        });
        let exact_gate = gate.as_ref().is_some_and(|stored| {
            submission.expected_gate_revision().checked_next().ok()
                == Some(stored.record().revision())
                && stored.record().state()
                    == &crate::InputGateState::PendingTurn(submission.submitted_turn_id())
        });
        Ok(
            if draft.is_none()
                && accepted.is_none()
                && exact_turn
                && exact_item
                && exact_current
                && exact_gate
                && marker_set_is_exact(marker_status, submission.markers())
            {
                InputAdmissionStatus::ExactSubmitted
            } else {
                InputAdmissionStatus::Collision
            },
        )
    }

    /// Reconciles one non-idle admission through its draft-derived immutable identity.
    pub fn accepted_input_status(
        &self,
        store: &HomeStore,
        admission: &AcceptedInputAdmission,
        limit: SyndicPointReadLimit,
    ) -> Result<InputAdmissionStatus, SyndicReadError> {
        let before = self.revision(store)?;
        let draft = self.point::<DraftsFamily>(store, admission.draft_id(), limit)?;
        let turn =
            self.point::<TurnsFamily>(store, admission.draft_id().submitted_turn_id(), limit)?;
        let input =
            self.point::<AcceptedInputsFamily>(store, admission.accepted_input_id(), limit)?;
        let current = self.current_draft(store, admission.thread_id(), limit)?;
        let gate = self.input_gate(store, admission.thread_id(), limit)?;
        let marker_status = marker_status(
            self,
            store,
            InputMarkerOwner::AcceptedInput(admission.accepted_input_id()),
            admission.markers(),
            limit,
        )?;
        if before != self.revision(store)? {
            return Err(concurrent("accepted-input reconciliation"));
        }
        if draft_matches_admission(draft.as_ref(), admission)
            && turn.is_none()
            && input.is_none()
            && marker_status == MarkerStatus::Absent
        {
            return Ok(InputAdmissionStatus::Absent);
        }
        let exact_input = input.as_ref().is_some_and(|stored| {
            let input = stored.record();
            input.id() == admission.accepted_input_id()
                && input.thread_id() == admission.thread_id()
                && input.gate_revision() == admission.expected_gate_revision()
                && input.content() == admission.expected_content()
                && input.marker_count()
                    == u64::try_from(admission.markers().markers().len()).unwrap_or(u64::MAX)
                && input.admitted_at() == admission.admitted_at()
        });
        let exact_current = current.as_ref().is_some_and(|current| {
            replacement_draft_matches(
                current,
                admission.expected_thread_revision(),
                admission.next_draft_id(),
                admission.admitted_at(),
            )
        });
        let exact_gate = gate.as_ref().is_some_and(|stored| {
            admission.expected_gate_revision().checked_next().ok()
                == Some(stored.record().revision())
                && input.as_ref().is_some_and(|input| {
                    stored.record().state().admitted_disposition().as_ref()
                        == Some(input.record().disposition())
                        && stored.record().accepted_high_water() == input.record().ordinal().get()
                })
        });
        Ok(
            if draft.is_none()
                && turn.is_none()
                && exact_input
                && exact_current
                && exact_gate
                && marker_set_is_exact(marker_status, admission.markers())
            {
                InputAdmissionStatus::ExactAccepted
            } else {
                InputAdmissionStatus::Collision
            },
        )
    }
}

fn replacement_draft_matches(
    current: &SyndicCurrentDraft,
    expected_thread_revision: beryl_model::ThreadRevision,
    next_draft_id: beryl_model::SyndicDraftId,
    admitted_at: SyndicTimestamp,
) -> bool {
    expected_thread_revision.checked_next().ok() == Some(current.thread().revision())
        && current.draft().id() == next_draft_id
        && current.draft().revision().get() == 1
        && current.draft().replacement_edit_intent().is_none()
        && current.draft().created_at() == admitted_at
        && current.draft().updated_at() == admitted_at
        && current.draft().content().summary().atom_count() == 0
}

fn validate_current(
    thread: &ThreadRecord,
    draft: &DraftRecord,
    content: &ContentManifestRecord,
    index: &DraftByThreadRecord,
) -> Result<(), SyndicReadError> {
    if thread.current_draft_id() != draft.id()
        || draft.thread_id() != thread.id()
        || index.thread_id() != thread.id()
        || index.draft_id() != draft.id()
        || index.draft_revision() != draft.revision()
        || index.thread_revision() != thread.revision()
        || content.lifecycle() != ContentLifecycle::Sealed
        || content.sealed_reference() != Some(draft.content())
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

fn required<T>(
    record: Option<SyndicStoredRecord<T>>,
    message: &'static str,
) -> Result<SyndicStoredRecord<T>, SyndicReadError> {
    record.ok_or(SyndicReadError::Invariant(message))
}

fn matches_record<T: Eq>(record: Option<SyndicStoredRecord<T>>, expected: &T) -> bool {
    record.is_some_and(|record| record.record() == expected)
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

fn sum_bytes(values: &[usize]) -> Result<usize, SyndicReadError> {
    values.iter().try_fold(0usize, |total, value| {
        total.checked_add(*value).ok_or(SyndicReadError::Invariant(
            "current-draft stored-byte accounting overflowed",
        ))
    })
}
