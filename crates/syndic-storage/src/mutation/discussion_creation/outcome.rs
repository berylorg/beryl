use super::*;
use crate::{draft_piece::*, read::access::ReadAccess};
use beryl_home_store::HomeCandidateRecoveryAccess;

impl SyndicStorage {
    pub fn discussion_creation_status(
        &self,
        store: &HomeStore,
        intent: &DiscussionCreationIntent,
    ) -> Result<ThreadCreationStatus, SyndicReadError> {
        self.discussion_creation_status_with_access(ReadAccess::Ordinary(store), intent)
    }

    pub fn discussion_creation_status_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        intent: &DiscussionCreationIntent,
    ) -> Result<ThreadCreationStatus, SyndicReadError> {
        self.discussion_creation_status_with_access(ReadAccess::Candidate(store), intent)
    }

    fn discussion_creation_status_with_access(
        &self,
        access: ReadAccess<'_>,
        intent: &DiscussionCreationIntent,
    ) -> Result<ThreadCreationStatus, SyndicReadError> {
        let before = self.revision_with_access(access)?;
        let records = &intent.0;
        if access.home_id() != records.home_id {
            return Err(SyndicReadError::Invariant(
                "discussion creation intent belongs to another home",
            ));
        }
        let initial = &records.initial;
        let thread = initial.thread.id();
        let mut absent = true;
        let mut exact = true;
        macro_rules! inspect {
            ($family:ty, $key:expr, $expected:expr) => {
                match access.read_point::<SyndicDomain, ExactCodec<$family>>(
                    &self.handle,
                    &$key,
                    family_point_limit::<$family>(),
                )? {
                    None => exact = false,
                    Some(actual) => {
                        absent = false;
                        exact &= actual == $expected;
                    }
                }
            };
        }
        macro_rules! require_absent {
            ($family:ty, $key:expr) => {
                if access
                    .read_point::<SyndicDomain, ExactCodec<$family>>(
                        &self.handle,
                        &$key,
                        family_point_limit::<$family>(),
                    )?
                    .is_some()
                {
                    absent = false;
                    exact = false;
                }
            };
        }
        inspect!(ThreadsFamily, thread, initial.thread);
        inspect!(
            ImageLabelAuthorityHeadsFamily,
            thread,
            initial.image_label_authority_head
        );
        inspect!(
            DraftImageLabelProtectionHeadsFamily,
            thread,
            initial.draft_image_label_protection_head
        );
        inspect!(ThreadExecutionsFamily, thread, initial.execution);
        inspect!(ThreadAttributesFamily, thread, initial.attributes);
        inspect!(ThreadUsageFamily, thread, initial.usage);
        inspect!(
            ThreadCatalogSummariesFamily,
            thread,
            initial.catalog_summary
        );
        inspect!(DraftsFamily, initial.draft.id(), initial.draft);
        inspect!(
            DraftPieceRootsFamily,
            initial.draft_piece_root.reference().key(),
            initial.draft_piece_root
        );
        inspect!(
            DraftEditHistoryFrontiersFamily,
            initial.draft_edit_history.reference().key(),
            initial.draft_edit_history
        );
        inspect!(DraftByThreadFamily, thread, initial.draft_index);
        inspect!(TranscriptHeadsFamily, thread, initial.transcript_head);
        inspect!(HistorySummariesFamily, thread, initial.summary);
        inspect!(InputGatesFamily, thread, initial.input_gate);
        inspect!(ActivityQueryHeadsFamily, thread, initial.activity_head);
        inspect!(
            BindingsFamily,
            BindingKey {
                thread,
                revision: initial.binding.revision()
            },
            initial.binding
        );
        inspect!(BindingHeadsFamily, thread, initial.binding_head);
        inspect!(
            ContextEnvelopesFamily,
            ContextOwnerKey::from(records.context.owner()),
            records.context
        );
        inspect!(
            ThreadParentFamily,
            ThreadPairKey {
                first: records.parent.parent_thread_id(),
                second: thread
            },
            records.parent
        );
        inspect!(DiscussionHandoffGatesFamily, thread, records.gate);
        require_absent!(
            TranscriptBuildsFamily,
            ThreadTranscriptBuildKey {
                thread,
                generation: TranscriptGeneration::FIRST
            }
        );
        require_absent!(NonIdleGateSourcesFamily, thread);
        require_absent!(
            TurnsFamily,
            beryl_model::SyndicTurnId::from_bytes(*initial.draft.id().as_bytes())
        );
        require_absent!(AcceptedInputsFamily, initial.draft.id().accepted_input_id());
        if self.revision_with_access(access)? != before {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion creation outcome",
            });
        }
        Ok(if absent {
            ThreadCreationStatus::Absent
        } else if exact {
            ThreadCreationStatus::Exact
        } else {
            ThreadCreationStatus::Collision
        })
    }
}
