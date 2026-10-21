use super::*;
use crate::cas_projection::initial_start::InitialStartOwner;

impl ProcessServiceOwner {
    pub(crate) fn publish_recovery_service_graph(
        &mut self,
        expected: HomeGeneration,
        generation: HomeGeneration,
        prepared: &mut Option<PreparedRecoveryServiceGraph>,
        cancellation: &CommandCancellation,
    ) -> Result<InitialStartOwner, String> {
        let graph = prepared
            .as_mut()
            .ok_or("prepared recovery graph is unavailable")?;
        self.validate_retired_service_home_return(expected, Some(self.home_id))
            .map_err(|error| error.to_string())?;
        self.require_catalog_recovery_settlement(expected)
            .map_err(|error| error.to_string())?;
        if self.graph.is_some()
            || self.failed_close.is_some()
            || self.failed_retirement.is_some()
            || self.failed_cas_close.is_some()
            || self.closing_graph.is_some()
            || !matches!(self.attempt, InitialServiceAttemptState::Blocked)
            || !graph.process.same_process(&self.process)
            || generation == expected
            || !graph.matches_candidate(self.home_id, generation)
        {
            return Err(
                "recovery publication requires the exact prepared graph and process".into(),
            );
        }
        self.require_settled_custody()
            .map_err(|error| error.to_string())?;
        check_cancellation(cancellation).map_err(|error| error.to_string())?;
        graph.validate_claim_targets()?;
        let services = graph.services.as_mut().expect("complete recovery services");
        assert!(
            services.marker.is_some() && services.activity.is_some() && services.theme.is_some()
        );
        assert!(graph.attention.is_some());
        let (home, cas, start, handoff, identity) = services
            .cas
            .as_mut()
            .expect("complete recovery CAS")
            .publish()
            .map_err(|error| error.to_string())?;
        services
            .catalog_query
            .as_ref()
            .expect("prepared recovery queries")
            .publish();
        self.graph = Some(PublishedAppServices {
            media: services.media.take(),
            prepared_settings: services.settings.take(),
            settings: None,
            settings_themes: None,
            settings_theme_retirement: None,
            model_selection: std::sync::OnceLock::new(),
            runtime_setup: graph
                .runtime_setup
                .take()
                .expect("prepared runtime admission"),
            private_clipboard: graph
                .private_clipboard
                .take()
                .expect("prepared clipboard owner"),
            restore_lifetime: Some(Arc::new(())),
            process: self.process.clone(),
            shutdown: None,
            shutdown_ready: false,
            handoff: Some(handoff),
            activity: Some(
                services
                    .activity
                    .take()
                    .unwrap()
                    .into_service(&home, identity),
            ),
            marker: Some(services.marker.take().unwrap().into_service()),
            theme: services.theme.take(),
            loaded_theme: None,
            cas: Some(cas),
            catalog_source: services.catalog_source.take(),
            catalog_query: services.catalog_query.take(),
            sessions: graph.sessions.clone(),
            attention: graph.attention.take().unwrap(),
            state: graph.state.clone(),
            syndic: graph.syndic.clone(),
            home: Some(home),
        });
        if let Some(first) = graph.first_composer.take() {
            assert!(
                (*first).release_publication().is_ok(),
                "qualified first editor transfers with whole graph publication"
            );
        }
        assert!(
            graph.release_claim_publications().is_ok(),
            "qualified New Thread editor transfers with whole graph publication"
        );
        self.failed_claims.append(&mut graph.failed_claims);
        let retirement = self.recovery_retirement.take().unwrap();
        self.attempt = InitialServiceAttemptState::Published(Some(retirement.fence));
        drop(prepared.take());
        Ok(start)
    }

    #[cfg(test)]
    pub(crate) fn reopen_recovery_admission(
        &mut self,
        generation: HomeGeneration,
    ) -> Result<(), String> {
        if self.reopen_recovery_admission_observed(generation, None, || ())? {
            Ok(())
        } else {
            Err("Recovery publication observation is busy or stale".into())
        }
    }

    pub(crate) fn reopen_recovery_admission_observed(
        &mut self,
        generation: HomeGeneration,
        snapshot: Option<&crate::discussion_presentation::DiscussionPresentationSnapshot>,
        apply: impl FnOnce(),
    ) -> Result<bool, String> {
        let InitialServiceAttemptState::Published(Some(fence)) = &self.attempt else {
            return Err("Published recovery admission fence is unavailable".into());
        };
        let graph = self
            .graph
            .as_ref()
            .ok_or("Published recovery graph is unavailable")?;
        if graph.home().health().generation() != Some(generation) {
            return Err("Published recovery generation changed".into());
        }
        let exclusions = self
            .failed_claims
            .iter()
            .map(|source| source.operation.exclusion())
            .collect::<Result<Vec<_>, String>>()?;
        for (source, exclusion) in self.failed_claims.iter().zip(&exclusions) {
            exclusion.validate_owner()?;
            if source.committed {
                source
                    .retirement
                    .as_ref()
                    .ok_or("original New Thread cleanup is missing")?
                    .validate_complete_retirement()?;
            }
        }
        let observation = snapshot.map(|snapshot| snapshot.home_observation());
        let publish = || {
            graph.cas().try_reopen_shutdown_admission_observed_with(
                fence,
                observation.as_ref(),
                || {
                    exclusions
                        .iter()
                        .map(|exclusion| exclusion.try_prepare_coherent_release())
                        .collect::<Result<Vec<_>, _>>()
                },
                |releases| {
                    for release in releases {
                        release.apply();
                    }
                    apply();
                },
            )
        };
        let result = if let Some(snapshot) = snapshot {
            match snapshot.try_elect_local(publish) {
                Ok(result) => result,
                Err(
                    crate::discussion_presentation::DiscussionPresentationError::Changed
                    | crate::discussion_presentation::DiscussionPresentationError::Retired
                    | crate::discussion_presentation::DiscussionPresentationError::Settlement(
                        crate::discussion_settlement::DiscussionSettlementError::ConcurrentChange,
                    ),
                ) => return Ok(false),
                Err(error) => return Err(error.to_string()),
            }
        } else {
            publish()
        };
        match result {
            Ok(Ok(())) => {}
            Ok(Err(
                crate::window_acquisition::RetiredSelectionReleaseError::Busy
                | crate::window_acquisition::RetiredSelectionReleaseError::Changed,
            )) => return Ok(false),
            Ok(Err(error)) => return Err(error.to_string()),
            Err(error) => {
                if error.is_waiting() {
                    return Ok(false);
                }
                return Err(error.to_string());
            }
        }
        drop(exclusions);
        self.attempt = InitialServiceAttemptState::Published(None);
        self.failed_claims.clear();
        Ok(true)
    }
}
