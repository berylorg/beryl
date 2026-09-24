use beryl_home_store::{DomainReader, HomeCandidateRecoveryAccess, HomeStore};
use beryl_model::BerylHomeId;

use super::required;
use crate::{codec::*, domain::SyndicDomain, read::access::ReadAccess, *};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivitySourceQualification {
    Current {
        token: ActivityPeriodToken,
        source: ActivityQuerySource,
    },
    Retired(ActivityRetirementFingerprint),
    Unenrolled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityRetirementFingerprint {
    home: BerylHomeId,
    period: ActivityWorkPeriod,
    source: ActivityQuerySource,
}

impl ActivityRetirementFingerprint {
    pub fn from_enrollment(token: &ActivityPeriodToken, source: ActivityQuerySource) -> Self {
        Self {
            home: token.home_id(),
            period: token.work_period(),
            source,
        }
    }

    pub(crate) fn matches(&self, home: BerylHomeId, head: &ActivityQueryHeadRecord) -> bool {
        self.home == home && head.work_period() == self.period && head.source() == Some(self.source)
    }

    pub(crate) fn authenticates(&self, home: BerylHomeId, source: ActivityQuerySource) -> bool {
        self.home == home && self.source == source
    }
}

impl ActivitySourceQualification {
    pub(crate) fn current_head(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        home: BerylHomeId,
        source: ActivityQuerySource,
    ) -> Result<ActivityQueryHeadRecord, SyndicMutationError> {
        let Self::Current {
            token,
            source: qualified,
        } = self
        else {
            return Err(SyndicMutationError::ActivityQueryConflict);
        };
        let execution = required::<ThreadExecutionsFamily>(reader, &source.thread_id())?;
        let head = required::<ActivityQueryHeadsFamily>(reader, &source.thread_id())?;
        if token.home_id() != home
            || *qualified != source
            || execution.thread_id() != source.thread_id()
            || execution.execution().runtime_id() != token.runtime_id()
            || head.thread_id() != source.thread_id()
            || head.source() != Some(source)
            || head.work_period() != token.work_period()
            || !head.source_active()
            || head.lifecycle() != ProjectionLifecycle::Current
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let member = required::<ActivityQuerySourcesFamily>(
            reader,
            &ActivityQuerySourceKey {
                thread: source.thread_id(),
                work_period: token.work_period(),
                source_thread: source.thread_id(),
                source_turn: source.turn_id(),
            },
        )?;
        if member.thread_id() != source.thread_id()
            || member.source() != source
            || member.work_period() != token.work_period()
            || !member.active()
            || member.child_handoff().is_some()
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        Ok(head)
    }
}

impl SyndicStorage {
    pub fn activity_retirement_fingerprint(
        &self,
        home: &HomeStore,
        source: ActivityQuerySource,
    ) -> Result<Option<ActivityRetirementFingerprint>, SyndicReadError> {
        self.activity_retirement_fingerprint_with_access(ReadAccess::Ordinary(home), source)
    }

    pub fn activity_retirement_fingerprint_candidate(
        &self,
        home: &HomeCandidateRecoveryAccess<'_>,
        source: ActivityQuerySource,
    ) -> Result<Option<ActivityRetirementFingerprint>, SyndicReadError> {
        self.activity_retirement_fingerprint_with_access(ReadAccess::Candidate(home), source)
    }

    fn activity_retirement_fingerprint_with_access(
        &self,
        access: ReadAccess<'_>,
        source: ActivityQuerySource,
    ) -> Result<Option<ActivityRetirementFingerprint>, SyndicReadError> {
        let head = self
            .point_with_access::<ActivityQueryHeadsFamily>(
                access,
                source.thread_id(),
                SyndicPointReadLimit::new(65_536).expect("fixed head bound"),
            )?
            .ok_or(SyndicReadError::Invariant(
                "Activity retirement owner head is missing",
            ))?;
        if head.thread_id() != source.thread_id() {
            return Err(SyndicReadError::Invariant(
                "Activity retirement owner head identity disagrees",
            ));
        }
        Ok(
            (head.source() == Some(source)).then(|| ActivityRetirementFingerprint {
                home: access.home_id(),
                period: head.work_period(),
                source,
            }),
        )
    }
}
