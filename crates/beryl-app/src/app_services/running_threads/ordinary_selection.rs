use super::*;

impl PublishedRunningThreadsReader {
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
