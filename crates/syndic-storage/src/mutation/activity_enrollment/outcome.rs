use super::*;

impl SyndicStorage {
    pub fn activity_enrollment_status(
        &self,
        store: &HomeStore,
        witness: &ActivityEnrollmentWitness,
    ) -> Result<ActivityEnrollmentStatus, SyndicReadError> {
        self.activity_enrollment_status_with_access(ReadAccess::Ordinary(store), witness)
    }
    pub fn activity_enrollment_status_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        witness: &ActivityEnrollmentWitness,
    ) -> Result<ActivityEnrollmentStatus, SyndicReadError> {
        self.activity_enrollment_status_with_access(ReadAccess::Candidate(store), witness)
    }
    fn activity_enrollment_status_with_access(
        &self,
        access: ReadAccess<'_>,
        witness: &ActivityEnrollmentWitness,
    ) -> Result<ActivityEnrollmentStatus, SyndicReadError> {
        let before = self.revision_with_access(access)?;
        let r = &witness.0;
        if access.home_id() != r.home {
            return Err(invalid());
        }
        let head = self.point_with_access::<ActivityQueryHeadsFamily>(
            access,
            r.old_head.thread_id(),
            limit(),
        )?;
        let mut old = head.as_ref() == Some(&r.old_head);
        let mut new = head.as_ref() == Some(&r.new_head);
        if let Some(source) = &r.source {
            let actual = self.point_with_access::<ActivityQuerySourcesFamily>(
                access,
                source_key(source),
                limit(),
            )?;
            old &= actual.is_none();
            new &= actual.as_ref() == Some(source);
        }
        for (key, value) in &r.deleted {
            let actual =
                self.point_with_access::<ActivityQueryEntriesFamily>(access, *key, limit())?;
            old &= actual.as_ref() == Some(value);
            new &= actual.is_none();
        }
        if self.revision_with_access(access)? != before {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "activity enrollment outcome",
            });
        }
        Ok(if new {
            ActivityEnrollmentStatus::Committed {
                token: r.source.as_ref().map(|_| r.token.clone()),
            }
        } else if old {
            ActivityEnrollmentStatus::NotCommitted
        } else {
            ActivityEnrollmentStatus::Conflict
        })
    }
}
