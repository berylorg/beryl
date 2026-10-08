use super::*;
use crate::main_window::{
    MainWindowComposerMarkerMetadataAuthority, MainWindowFreshComposerPreparation,
    MainWindowInitialComposerProgress,
};
use crate::runtime_admission::recovery::FirstConversationFacts;

impl PreparedRecoveryServiceGraph {
    pub(crate) fn advance_first_conversation(
        &mut self,
        facts: &FirstConversationFacts,
        cancellation: &CommandCancellation,
    ) -> Result<bool, String> {
        let (candidate, _) = self
            .services
            .as_mut()
            .expect("prepared services")
            .cas
            .as_mut()
            .expect("prepared CAS")
            .app_preparation_parts()
            .ok_or("first conversation candidate is unavailable")?;
        if facts.home_id() != candidate.home_id()
            || facts.home_generation() == candidate.generation()
        {
            return Err("first conversation candidate does not match captured admission".into());
        }
        let transcript =
            prepare_empty_thread_transcript(candidate, &self.syndic, facts.thread_id())?;
        if self
            .first_transcript
            .as_ref()
            .is_some_and(|old| old != &transcript)
        {
            return Err("first conversation transcript source changed".into());
        }
        self.first_transcript = Some(transcript);
        if self.first_composer.is_none() {
            self.first_composer = Some(Box::new(MainWindowFreshComposerPreparation::new_fresh(
                candidate,
                &self.state,
                self.syndic.clone(),
                facts.window().clone(),
                facts.thread_id(),
                facts.draft_id(),
                MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
            )?));
        }
        Ok(matches!(
            self.first_composer
                .as_mut()
                .unwrap()
                .advance(candidate, cancellation)?,
            MainWindowInitialComposerProgress::Activated
        ))
    }

    pub(crate) fn first_conversation_configurator(
        &self,
    ) -> Result<crate::main_window::MainWindowShellComposerConfigurator, String> {
        let source = self
            .first_configurator
            .as_ref()
            .ok_or("first conversation configurator source is unavailable")?;
        let mut configure = source();
        let clipboard = self
            .private_clipboard
            .as_ref()
            .expect("prepared private clipboard")
            .clone();
        Ok(Box::new(move |selection| {
            configure(selection)
                .map(|config| config.with_private_clipboard_owner(clipboard.clone()))
        }))
    }

    pub(crate) fn first_conversation_preparation(
        &mut self,
    ) -> Result<&mut MainWindowFreshComposerPreparation, String> {
        self.first_composer
            .as_deref_mut()
            .ok_or_else(|| "fresh first conversation editor is unavailable".into())
    }

    pub(crate) fn first_conversation_transcript(
        &self,
    ) -> Result<crate::syndic_transcript::PreparedTranscriptActivation, String> {
        self.first_transcript
            .clone()
            .ok_or_else(|| "first conversation transcript is not qualified".into())
    }

    pub(crate) fn validate_first_conversation(&mut self) -> Result<(), String> {
        if let Some(first) = self.first_composer.as_ref() {
            let (candidate, _) = self
                .services
                .as_mut()
                .expect("prepared services")
                .cas
                .as_mut()
                .expect("prepared CAS")
                .app_preparation_parts()
                .ok_or("first conversation candidate is unavailable")?;
            first.revalidate_ready(candidate)?;
            let transcript = prepare_empty_thread_transcript(
                candidate,
                &self.syndic,
                first.window().selected_thread().unwrap().thread_id(),
            )?;
            if self.first_transcript.as_ref() != Some(&transcript) {
                return Err(
                    "first conversation transcript binding changed before publication".into(),
                );
            }
        }
        Ok(())
    }
}

pub(super) fn prepare_empty_thread_transcript(
    candidate: &mut HomeRecoveryCandidate,
    storage: &SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
) -> Result<crate::syndic_transcript::PreparedTranscriptActivation, String> {
    use crate::syndic_transcript::*;
    let access = candidate.recovery_access().map_err(|e| e.to_string())?;
    let before = access.home_revision().map_err(|e| e.to_string())?;
    let limit = syndic_storage::SyndicPointReadLimit::new(65_536).unwrap();
    let thread = storage
        .thread_candidate(&access, thread_id, limit)
        .map_err(|e| e.to_string())?
        .ok_or("first conversation transcript thread is missing")?;
    let head = storage
        .transcript_view_head_candidate(&access, thread_id, limit)
        .map_err(|e| e.to_string())?;
    if thread.committed_tail().is_some()
        || head
            .as_ref()
            .is_some_and(|head| head.entry_count() != 0 || head.thread_id() != thread_id)
    {
        return Err("unpublished first conversation transcript is no longer empty".into());
    }
    if access.home_revision().map_err(|e| e.to_string())? != before {
        return Err("first conversation transcript changed during candidate read".into());
    }
    let current = head
        .as_ref()
        .is_some_and(|head| head.lifecycle() == syndic_storage::ProjectionLifecycle::Current);
    let revision = head
        .as_ref()
        .map_or(thread.revision().get(), |head| head.revision().get());
    let id = TranscriptViewId(thread_id.to_string());
    Ok(PreparedTranscriptActivation::new(
        id.clone(),
        TranscriptActivationPlacement::Tail,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: id,
            revision: ProviderRevision(revision),
            history_state: if current {
                TranscriptProviderHistoryState::Complete
            } else {
                TranscriptProviderHistoryState::Unavailable {
                    reason: TranscriptProviderHistoryReason::ProjectionStale,
                    detail: Some("The transcript is still being prepared.".into()),
                }
            },
            records: Vec::new(),
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        None,
    ))
}

impl ProcessServiceOwner {
    pub(crate) fn retain_cancelled_first_conversation(
        &mut self,
        graph: &mut PreparedRecoveryServiceGraph,
    ) -> Result<(), String> {
        if let Some(first) = graph.first_composer.take() {
            match (*first).capture_cleanup() {
                Ok(cleanup) => self
                    .first_cleanups
                    .get_mut()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(cleanup),
                Err((first, error)) => {
                    graph.first_composer = Some(Box::new(first));
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}
