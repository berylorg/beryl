use super::*;
use crate::{
    activity_service::PreparedActivityService,
    cas_projection::{
        PreparedRecoveryCasServices, RecoveryCasCloseFailure, RecoveryCasPreparationFailure,
    },
    composer_marker_seal::initial_preparation::PreparedMarkerServices,
};

pub(crate) struct PreparedRecoveryAppServices {
    cas: Option<PreparedRecoveryCasServices>,
    marker: Option<PreparedMarkerServices>,
    activity: Option<PreparedActivityService>,
    theme: Option<PreparedThemeRuntime>,
}

pub(crate) struct RecoveryAppServicePreparationFailure {
    error: AppServiceOpenError,
    cas: RecoveryCasPreparationFailure,
}

#[derive(Debug)]
pub(crate) struct RecoveryAppServiceCloseFailure {
    _preparation: AppServiceOpenError,
    _cas: RecoveryCasCloseFailure,
}

impl std::fmt::Debug for RecoveryAppServicePreparationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecoveryAppServicePreparationFailure")
            .field("error", &self.error)
            .field("cas", &self.cas.error())
            .finish_non_exhaustive()
    }
}

impl RecoveryAppServicePreparationFailure {
    pub(crate) fn close(self) -> Result<(), RecoveryAppServiceCloseFailure> {
        self.cas
            .close()
            .map_err(|cas| RecoveryAppServiceCloseFailure {
                _preparation: self.error,
                _cas: cas,
            })
    }

    pub(crate) fn error(&self) -> &AppServiceOpenError {
        &self.error
    }

    pub(crate) fn into_retry_parts(
        self,
    ) -> Result<(HomeStore, AppServiceOpenError, CasPreparationError), Self> {
        match self.cas.into_retry_parts() {
            Ok((home, cas)) => Ok((home, self.error, cas)),
            Err(cas) => Err(Self {
                error: self.error,
                cas,
            }),
        }
    }
}

impl PreparedRecoveryAppServices {
    pub(crate) fn prepare(
        cas: PreparedRecoveryCasServices,
        state: &BerylState,
        syndic: SyndicStorage,
        configuration: AppServiceConfiguration,
        cancellation: &CommandCancellation,
    ) -> Result<Self, RecoveryAppServicePreparationFailure> {
        let mut prepared = Self {
            cas: Some(cas),
            marker: None,
            activity: None,
            theme: None,
        };
        let result: Result<(), AppServiceOpenError> = (|| {
            check_cancellation(cancellation)?;
            let (candidate, cas) = prepared
                .cas
                .as_mut()
                .expect("prepared recovery CAS custody")
                .app_preparation_parts()
                .ok_or(AppServiceOpenError::HandoffUnavailable)?;
            let runtime = cas
                .activity_read_source()
                .ok_or(AppServiceOpenError::RuntimeUnavailable)?;
            prepared.marker = Some(PreparedMarkerServices::prepare_recovery(
                candidate,
                syndic.clone(),
                state.assets(),
                configuration.marker,
            )?);
            check_cancellation(cancellation)?;
            prepared.activity = Some(PreparedActivityService::prepare_recovery(
                candidate,
                syndic,
                runtime,
                configuration.activity,
            )?);
            check_cancellation(cancellation)?;
            prepared.theme = Some(PreparedThemeRuntime::prepare_recovery(
                candidate,
                state.themes(),
                configuration.theme,
            )?);
            check_cancellation(cancellation)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(prepared),
            Err(error) => Err(prepared.fail(error)),
        }
    }

    pub(crate) fn cancel(self) -> RecoveryAppServicePreparationFailure {
        self.fail(AppServiceOpenError::Cancelled)
    }

    fn fail(mut self, error: AppServiceOpenError) -> RecoveryAppServicePreparationFailure {
        self.join_ancillary_services();
        let cas = self
            .cas
            .take()
            .expect("prepared recovery CAS custody")
            .cancel();
        RecoveryAppServicePreparationFailure { error, cas }
    }

    fn join_ancillary_services(&mut self) {
        drop(self.theme.take());
        drop(self.activity.take());
        drop(self.marker.take());
    }
}

impl Drop for PreparedRecoveryAppServices {
    fn drop(&mut self) {
        self.join_ancillary_services();
        drop(self.cas.take());
    }
}
