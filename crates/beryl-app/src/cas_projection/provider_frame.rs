use beryl_home_store::{CommandError, CommandOutcome, CommitReceipt, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, CasItemId, SyndicItemId};
use syndic_storage::{
    CasItemSource, PreparedProviderFrame, ProviderFramePreparationPlan, ProviderFrameStageBatch,
    ProviderFrameStageError, ProviderFrameStageOutcome, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderItemFrameV1, ProviderNarrativeCompletionDisposition,
    SourceEventPayload, SyndicPointReadLimit, SyndicReadError, SyndicStorage,
    prepare_provider_frame, stage_provider_frame,
};
use thiserror::Error;

use super::live_source::{
    LiveSourceFrontier, LiveSourcePublicationError, LiveSourceTarget, publish_provider_reconciled,
};
use super::provider_identity;

#[derive(Debug, Error)]
pub(super) enum ProviderFramePublicationError {
    #[error(transparent)]
    Preparation(#[from] syndic_storage::ProviderFramePreparationError),
    #[error("provider-frame build command failed before durable admission: {0}")]
    BeginCommand(#[source] CommandError),
    #[error("provider-frame build committed before a later failure: {later_failure}")]
    BeginCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("provider-frame build has an indeterminate durable outcome: {failure}")]
    BeginIndeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("provider-frame build command reported success without durable admission")]
    BeginPrior,
    #[error("provider-frame build identity collided with another durable build")]
    BeginCollision,
    #[error(transparent)]
    Stage(#[from] ProviderFrameStageError),
    #[error("provider-frame staging has an indeterminate durable outcome: {failure}")]
    StageIndeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("provider completion comparison command failed before durable admission: {0}")]
    CompletionCommand(#[source] CommandError),
    #[error("provider completion comparison committed before a later failure: {later_failure}")]
    CompletionCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("provider completion comparison has an indeterminate durable outcome: {failure}")]
    CompletionIndeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("provider completion comparison reported success without advancing")]
    CompletionPrior,
    #[error("provider completion comparison collided with another durable build")]
    CompletionCollision,
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error("provider-frame build lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
    #[error(transparent)]
    LiveSource(#[from] LiveSourcePublicationError),
}

impl ProviderFramePublicationError {
    pub(super) fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            Self::LiveSource(source) => source.authority(),
            _ => None,
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum ProviderBatchDispatchError {
    #[error("provider-frame batch command failed before durable admission: {0}")]
    Command(#[source] CommandError),
    #[error("provider-frame batch committed before a later failure: {later_failure}")]
    Committed {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("provider-frame batch has an indeterminate durable outcome: {failure}")]
    Indeterminate {
        #[source]
        failure: CommandError,
    },
    #[error("provider-frame batch command reported success without advancing")]
    Prior,
    #[error("provider-frame batch collided with another durable build")]
    Collision,
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error("provider-frame batch lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
}

impl ProviderBatchDispatchError {
    fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            _ => None,
        }
    }
}

pub(super) struct PublishedProviderFrame {
    pub(super) completion: Option<ProviderNarrativeCompletionDisposition>,
}

pub(super) struct ProviderFramePublication {
    pub(super) target: LiveSourceTarget,
    pub(super) item_id: SyndicItemId,
    pub(super) cas_item_id: CasItemId,
    pub(super) frame: ProviderItemFrameV1,
    pub(super) prior: Option<syndic_storage::SealedProviderFrameReference>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish_frame(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    publication: ProviderFramePublication,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<PublishedProviderFrame, ProviderFramePublicationError> {
    let ProviderFramePublication {
        target,
        item_id,
        cas_item_id,
        frame,
        prior,
    } = publication;
    let frontier = read_initial_frontier(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        &target,
        limit,
        command,
    )?;
    let source = CasItemSource::new(target.source().clone(), cas_item_id);
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item_id,
            target.turn_id(),
            source,
            frontier.sequence(),
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item_id,
            target.turn_id(),
            source,
            frontier.sequence(),
            provider_content_id(&target, item_id),
            frame,
        ),
    };
    let prepared = prepare_provider_frame(plan)?;
    let current = begin_build(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        &prepared,
        limit,
        command,
    )?;
    let staged = stage_build(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        &prepared,
        current,
        limit,
        command,
    )?;
    let sealed = complete_comparison(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        staged,
        limit,
        command,
    )?;
    let completion = sealed
        .completion_check()
        .and_then(syndic_storage::ProviderNarrativeCompletionCheck::disposition);
    let event = frontier.event(
        &target,
        Some(target.source().clone()),
        SourceEventPayload::ItemFrame {
            item_id,
            frame: Box::new(sealed.target().clone()),
        },
    )?;
    publish_provider_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage,
        &event,
        limit,
        command,
    )?;
    Ok(PublishedProviderFrame { completion })
}

fn read_initial_frontier(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    target: &LiveSourceTarget,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<LiveSourceFrontier, ProviderFramePublicationError> {
    loop {
        let verification = command
            .enter_current_home(store, home_id, home_generation)
            .map_err(ProviderFramePublicationError::Authority)?;
        let frontier = LiveSourceFrontier::read(store, storage, target, limit);
        match verification.settle_after_operation() {
            Ok(settlement) if settlement.requires_retry() => continue,
            Ok(_) => return frontier.map_err(ProviderFramePublicationError::LiveSource),
            Err(source) => return Err(ProviderFramePublicationError::Authority(source)),
        }
    }
}

fn begin_build(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    prepared: &PreparedProviderFrame,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<ProviderItemBuildRecord, ProviderFramePublicationError> {
    let verification = command
        .enter_current_home(store, home_id, home_generation)
        .map_err(ProviderFramePublicationError::Authority)?;
    let dispatch = store.execute_current(storage.current_begin_provider_frame_build(prepared));
    let dispatch = match dispatch {
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            verification
                .settle_after_operation()
                .map_err(ProviderFramePublicationError::Authority)?;
            return Err(ProviderFramePublicationError::BeginIndeterminate { failure });
        }
        dispatch => dispatch,
    };
    verification
        .settle_after_operation()
        .map_err(ProviderFramePublicationError::Authority)?;
    match dispatch {
        CommandOutcome::NotCommitted { evidence } => {
            return Err(ProviderFramePublicationError::BeginCommand(evidence));
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => {}
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => {
            return Err(ProviderFramePublicationError::BeginCommitted {
                receipt,
                later_failure,
            });
        }
        CommandOutcome::Indeterminate { .. } => unreachable!(),
    }
    match read_build(
        store,
        home_id,
        home_generation,
        storage,
        prepared.initial_build().item_id(),
        limit,
        command,
    )
    .map_err(map_publication_read_error)?
    {
        Some(current) if &current == prepared.initial_build() => Ok(current),
        None => Err(ProviderFramePublicationError::BeginPrior),
        Some(_) => Err(ProviderFramePublicationError::BeginCollision),
    }
}

fn stage_build(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    prepared: &PreparedProviderFrame,
    current: ProviderItemBuildRecord,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<ProviderItemBuildRecord, ProviderFramePublicationError> {
    let outcome =
        stage_provider_frame(prepared, current, &mut |batch: &ProviderFrameStageBatch| {
            store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))
        })?;
    match outcome {
        ProviderFrameStageOutcome::Unchanged { value }
        | ProviderFrameStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => Ok(value),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            Err(ProviderFrameStageError::NotCommitted { evidence }.into())
        }
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: Some(later_failure),
        } => Err(ProviderFrameStageError::CommittedLaterFailure {
            value,
            receipt,
            later_failure,
        }
        .into()),
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(ProviderFramePublicationError::StageIndeterminate { failure })
        }
    }
}

fn dispatch_batch(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    batch: &ProviderFrameStageBatch,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<(), ProviderBatchDispatchError> {
    let verification = command
        .enter_current_home(store, home_id, home_generation)
        .map_err(ProviderBatchDispatchError::Authority)?;
    let dispatch = store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()));
    let dispatch = match dispatch {
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            verification
                .settle_after_operation()
                .map_err(ProviderBatchDispatchError::Authority)?;
            return Err(ProviderBatchDispatchError::Indeterminate { failure });
        }
        dispatch => dispatch,
    };
    verification
        .settle_after_operation()
        .map_err(ProviderBatchDispatchError::Authority)?;
    match dispatch {
        CommandOutcome::NotCommitted { evidence } => {
            return Err(ProviderBatchDispatchError::Command(evidence));
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => {}
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => {
            return Err(ProviderBatchDispatchError::Committed {
                receipt,
                later_failure,
            });
        }
        CommandOutcome::Indeterminate { .. } => unreachable!(),
    }
    match read_build(
        store,
        home_id,
        home_generation,
        storage,
        batch.expected_build().item_id(),
        limit,
        command,
    )
    .map_err(map_batch_read_error)?
    {
        Some(current) if &current == batch.next_build() => Ok(()),
        Some(current) if &current == batch.expected_build() => {
            Err(ProviderBatchDispatchError::Prior)
        }
        Some(_) | None => Err(ProviderBatchDispatchError::Collision),
    }
}

fn complete_comparison(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    mut current: ProviderItemBuildRecord,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<ProviderItemBuildRecord, ProviderFramePublicationError> {
    while current.lifecycle() != ProviderItemBuildLifecycle::Sealed {
        let verification = command
            .enter_current_home(store, home_id, home_generation)
            .map_err(ProviderFramePublicationError::Authority)?;
        let dispatch =
            store.execute_current(storage.current_compare_provider_completion(current.clone()));
        let dispatch = match dispatch {
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                reconciliation.install();
                verification
                    .settle_after_operation()
                    .map_err(ProviderFramePublicationError::Authority)?;
                return Err(ProviderFramePublicationError::CompletionIndeterminate { failure });
            }
            dispatch => dispatch,
        };
        verification
            .settle_after_operation()
            .map_err(ProviderFramePublicationError::Authority)?;
        match dispatch {
            CommandOutcome::NotCommitted { evidence } => {
                return Err(ProviderFramePublicationError::CompletionCommand(evidence));
            }
            CommandOutcome::Committed {
                receipt: _,
                later_failure: None,
            } => {}
            CommandOutcome::Committed {
                receipt,
                later_failure: Some(later_failure),
            } => {
                return Err(ProviderFramePublicationError::CompletionCommitted {
                    receipt,
                    later_failure,
                });
            }
            CommandOutcome::Indeterminate { .. } => unreachable!(),
        }
        let durable = read_build(
            store,
            home_id,
            home_generation,
            storage,
            current.item_id(),
            limit,
            command,
        )
        .map_err(map_publication_read_error)?;
        let Some(next) = durable else {
            return Err(ProviderFramePublicationError::CompletionCollision);
        };
        if next == current {
            return Err(ProviderFramePublicationError::CompletionPrior);
        }
        let Some(next_state) = next.completion_check().map(|check| check.state()) else {
            return Err(ProviderFramePublicationError::CompletionCollision);
        };
        let expected_next = current
            .advance_completion(next_state)
            .map_err(|_| ProviderFramePublicationError::CompletionCollision)?;
        if next != expected_next {
            return Err(ProviderFramePublicationError::CompletionCollision);
        }
        current = next;
    }
    Ok(current)
}

fn read_build(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: &SyndicStorage,
    item_id: SyndicItemId,
    limit: SyndicPointReadLimit,
    command: &crate::cas_projection::LiveCommandPermit,
) -> Result<Option<ProviderItemBuildRecord>, ProviderFrameReadError> {
    loop {
        let verification = command
            .enter_current_home(store, home_id, home_generation)
            .map_err(ProviderFrameReadError::Authority)?;
        let current = storage.provider_item_build(store, item_id, limit);
        match verification.settle_after_operation() {
            Ok(settlement) if settlement.requires_retry() => continue,
            Ok(_) => return current.map_err(ProviderFrameReadError::Read),
            Err(source) => return Err(ProviderFrameReadError::Authority(source)),
        }
    }
}

enum ProviderFrameReadError {
    Authority(crate::cas_projection::LiveCommandAdmissionError),
    Read(SyndicReadError),
}

fn map_publication_read_error(error: ProviderFrameReadError) -> ProviderFramePublicationError {
    match error {
        ProviderFrameReadError::Authority(source) => {
            ProviderFramePublicationError::Authority(source)
        }
        ProviderFrameReadError::Read(source) => ProviderFramePublicationError::Read(source),
    }
}

fn map_batch_read_error(error: ProviderFrameReadError) -> ProviderBatchDispatchError {
    match error {
        ProviderFrameReadError::Authority(source) => ProviderBatchDispatchError::Authority(source),
        ProviderFrameReadError::Read(source) => ProviderBatchDispatchError::Read(source),
    }
}

fn provider_content_id(
    target: &LiveSourceTarget,
    item_id: SyndicItemId,
) -> beryl_model::SyndicContentId {
    provider_identity::provider_content_id(
        target.thread_id(),
        target.turn_id(),
        target.source(),
        item_id,
    )
}
