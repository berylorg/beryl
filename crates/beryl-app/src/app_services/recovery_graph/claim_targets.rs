use super::*;
use crate::main_window::{
    MainWindowComposerMarkerMetadataAuthority, MainWindowFreshComposerPreparation,
    MainWindowInitialComposerProgress,
};
mod adopted_return;
mod requests;

pub(super) struct PreparedClaimTarget {
    window: beryl_model::WindowId,
    composer: MainWindowFreshComposerPreparation,
    transcript: crate::syndic_transcript::PreparedTranscriptActivation,
    widget_reply: Option<Box<crate::main_window::MainWindowFreshClaimWidgetReply>>,
}

impl PreparedRecoveryServiceGraph {
    pub(crate) async fn dispose_cancelled_failed_resident_source(
        &mut self,
        source: Box<crate::main_window::MainWindowFailedResidentCandidateSource>,
        executor: gpui::BackgroundExecutor,
    ) -> Result<
        Box<crate::main_window::MainWindowFailedComposerRetirement>,
        (
            Box<crate::main_window::MainWindowFailedResidentCandidateSource>,
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

    pub(crate) fn has_committed_claim(&self, window: beryl_model::WindowId) -> bool {
        self.failed_claims
            .iter()
            .any(|source| source.window == window && source.qualified && source.committed)
    }

    pub(crate) fn advance_claim_target(
        &mut self,
        window: beryl_model::WindowId,
        cancellation: &CommandCancellation,
    ) -> Result<bool, String> {
        let source = self
            .failed_claims
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
        let transcript = match commit {
            RetiredClaimCommit::Creation(commit) => {
                super::first_conversation::prepare_empty_thread_transcript(
                    candidate,
                    &self.syndic,
                    commit.selection.thread_id(),
                )?
            }
            RetiredClaimCommit::Ordinary(commit) => {
                crate::transcript_provider::prepare_candidate_activation(
                    &candidate
                        .recovery_access()
                        .map_err(|error| error.to_string())?,
                    &self.syndic,
                    window,
                    commit.selection.thread_id(),
                    cancellation,
                )?
            }
        };
        if !self
            .claim_targets
            .iter()
            .any(|prepared| prepared.window == window)
        {
            let draft = match commit {
                RetiredClaimCommit::Creation(commit) => commit.draft,
                RetiredClaimCommit::Ordinary(commit) => self
                    .syndic
                    .current_draft_candidate(
                        &candidate
                            .recovery_access()
                            .map_err(|error| error.to_string())?,
                        commit.selection.thread_id(),
                        syndic_storage::SyndicPointReadLimit::new(65_536)
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or("ordinary target current draft is missing")?
                    .draft()
                    .id(),
            };
            let prepare = match commit {
                RetiredClaimCommit::Creation(_) => MainWindowFreshComposerPreparation::new_fresh,
                RetiredClaimCommit::Ordinary(_) => {
                    MainWindowFreshComposerPreparation::new_ordinary_claim
                }
            };
            let mut composer = prepare(
                candidate,
                &self.state,
                self.syndic.clone(),
                commit.window().clone(),
                commit.selection().thread_id(),
                draft,
                MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
            )?;
            let title =
                candidate_selected_title(candidate, &self.state, window, commit.selection())?;
            composer.set_qualified_selected_title(commit.selection(), title)?;
            self.claim_targets.push(PreparedClaimTarget {
                window,
                composer,
                transcript: transcript.clone(),
                widget_reply: None,
            });
        }
        let prepared = self
            .claim_targets
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

    pub(crate) fn claim_target_parts(
        &mut self,
        window: beryl_model::WindowId,
    ) -> Result<
        (
            &mut MainWindowFreshComposerPreparation,
            &mut crate::main_window::MainWindowFailedClaimRetirement,
            crate::syndic_transcript::PreparedTranscriptActivation,
        ),
        String,
    > {
        let source = self
            .failed_claims
            .iter_mut()
            .find(|source| source.window == window && source.committed)
            .ok_or("original committed New Thread owner is unavailable")?;
        let prepared = self
            .claim_targets
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

    pub(crate) fn claim_target_configurator(
        &self,
    ) -> Result<crate::main_window::MainWindowShellComposerConfigurator, String> {
        self.first_conversation_configurator()
    }

    pub(crate) fn validate_claim_targets(&mut self) -> Result<(), String> {
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
        for source in &self.failed_claims {
            source.operation.validate_exclusion(&access)?;
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
        for prepared in &self.claim_targets {
            let title = candidate_selected_title(
                candidate,
                &self.state,
                prepared.window,
                prepared.composer.window().selected_thread().unwrap(),
            )?;
            if prepared.composer.qualified_selected_title() != Some(&title) {
                return Err("fresh candidate title changed before publication".into());
            }
            if prepared.widget_reply.is_some() {
                return Err("fresh claim widget batch still retains candidate custody".into());
            }
            prepared.composer.revalidate_ready(candidate)?;
            let original = self
                .failed_claims
                .iter()
                .find(|source| source.window == prepared.window)
                .ok_or("original claim target is missing")?;
            let thread = prepared
                .composer
                .window()
                .selected_thread()
                .unwrap()
                .thread_id();
            let transcript = match original.operation.as_ref() {
                RetiredClaimOperation::Creation(_) => {
                    super::first_conversation::prepare_empty_thread_transcript(
                        candidate,
                        &self.syndic,
                        thread,
                    )?
                }
                RetiredClaimOperation::Ordinary(_) => {
                    crate::transcript_provider::prepare_candidate_activation(
                        &candidate
                            .recovery_access()
                            .map_err(|error| error.to_string())?,
                        &self.syndic,
                        prepared.window,
                        thread,
                        &CommandCancellation::new(),
                    )?
                }
            };
            if prepared.transcript != transcript {
                return Err("New Thread transcript changed before publication".into());
            }
        }
        if self
            .failed_claims
            .iter()
            .filter(|source| source.committed)
            .count()
            != self.claim_targets.len()
        {
            return Err("fresh New Thread target set is incomplete".into());
        }
        Ok(())
    }

    pub(super) fn release_claim_publications(&mut self) -> Result<(), String> {
        while let Some(prepared) = self.claim_targets.pop() {
            if let Err(composer) = prepared.composer.release_publication() {
                self.claim_targets.push(PreparedClaimTarget {
                    composer,
                    ..prepared
                });
                return Err("fresh New Thread publication custody is unsettled".into());
            }
        }
        Ok(())
    }
}

fn candidate_selected_title(
    candidate: &mut beryl_home_store::HomeRecoveryCandidate,
    state: &beryl_state::BerylState,
    window: beryl_model::WindowId,
    selection: beryl_state::WindowClaimSelection,
) -> Result<beryl_state::CatalogResolvedTitle, String> {
    let access = candidate
        .recovery_access()
        .map_err(|error| error.to_string())?;
    let revision = access.home_revision().map_err(|error| error.to_string())?;
    let row = state
        .catalog()
        .current_row_source_candidate(
            &access,
            selection.thread_id(),
            beryl_state::CatalogPointReadLimit::schema_maximum(),
        )
        .map_err(|error| error.to_string())?
        .ok_or("fresh candidate title row is missing")?;
    if row.row().facts().claim()
        != beryl_state::CatalogClaimSummary::claimed(window, beryl_state::CatalogClaimKind::Active)
        || row.row().sources().claim() != Some(selection.revision())
    {
        return Err("fresh candidate title claim differs from the qualified selection".into());
    }
    if access.home_revision().map_err(|error| error.to_string())? != revision {
        return Err("fresh candidate title source changed".into());
    }
    Ok(row.row().title().clone())
}

impl ProcessServiceOwner {
    pub(crate) fn retain_cancelled_claim_editors(
        &mut self,
        graph: &mut PreparedRecoveryServiceGraph,
    ) -> Result<(), String> {
        while let Some(prepared) = graph.claim_targets.pop() {
            match prepared.composer.capture_cleanup() {
                Ok(cleanup) => self
                    .first_cleanups
                    .get_mut()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(cleanup),
                Err((composer, error)) => {
                    graph.claim_targets.push(PreparedClaimTarget {
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
