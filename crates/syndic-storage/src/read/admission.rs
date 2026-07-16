use beryl_home_store::HomeStore;

use crate::{
    AcceptedInputAdmission, AdmissionMarkers, DraftRecord, IdleSubmission, InputMarkerOrdinal,
    InputMarkerOwner, SyndicReadError, codec::*, domain::SyndicStorage,
};

use super::{SyndicPointReadLimit, SyndicStoredRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MarkerStatus {
    Absent,
    Exact,
    Collision,
}

pub(super) fn marker_set_is_exact(status: MarkerStatus, markers: &AdmissionMarkers) -> bool {
    status == MarkerStatus::Exact
        || (markers.markers().is_empty() && status == MarkerStatus::Absent)
}

pub(super) fn draft_matches_submission(
    draft: Option<&SyndicStoredRecord<DraftRecord>>,
    submission: &IdleSubmission,
) -> bool {
    draft.is_some_and(|stored| {
        let draft = stored.record();
        draft.id() == submission.draft_id()
            && draft.thread_id() == submission.thread_id()
            && draft.revision() == submission.expected_draft_revision()
            && draft.content() == submission.expected_content()
    })
}

pub(super) fn draft_matches_admission(
    draft: Option<&SyndicStoredRecord<DraftRecord>>,
    admission: &AcceptedInputAdmission,
) -> bool {
    draft.is_some_and(|stored| {
        let draft = stored.record();
        draft.id() == admission.draft_id()
            && draft.thread_id() == admission.thread_id()
            && draft.revision() == admission.expected_draft_revision()
            && draft.content() == admission.expected_content()
    })
}

pub(super) fn marker_status(
    storage: &SyndicStorage,
    store: &HomeStore,
    owner: InputMarkerOwner,
    markers: &AdmissionMarkers,
    limit: SyndicPointReadLimit,
) -> Result<MarkerStatus, SyndicReadError> {
    let mut all_absent = true;
    let mut all_exact = true;
    for (index, expected) in markers.markers().iter().enumerate() {
        let ordinal = InputMarkerOrdinal::new(
            u64::try_from(index)
                .expect("bounded marker count fits u64")
                .checked_add(1)
                .expect("bounded marker ordinal does not overflow"),
        )
        .expect("marker ordinal is nonzero");
        let stored = storage.point::<InputMarkerResolutionsFamily>(
            store,
            InputMarkerKey { owner, ordinal },
            limit,
        )?;
        all_absent &= stored.is_none();
        all_exact &= stored.as_ref().is_some_and(|stored| {
            let record = stored.record();
            record.owner() == owner && record.ordinal() == ordinal && record.marker() == *expected
        });
    }
    let next = InputMarkerOrdinal::new(
        u64::try_from(markers.markers().len())
            .expect("bounded marker count fits u64")
            .checked_add(1)
            .expect("bounded marker ordinal does not overflow"),
    )
    .expect("marker ordinal is nonzero");
    let trailing = storage.point::<InputMarkerResolutionsFamily>(
        store,
        InputMarkerKey {
            owner,
            ordinal: next,
        },
        limit,
    )?;
    all_absent &= trailing.is_none();
    all_exact &= trailing.is_none();
    Ok(if all_absent {
        MarkerStatus::Absent
    } else if all_exact {
        MarkerStatus::Exact
    } else {
        MarkerStatus::Collision
    })
}
