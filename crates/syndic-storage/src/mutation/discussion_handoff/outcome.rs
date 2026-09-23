use super::*;
use crate::read::access::ReadAccess;
use beryl_home_store::HomeCandidateRecoveryAccess;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionHandoffStatus {
    ExactOld,
    ExactNew,
    Collision,
}

impl SyndicStorage {
    pub fn discussion_handoff_status(
        &self,
        store: &HomeStore,
        intent: &DiscussionHandoffIntent,
    ) -> Result<DiscussionHandoffStatus, SyndicReadError> {
        self.discussion_handoff_status_with_access(ReadAccess::Ordinary(store), intent)
    }

    pub fn discussion_handoff_status_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        intent: &DiscussionHandoffIntent,
    ) -> Result<DiscussionHandoffStatus, SyndicReadError> {
        self.discussion_handoff_status_with_access(ReadAccess::Candidate(store), intent)
    }

    fn discussion_handoff_status_with_access(
        &self,
        access: ReadAccess<'_>,
        intent: &DiscussionHandoffIntent,
    ) -> Result<DiscussionHandoffStatus, SyndicReadError> {
        let before = self.revision_with_access(access)?;
        if access.home_id() != intent.home_id {
            return Err(SyndicReadError::Invariant(
                "discussion handoff intent belongs to another home",
            ));
        }
        let thread = intent.old_gate.thread_id();
        let gate = access.read_point::<SyndicDomain, DiscussionHandoffGatesCodec>(
            &self.handle,
            &thread,
            family_point_limit::<DiscussionHandoffGatesFamily>(),
        )?;
        let mut old = gate == Some(intent.old_gate);
        let mut new = gate == Some(intent.new_gate);
        if let Some((old_attributes, new_attributes)) = &intent.attributes {
            let attributes = access.read_point::<SyndicDomain, ThreadAttributesCodec>(
                &self.handle,
                &thread,
                family_point_limit::<ThreadAttributesFamily>(),
            )?;
            old &= attributes.as_ref() == Some(old_attributes);
            new &= attributes.as_ref() == Some(new_attributes);
        }
        if self.revision_with_access(access)? != before {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion handoff outcome",
            });
        }
        Ok(if old {
            DiscussionHandoffStatus::ExactOld
        } else if new {
            DiscussionHandoffStatus::ExactNew
        } else {
            DiscussionHandoffStatus::Collision
        })
    }
}
