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
        let services = graph.services.as_mut().expect("complete recovery services");
        assert!(
            services.marker.is_some() && services.activity.is_some() && services.theme.is_some()
        );
        assert!(graph.attention.is_some());
        let (home, cas, start, handoff) = services
            .cas
            .as_mut()
            .expect("complete recovery CAS")
            .publish()
            .map_err(|error| error.to_string())?;
        self.graph = Some(PublishedAppServices {
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
            activity: Some(services.activity.take().unwrap().into_service()),
            marker: Some(services.marker.take().unwrap().into_service()),
            theme: services.theme.take(),
            loaded_theme: None,
            cas: Some(cas),
            sessions: graph.sessions.clone(),
            attention: graph.attention.take().unwrap(),
            state: graph.state.clone(),
            syndic: graph.syndic.clone(),
            home: Some(home),
        });
        if let Some(first) = graph.first_composer.take() {
            assert!(
                first.release_publication().is_ok(),
                "qualified first editor transfers with whole graph publication"
            );
        }
        let retirement = self.recovery_retirement.take().unwrap();
        self.attempt = InitialServiceAttemptState::Published(Some(retirement.fence));
        drop(prepared.take());
        Ok(start)
    }

    pub(crate) fn reopen_recovery_admission(
        &mut self,
        generation: HomeGeneration,
    ) -> Result<(), String> {
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
        graph
            .cas()
            .try_reopen_shutdown_admission(fence)
            .map_err(|error| error.to_string())?;
        self.attempt = InitialServiceAttemptState::Published(None);
        Ok(())
    }
}
