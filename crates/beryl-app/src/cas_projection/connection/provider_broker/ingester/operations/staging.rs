use beryl_backend::{
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSubmitCause, ProviderObservationFragment,
};
use syndic_storage::{
    ProviderCompactionMarkerStager, ProviderObservationStageBatchError,
    ProviderObservationStageOutcome, ProviderObservationStager, ProviderObservationStagingBytes,
    ProviderObservationStagingError,
};

use super::super::Ingester;

pub(super) fn staging_rejection(
    error: &ProviderObservationStagingError,
) -> OrderedTurnStreamRejection {
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
        | ProviderObservationStagingError::Record(_) => OrderedTurnStreamRejection::StagingConflict,
    }
}

enum ProviderStagingOutcome {
    Rejection(OrderedTurnStreamRejection),
}

fn classify_stage_outcome<T>(
    outcome: ProviderObservationStageOutcome<T>,
) -> Result<T, ProviderStagingOutcome> {
    match outcome {
        ProviderObservationStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => Ok(value),
        ProviderObservationStageOutcome::NotCommitted { .. }
        | ProviderObservationStageOutcome::Committed {
            later_failure: Some(_),
            ..
        } => Err(ProviderStagingOutcome::Rejection(
            OrderedTurnStreamRejection::StagingConflict,
        )),
        ProviderObservationStageOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install();
            Err(ProviderStagingOutcome::Rejection(
                OrderedTurnStreamRejection::StagingConflict,
            ))
        }
    }
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
        let verification = match self.live_command().enter_current_home(
            &self.home,
            self.home_id,
            self.home_generation,
        ) {
            Ok(verification) => verification,
            Err(_) => return self.authority_lost_terminal(),
        };
        let authority = self.current_observation_authority_typed();
        if verification.settle_after_operation().is_err() {
            return self.authority_lost_terminal();
        }
        let (home_generation, storage) = match authority {
            Ok(authority) => authority,
            Err(_) => {
                return self.reject(
                    OrderedTurnStreamOperation::ProviderBegin(begin),
                    OrderedTurnStreamRejection::StagingConflict,
                );
            }
        };
        let mut commit = self.committer(identity, home_generation, &storage);
        let staged = ProviderObservationStager::begin(identity, translated, &mut commit)
            .map_err(|error| ProviderStagingOutcome::Rejection(staging_rejection(&error)))
            .and_then(classify_stage_outcome);
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
            Err(ProviderStagingOutcome::Rejection(rejection)) => {
                self.reject(OrderedTurnStreamOperation::ProviderBegin(begin), rejection)
            }
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
                    &observation.storage,
                );
                observation
                    .stager
                    .control(translated, &mut commit)
                    .map_err(|error| ProviderStagingOutcome::Rejection(staging_rejection(&error)))
                    .and_then(classify_stage_outcome)
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
                    &observation.storage,
                );
                observation
                    .stager
                    .fragment(staged, &mut commit)
                    .map_err(|error| ProviderStagingOutcome::Rejection(staging_rejection(&error)))
                    .and_then(classify_stage_outcome)
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
