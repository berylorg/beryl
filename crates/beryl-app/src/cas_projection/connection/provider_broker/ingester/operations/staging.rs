use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSubmitCause, ProviderObservationFragment,
};
use syndic_storage::{
    ProviderCompactionMarkerStager, ProviderObservationStageBatchError, ProviderObservationStager,
    ProviderObservationStagingBytes, ProviderObservationStagingError,
};

use super::super::super::staging::StageCommitError;
use super::super::Ingester;

pub(super) fn staging_rejection<E>(
    error: &ProviderObservationStagingError<E>,
) -> OrderedTurnStreamRejection
where
    E: std::error::Error + Send + Sync + 'static,
{
    match error {
        ProviderObservationStagingError::Validation(_)
        | ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::EmptyFragment
            | ProviderObservationStageBatchError::FragmentTooLarge { .. },
        ) => OrderedTurnStreamRejection::SchemaMismatch,
        ProviderObservationStagingError::Batch(
            ProviderObservationStageBatchError::InvalidTransition
            | ProviderObservationStageBatchError::FrontierOverflow
            | ProviderObservationStageBatchError::ReplayMismatch,
        )
        | ProviderObservationStagingError::Record(_)
        | ProviderObservationStagingError::Callback(_) => {
            OrderedTurnStreamRejection::StagingConflict
        }
    }
}

pub(super) fn staging_authority(
    error: &ProviderObservationStagingError<StageCommitError>,
) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
    match error {
        ProviderObservationStagingError::Callback(source) => source.authority(),
        _ => None,
    }
}

enum ProviderStagingOutcome {
    Rejection(OrderedTurnStreamRejection),
    AuthorityLost,
}

impl Ingester {
    pub(in super::super) fn abandon_provider(
        &mut self,
        reason: beryl_backend::ProviderObservationAbandonReason,
    ) -> (super::super::BrokerReply, bool) {
        let Some(observation) = self.take_provider() else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderAbandon(reason),
                OrderedTurnStreamRejection::InvalidControl,
            );
        };
        observation.abandon();
        (
            super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
            false,
        )
    }

    pub(in super::super) fn begin(
        &mut self,
        begin: beryl_backend::ProviderObservationBegin,
    ) -> (super::super::BrokerReply, bool) {
        if self.active.is_some() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let translated = super::super::super::translation::begin(begin);
        if matches!(
            translated,
            syndic_storage::ProviderObservationBegin::Item {
                kind: syndic_storage::ProviderObservationItemKind::ContextCompaction,
                ..
            }
        ) {
            return match ProviderCompactionMarkerStager::begin(translated) {
                Ok(marker) => {
                    self.put_provider(super::super::ActiveObservation::Compaction(marker));
                    (
                        super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                        false,
                    )
                }
                Err(_) => self.reject(
                    OrderedTurnStreamOperation::ProviderBegin(begin),
                    OrderedTurnStreamRejection::SchemaMismatch,
                ),
            };
        }
        let mut bytes = [0_u8; 16];
        if getrandom::fill(&mut bytes).is_err() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                OrderedTurnStreamRejection::StagingConflict,
            );
        }
        let identity = beryl_model::ProviderObservationId::from_bytes(bytes);
        let mut verified_continuation = false;
        let (home_generation, storage) = loop {
            let verification = if verified_continuation {
                None
            } else {
                match self.live_command().await_current_or_verification(
                    &self.home,
                    self.home_id,
                    self.home_generation,
                ) {
                    Ok(verification) => Some(verification),
                    Err(_) => return self.authority_lost_terminal(),
                }
            };
            #[cfg(all(test, feature = "test-faults"))]
            super::tests::pause_begin_preparation(self.home_id, identity);
            let authority = self.current_observation_authority_typed();
            let Some(verification) = verification else {
                match authority {
                    Ok(authority) => break authority,
                    Err(error) if error.verification_ambiguous(self.home_generation) => {
                        verified_continuation = false;
                        continue;
                    }
                    Err(_) => {
                        return self.reject(
                            OrderedTurnStreamOperation::ProviderBegin(begin),
                            OrderedTurnStreamRejection::StagingConflict,
                        );
                    }
                }
            };
            match verification.settle_after_operation() {
                Ok(settlement) if settlement.verified_current() => match &authority {
                    Ok(_) => {
                        verified_continuation = true;
                        continue;
                    }
                    Err(error) if error.verification_ambiguous(self.home_generation) => {
                        verified_continuation = true;
                        continue;
                    }
                    Err(_) => {}
                },
                Ok(_) => {}
                Err(_) => return self.authority_lost_terminal(),
            }
            match authority {
                Ok(authority) => break authority,
                Err(_) => {
                    return self.reject(
                        OrderedTurnStreamOperation::ProviderBegin(begin),
                        OrderedTurnStreamRejection::StagingConflict,
                    );
                }
            }
        };
        let mut commit = self.committer(identity, home_generation, storage);
        let staged = ProviderObservationStager::begin(identity, translated, &mut commit);
        match staged {
            Ok(observation) => {
                self.put_provider(super::super::ActiveObservation::Durable(
                    super::super::active::ActiveDurableObservation {
                        stager: observation,
                        identity,
                        home_generation,
                        storage,
                    },
                ));
                (
                    super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    false,
                )
            }
            Err(ref error) if staging_authority(error).is_some() => self.authority_lost_terminal(),
            Err(error) => self.reject(
                OrderedTurnStreamOperation::ProviderBegin(begin),
                staging_rejection(&error),
            ),
        }
    }

    pub(in super::super) fn control(
        &mut self,
        control: beryl_backend::ProviderObservationControl,
    ) -> (super::super::BrokerReply, bool) {
        let Some(mut observation) = self.take_provider() else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderControl(control),
                OrderedTurnStreamRejection::InvalidControl,
            );
        };
        let translated = super::super::super::translation::control(control);
        let result = match &mut observation {
            super::super::ActiveObservation::Durable(observation) => {
                let mut commit = self.committer(
                    observation.identity,
                    observation.home_generation,
                    observation.storage,
                );
                observation
                    .stager
                    .control(translated, &mut commit)
                    .map_err(|error| {
                        if staging_authority(&error).is_some() {
                            ProviderStagingOutcome::AuthorityLost
                        } else {
                            ProviderStagingOutcome::Rejection(staging_rejection(&error))
                        }
                    })
            }
            super::super::ActiveObservation::Compaction(marker) => {
                marker.control(translated).map_err(|_| {
                    ProviderStagingOutcome::Rejection(OrderedTurnStreamRejection::SchemaMismatch)
                })
            }
        };
        match result {
            Ok(()) => {
                self.put_provider(observation);
                (
                    super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::Applied),
                    false,
                )
            }
            Err(ProviderStagingOutcome::AuthorityLost) => self.authority_lost_terminal(),
            Err(ProviderStagingOutcome::Rejection(rejection)) => {
                observation.abandon();
                self.reject(
                    OrderedTurnStreamOperation::ProviderControl(control),
                    rejection,
                )
            }
        }
    }

    pub(in super::super) fn acquire_page(&mut self) -> (super::super::BrokerReply, bool) {
        if !self.provider_is_active() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderAcquirePage,
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        match self.pages.try_lease() {
            Ok(page) => (
                super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(page)),
                false,
            ),
            Err(_) => (
                super::super::BrokerReply::Rejected(
                    OrderedTurnStreamOperation::ProviderAcquirePage,
                    OrderedTurnStreamSubmitCause::CapacityFull,
                ),
                true,
            ),
        }
    }

    pub(in super::super) fn fragment(
        &mut self,
        fragment: ProviderObservationFragment,
    ) -> (super::super::BrokerReply, bool) {
        if !self.provider_is_active() {
            return self.reject(
                OrderedTurnStreamOperation::ProviderFragment(fragment),
                OrderedTurnStreamRejection::InvalidControl,
            );
        }
        let staged = ProviderObservationStagingBytes::new(
            super::super::super::translation::context(fragment.context()),
            fragment.bytes(),
        );
        let Ok(staged) = staged else {
            return self.reject(
                OrderedTurnStreamOperation::ProviderFragment(fragment),
                OrderedTurnStreamRejection::SchemaMismatch,
            );
        };
        let mut observation = self
            .take_provider()
            .expect("provider fragment requires one active observation");
        let result = match &mut observation {
            super::super::ActiveObservation::Durable(observation) => {
                let mut commit = self.committer(
                    observation.identity,
                    observation.home_generation,
                    observation.storage,
                );
                observation
                    .stager
                    .fragment(staged, &mut commit)
                    .map_err(|error| {
                        if staging_authority(&error).is_some() {
                            ProviderStagingOutcome::AuthorityLost
                        } else {
                            ProviderStagingOutcome::Rejection(staging_rejection(&error))
                        }
                    })
            }
            super::super::ActiveObservation::Compaction(marker) => {
                marker.fragment(staged).map_err(|_| {
                    ProviderStagingOutcome::Rejection(OrderedTurnStreamRejection::SchemaMismatch)
                })
            }
        };
        match result {
            Ok(()) => {
                self.put_provider(observation);
                let mut page = fragment.into_lease();
                page.clear();
                (
                    super::super::BrokerReply::Applied(OrderedTurnStreamCompletion::PageLease(
                        page,
                    )),
                    false,
                )
            }
            Err(ProviderStagingOutcome::AuthorityLost) => self.authority_lost_terminal(),
            Err(ProviderStagingOutcome::Rejection(rejection)) => {
                observation.abandon();
                self.reject(
                    OrderedTurnStreamOperation::ProviderFragment(fragment),
                    rejection,
                )
            }
        }
    }
}
