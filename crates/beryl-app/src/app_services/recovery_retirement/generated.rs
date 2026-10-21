use super::*;

impl ServiceGraphRetirement {
    pub(super) fn finish_components(&mut self, expected: HomeGeneration) {
        let graph = self.graph.as_mut().expect("retained retiring graph");
        assert!(graph.cas.is_none());
        let generated = graph.join_generated_media_retirement();
        let handoff = graph.handoff.as_mut().expect("graph handoff").shutdown();
        drop(graph.handoff.take());
        if let Some(activity) = graph.activity.take() { activity.retire(); }
        self.marker = graph.marker.take();
        if let Some(marker) = &self.marker { marker.retire_home_generation(); }
        drop(graph.theme.take());
        if let Some(mut theme) = graph.loaded_theme.take() { theme.retire(); }
        graph.attention.close();
        self.home = graph.home.take();
        drop(self.graph.take());
        let handoff = failed_retirement::settled_handoff(handoff, expected);
        let exact_failed_home = self.home.as_ref().is_some_and(|home| {
            let health = home.health();
            health.state() == HomeHealthState::Failed && health.generation() == Some(expected)
        });
        if exact_failed_home && matches!(
            &handoff,
            Err(HandoffCoordinatorError::Read(beryl_home_store::ReadError::Storage { .. }))
                | Err(HandoffCoordinatorError::Page(beryl_state::DurableJobReadError::Read(beryl_home_store::ReadError::Storage { .. })))
        ) {
            self.joined_handoff_read_failure = handoff.err();
        } else if let Err(error) = handoff {
            self.failure.get_or_insert_with(|| error.into());
        }
        if let Err(error) = generated { self.failure.get_or_insert_with(|| error.into()); }
    }
}
