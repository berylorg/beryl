use super::*;
mod discovery;
use discovery::discover;

impl SyndicStorage {
    pub fn discover_generated_discussion_input(
        &self,
        store: &HomeStore,
        intent: &GeneratedDiscussionInputLookup,
    ) -> Result<GeneratedDiscussionInputDiscovery, SyndicReadError> {
        self.discover_generated_discussion_input_with_access(ReadAccess::Ordinary(store), intent)
    }
    pub fn discover_generated_discussion_input_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        intent: &GeneratedDiscussionInputLookup,
    ) -> Result<GeneratedDiscussionInputDiscovery, SyndicReadError> {
        self.discover_generated_discussion_input_with_access(ReadAccess::Candidate(access), intent)
    }
    fn discover_generated_discussion_input_with_access(
        &self,
        access: ReadAccess<'_>,
        intent: &GeneratedDiscussionInputLookup,
    ) -> Result<GeneratedDiscussionInputDiscovery, SyndicReadError> {
        let revision = self.revision_with_access(access)?;
        if access.home_id() != intent.home_id {
            return Err(SyndicReadError::Invariant(
                "generated input witness belongs to another home",
            ));
        }
        let result = discover(
            &ParentRead {
                storage: self,
                access,
            },
            intent,
        )?;
        if self.revision_with_access(access)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "generated input discovery",
            });
        }
        Ok(result)
    }
    pub fn generated_discussion_input_status(
        &self,
        store: &HomeStore,
        intent: &GeneratedDiscussionInputIntent,
    ) -> Result<GeneratedDiscussionInputStatus, SyndicReadError> {
        self.generated_discussion_input_status_with_access(ReadAccess::Ordinary(store), intent)
    }
    pub fn generated_discussion_input_status_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        intent: &GeneratedDiscussionInputIntent,
    ) -> Result<GeneratedDiscussionInputStatus, SyndicReadError> {
        self.generated_discussion_input_status_with_access(ReadAccess::Candidate(access), intent)
    }
    fn generated_discussion_input_status_with_access(
        &self,
        access: ReadAccess<'_>,
        intent: &GeneratedDiscussionInputIntent,
    ) -> Result<GeneratedDiscussionInputStatus, SyndicReadError> {
        let revision = self.revision_with_access(access)?;
        if access.home_id() != intent.home_id {
            return Err(SyndicReadError::Invariant(
                "generated input witness belongs to another home",
            ));
        }
        let reader = ParentRead {
            storage: self,
            access,
        };
        let (mut old, mut new) = (true, true);
        for change in intent.changes.iter() {
            let sides = change.matches(&reader)?;
            old &= sides.0;
            new &= sides.1;
        }
        let content = super::content::closure_matches(&reader, &intent.changes)?;
        old &= content.0;
        new &= content.1;
        if self.revision_with_access(access)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "generated input outcome",
            });
        }
        Ok(match (old, new) {
            (true, false) => GeneratedDiscussionInputStatus::ExactOld,
            (false, true) => GeneratedDiscussionInputStatus::ExactNew,
            _ => GeneratedDiscussionInputStatus::Collision,
        })
    }
}
