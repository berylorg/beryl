use super::*;

impl PublishedRunningThreadsReader {
    pub(crate) fn qualify_captured_discussion(
        &self,
        captured: &crate::syndic_transcript::CapturedDiscussionSource,
    ) -> Result<(), String> {
        self.prepare_captured_discussion_source(captured)
            .map(|_| ())
    }

    fn prepare_captured_discussion_source(
        &self,
        captured: &crate::syndic_transcript::CapturedDiscussionSource,
    ) -> Result<syndic_storage::PreparedDiscussionSource, String> {
        if !self.current() {
            return Err("The branch source is unavailable.".into());
        }
        let thread = self
            .syndic
            .thread(
                &self.home,
                captured.source.thread_id(),
                syndic_storage::SyndicPointReadLimit::new(400_000)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
            .ok_or("The branch source thread is unavailable.")?;
        self.syndic
            .prepare_discussion_source(
                &self.home,
                captured.source,
                thread.selected_path(),
                captured.entry,
                captured.text.clone(),
            )
            .map_err(|error| error.to_string())
    }
    pub(crate) fn create_captured_discussion(
        &self,
        captured: &crate::syndic_transcript::CapturedDiscussionSource,
        request: syndic_storage::CreateDiscussion,
        cancellation: beryl_home_store::CommandCancellation,
        #[cfg(all(test, feature = "test-faults"))] before_execute: Option<
            Box<dyn FnOnce(&beryl_home_store::CommandCancellation) + Send>,
        >,
    ) -> Result<crate::discussion_creation::DiscussionCreationOutcome, String> {
        let source = self.prepare_captured_discussion_source(captured)?;
        let service = crate::discussion_creation::DiscussionCreationService::new(
            self.discussion_creations.clone(),
            self.home.as_ref().clone(),
            self.state.clone(),
            self.syndic.clone(),
        );
        let prepared = service
            .prepare(source, request, cancellation.clone())
            .map_err(|error| error.to_string())?;
        if !self.current() {
            return Err("The branch source retired before creation.".into());
        }
        #[cfg(all(test, feature = "test-faults"))]
        if let Some(hook) = before_execute {
            hook(&cancellation);
        }
        Ok(prepared.execute())
    }

    pub(crate) fn reconcile_captured_discussion(
        &self,
        audit: &crate::discussion_creation::DiscussionCreationAudit,
    ) -> Result<crate::discussion_creation::DiscussionCreationAuditOutcome, String> {
        if !self.current() {
            return Err("The original branch home is unavailable; exact creation evidence is retained for same-home recovery.".into());
        }
        audit
            .reconcile(&self.home, &self.syndic, &self.state)
            .map_err(|error| error.to_string())
    }
}
