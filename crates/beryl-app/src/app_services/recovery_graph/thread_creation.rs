use super::*;
use crate::main_window::{
    MainWindowComposerMarkerMetadataAuthority, MainWindowFreshComposerPreparation,
    MainWindowInitialComposerProgress,
};

pub(super) struct PreparedThreadCreation {
    window: beryl_model::WindowId,
    composer: MainWindowFreshComposerPreparation,
    transcript: crate::syndic_transcript::PreparedTranscriptActivation,
}

impl PreparedRecoveryServiceGraph {
    pub(crate) async fn dispose_cancelled_failed_resident_source(
        &mut self,
        source: crate::main_window::MainWindowFailedResidentCandidateSource,
        executor: gpui::BackgroundExecutor,
    ) -> Result<
        crate::main_window::MainWindowFailedComposerRetirement,
        (
            crate::main_window::MainWindowFailedResidentCandidateSource,
            String,
        ),
    > {
        let Some((candidate, _)) = self
            .services
            .as_mut()
            .and_then(|services| services.cas.as_mut())
            .and_then(|cas| cas.app_preparation_parts())
        else {
            return Err((
                source,
                "cancelled failed resident original recovery candidate is unavailable".into(),
            ));
        };
        source.dispose_cancelled_service(candidate, executor).await
    }

    pub(crate) fn has_committed_thread_creation(&self, window: beryl_model::WindowId) -> bool {
        self.failed_thread_creations
            .iter()
            .any(|source| source.window == window && source.qualified && source.committed)
    }

    pub(crate) fn advance_thread_creation(
        &mut self,
        window: beryl_model::WindowId,
        cancellation: &CommandCancellation,
    ) -> Result<bool, String> {
        let source = self
            .failed_thread_creations
            .iter_mut()
            .find(|source| source.window == window && source.qualified && source.committed)
            .ok_or("authenticated original New Thread is unavailable")?;
        let (candidate, _) = self
            .services
            .as_mut()
            .unwrap()
            .cas
            .as_mut()
            .unwrap()
            .app_preparation_parts()
            .ok_or("New Thread recovery candidate is unavailable")?;
        let commit = source
            .operation
            .committed()
            .ok_or("original New Thread receipt is unavailable")?;
        commit
            .validate_candidate(
                &candidate
                    .recovery_access()
                    .map_err(|error| error.to_string())?,
                &self.state,
            )
            .map_err(|error| error.to_string())?;
        let transcript = super::first_conversation::prepare_empty_thread_transcript(
            candidate,
            &self.syndic,
            commit.selection.thread_id(),
        )?;
        if !self
            .thread_creations
            .iter()
            .any(|prepared| prepared.window == window)
        {
            let composer = MainWindowFreshComposerPreparation::new_fresh(
                candidate,
                &self.state,
                self.syndic.clone(),
                commit.window.clone(),
                commit.selection.thread_id(),
                commit.draft,
                MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
            )?;
            self.thread_creations.push(PreparedThreadCreation {
                window,
                composer,
                transcript: transcript.clone(),
            });
        }
        let prepared = self
            .thread_creations
            .iter_mut()
            .find(|prepared| prepared.window == window)
            .unwrap();
        if prepared.transcript != transcript {
            return Err("New Thread recovery transcript changed".into());
        }
        Ok(matches!(
            prepared.composer.advance(candidate, cancellation)?,
            MainWindowInitialComposerProgress::Activated
        ))
    }

    pub(crate) fn thread_creation_parts(
        &mut self,
        window: beryl_model::WindowId,
    ) -> Result<
        (
            &mut MainWindowFreshComposerPreparation,
            &mut crate::main_window::MainWindowFailedThreadCreationRetirement,
            crate::syndic_transcript::PreparedTranscriptActivation,
        ),
        String,
    > {
        let source = self
            .failed_thread_creations
            .iter_mut()
            .find(|source| source.window == window && source.committed)
            .ok_or("original committed New Thread owner is unavailable")?;
        let prepared = self
            .thread_creations
            .iter_mut()
            .find(|prepared| prepared.window == window)
            .ok_or("fresh New Thread editor is unavailable")?;
        Ok((
            &mut prepared.composer,
            source
                .retirement
                .as_mut()
                .ok_or("original New Thread cleanup is unavailable")?,
            prepared.transcript.clone(),
        ))
    }

    pub(crate) fn thread_creation_configurator(
        &self,
    ) -> Result<crate::main_window::MainWindowShellComposerConfigurator, String> {
        self.first_conversation_configurator()
    }

    pub(crate) fn validate_thread_creations(&mut self) -> Result<(), String> {
        let (candidate, _) = self
            .services
            .as_mut()
            .unwrap()
            .cas
            .as_mut()
            .unwrap()
            .app_preparation_parts()
            .ok_or("New Thread validation candidate is unavailable")?;
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        for source in &self.failed_thread_creations {
            source
                .operation
                .exclusion
                .as_ref()
                .ok_or("original selection exclusion is missing")?
                .validate_candidate(&access)?;
            if !source.qualified {
                return Err("original thread operation remains unqualified".into());
            }
            if source.committed {
                source
                    .operation
                    .committed()
                    .ok_or("original New Thread receipt is missing")?
                    .validate_candidate(&access, &self.state)
                    .map_err(|error| error.to_string())?;
                source
                    .retirement
                    .as_ref()
                    .ok_or("original New Thread retirement is missing")?
                    .validate_complete_retirement()?;
            } else {
                source
                    .operation
                    .qualify_prior_candidate(&access, &self.state, source.prior)?;
            }
        }
        drop(access);
        for prepared in &self.thread_creations {
            prepared.composer.revalidate_ready(candidate)?;
            let transcript = super::first_conversation::prepare_empty_thread_transcript(
                candidate,
                &self.syndic,
                prepared
                    .composer
                    .window()
                    .selected_thread()
                    .unwrap()
                    .thread_id(),
            )?;
            if prepared.transcript != transcript {
                return Err("New Thread transcript changed before publication".into());
            }
        }
        if self
            .failed_thread_creations
            .iter()
            .filter(|source| source.committed)
            .count()
            != self.thread_creations.len()
        {
            return Err("fresh New Thread target set is incomplete".into());
        }
        Ok(())
    }

    pub(super) fn release_thread_creation_publications(&mut self) -> Result<(), String> {
        while let Some(prepared) = self.thread_creations.pop() {
            if let Err(composer) = prepared.composer.release_publication() {
                self.thread_creations.push(PreparedThreadCreation {
                    composer,
                    ..prepared
                });
                return Err("fresh New Thread publication custody is unsettled".into());
            }
        }
        Ok(())
    }
}

impl ProcessServiceOwner {
    pub(crate) fn retain_cancelled_thread_creation_editors(
        &mut self,
        graph: &mut PreparedRecoveryServiceGraph,
    ) -> Result<(), String> {
        while let Some(prepared) = graph.thread_creations.pop() {
            match prepared.composer.capture_cleanup() {
                Ok(cleanup) => self
                    .first_cleanups
                    .get_mut()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(cleanup),
                Err((composer, error)) => {
                    graph.thread_creations.push(PreparedThreadCreation {
                        composer,
                        ..prepared
                    });
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}
