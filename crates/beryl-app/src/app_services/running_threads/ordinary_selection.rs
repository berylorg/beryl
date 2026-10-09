use super::*;

impl PublishedRunningThreadsReader {
    pub(crate) fn ordinary_thread_unavailability(
        &self,
        window: beryl_model::WindowId,
        prior: Option<beryl_state::WindowClaimSelection>,
        thread: beryl_model::SyndicThreadId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(RunningThreadsObservation, Option<String>), String> {
        if cancellation.is_cancelled() {
            return Err("Thread availability read cancelled".into());
        }
        let observed = self.observe()?;
        let reason = match self.prepare_ordinary_thread_activation(window, prior, thread, &beryl_home_store::CommandCancellation::new()) {
            Ok((crate::main_window::running_threads::activation::RunningThreadActivationPreparation::Prepared(_), _)) => None,
            Ok((crate::main_window::running_threads::activation::RunningThreadActivationPreparation::Current { .. }, _)) => Some("The requested thread is already selected.".into()),
            Ok((crate::main_window::running_threads::activation::RunningThreadActivationPreparation::ClaimedElsewhere { .. }, _)) => Some("The requested thread is open in another window.".into()),
            Err(error) => Some(error),
        };
        if cancellation.is_cancelled() {
            return Err("Thread availability read cancelled".into());
        }
        self.elect(&observed, || ())?;
        Ok((
            observed,
            reason.map(|reason| reason.chars().take(512).collect()),
        ))
    }

    pub(crate) fn prepare_ordinary_thread_activation(
        &self,
        window: beryl_model::WindowId,
        prior: Option<beryl_state::WindowClaimSelection>,
        thread: beryl_model::SyndicThreadId,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<
        (
            crate::main_window::running_threads::activation::RunningThreadActivationPreparation,
            RunningThreadsObservation,
        ),
        String,
    > {
        if cancellation.is_cancelled() {
            return Err("Thread activation cancelled".into());
        }
        let (home, state, syndic) = self
            .activation_sources()
            .ok_or("Thread activation source retired")?;
        let observed = self.observe()?;
        let execution = syndic
            .thread_execution(
                &home,
                thread,
                syndic_storage::SyndicPointReadLimit::new(256 * 1024)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?
            .ok_or("The requested thread is unavailable.")?;
        let target = beryl_state::RememberedTarget::new(
            execution.execution().runtime_id(),
            execution.execution().root_id(),
        );
        let prepared =
            crate::main_window::running_threads::activation::RunningThreadActivation::prepare(
                &home, &state, &syndic, window, prior, target, thread,
            )
            .map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            return Err("Thread activation cancelled".into());
        }
        self.elect(&observed, || ())?;
        Ok((prepared, observed))
    }
}
