use super::*;
use crate::{
    activity_service::PreparedActivityService,
    cas_projection::{
        PreparedCasServices, ProcessScheduledExecutionProvider, initial_start::InitialStartOwner,
    },
    composer_marker_seal::initial_preparation::PreparedMarkerServices,
    discussion_settlement::DiscussionSettlementService,
};

pub(super) struct PreparedAppServices {
    process: ProcessAdmissionGate,
    cas: Option<PreparedCasServices>,
    marker: Option<PreparedMarkerServices>,
    activity: Option<PreparedActivityService>,
    theme: Option<PreparedThemeRuntime>,
    pub(super) candidate: Option<HomeOpenPublication>,
    sessions: ScheduledExecutionSessions,
    attention: Arc<ProcessLifecycleAttentionPool>,
    state: BerylState,
    pub(super) syndic: SyndicStorage,
}

impl PreparedAppServices {
    pub(super) fn prepare(
        owner: &ProcessServiceOwner,
        candidate: HomeOpenPublication,
        state: BerylState,
        syndic: SyndicStorage,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<Self, AppServiceOpenError> {
        let (provider, sessions) = ProcessScheduledExecutionProvider::new();
        let mut prepared = Self {
            process: owner.process.clone(),
            cas: None,
            marker: None,
            activity: None,
            theme: None,
            candidate: Some(candidate),
            sessions,
            attention: Arc::new(ProcessLifecycleAttentionPool::new()),
            state,
            syndic,
        };
        check_cancellation(cancellation)?;
        let candidate = prepared.candidate.as_mut().expect("private candidate");
        {
            let access = candidate.recovery_access()?;
            owner
                .enrollments
                .settle_retired_candidate(&access, &prepared.syndic, cancellation)?;
            owner.settlements.settle_retained_nondispatch_candidate(
                &access,
                &prepared.state,
                &prepared.syndic,
                cancellation.clone(),
            )?;
        }
        let settlement = DiscussionSettlementService::new(
            owner.settlements.clone(),
            candidate.service_reference(),
            prepared.state.clone(),
            prepared.syndic.clone(),
        );
        let provider = provider.with_discussion_settlement(settlement);
        prepared.cas = Some(
            PreparedCasServices::prepare(
                owner.process.clone(),
                candidate,
                prepared.syndic.clone(),
                configuration.projection,
                Box::new(provider),
            )?
            .configure_managed_sessions(
                candidate,
                &prepared.sessions,
                configuration.runtime_interest,
                owner.enrollments.clone(),
                RuntimeSessionPreparationConfig {
                    runtime_roots: prepared.state.runtime_roots(),
                    assets: prepared.state.assets(),
                    policy: configuration.session_policy,
                    token_directories: configuration.token_directories,
                },
                &prepared.attention,
            )?
            .prepare_handoff(
                candidate,
                owner.settlements.clone(),
                prepared.state.clone(),
                configuration.handoff,
                at,
                cancellation.clone(),
            )?,
        );
        check_cancellation(cancellation)?;
        prepared.marker = Some(PreparedMarkerServices::prepare(
            candidate,
            prepared.syndic.clone(),
            prepared.state.assets(),
            configuration.marker,
        )?);
        let runtime = prepared
            .cas
            .as_ref()
            .expect("prepared CAS")
            .activity_read_source()
            .ok_or(AppServiceOpenError::RuntimeUnavailable)?;
        prepared.activity = Some(PreparedActivityService::prepare(
            candidate,
            prepared.syndic.clone(),
            runtime,
            configuration.activity,
        )?);
        prepared.theme = Some(PreparedThemeRuntime::prepare(
            candidate,
            prepared.state.themes(),
            configuration.theme,
        )?);
        check_cancellation(cancellation)?;
        Ok(prepared)
    }

    pub(super) fn publish(
        mut self,
        cancellation: &CommandCancellation,
    ) -> Result<(PublishedAppServices, InitialStartOwner), AppServiceOpenError> {
        check_cancellation(cancellation)?;
        let home = match self.candidate.take().expect("private candidate").publish() {
            Ok(home) => home,
            Err(failure) => {
                let (error, candidate) = failure.into_parts();
                self.candidate = Some(candidate);
                return Err(error.into());
            }
        };
        let (cas, handoff, start) = self
            .cas
            .take()
            .expect("prepared CAS")
            .into_published_parts();
        let graph = PublishedAppServices {
            restore_lifetime: Some(Arc::new(())),
            process: self.process.clone(),
            shutdown: None,
            shutdown_ready: false,
            handoff: Some(handoff),
            activity: Some(
                self.activity
                    .take()
                    .expect("prepared Activity")
                    .into_service(),
            ),
            marker: Some(self.marker.take().expect("prepared marker").into_service()),
            theme: self.theme.take(),
            loaded_theme: None,
            cas: Some(cas),
            sessions: self.sessions.clone(),
            attention: Arc::clone(&self.attention),
            state: self.state.clone(),
            syndic: self.syndic.clone(),
            home: Some(home),
        };
        Ok((graph, start))
    }
}

impl Drop for PreparedAppServices {
    fn drop(&mut self) {
        drop(self.cas.take());
        drop(self.activity.take());
        drop(self.marker.take());
        drop(self.theme.take());
        drop(self.candidate.take());
    }
}
