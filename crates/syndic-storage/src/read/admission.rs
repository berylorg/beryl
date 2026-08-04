use crate::{AcceptedInputAdmission, DraftRecord, IdleSubmission};

mod accepted;

pub(super) fn draft_matches_submission(
    draft: Option<&DraftRecord>,
    submission: &IdleSubmission,
) -> bool {
    draft.is_some_and(|stored| {
        stored.id() == submission.draft_id()
            && stored.thread_id() == submission.thread_id()
            && stored.revision() == submission.expected_draft_revision()
            && stored.content() == submission.expected_content()
    })
}

pub(super) fn draft_matches_admission(
    draft: Option<&DraftRecord>,
    admission: &AcceptedInputAdmission,
) -> bool {
    draft.is_some_and(|stored| {
        stored.id() == admission.draft_id()
            && stored.thread_id() == admission.thread_id()
            && stored.revision() == admission.expected_draft_revision()
            && stored.content() == admission.expected_content()
    })
}
