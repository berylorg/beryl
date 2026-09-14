use super::*;
use crate::{
    cas_projection::{
        CasProjectionCoordinator, CompactionOperationWorkFact, CompactionWorkError,
        CompactionWorkPageLimits, ProjectionServiceGeneration,
        service::flight_registry::FlightRegistry,
    },
    process_admission::{ProcessAdmissionError, ProcessAdmissionFence},
};
use beryl_model::SyndicTurnId;
use syndic_storage::{
    CompactionOperationId, CompactionOperationRecord, CompactionRecoveryCase, SyndicPointReadLimit,
    SyndicReadError,
};

const CAPTURE_PAGE_BYTES: usize = 65_536;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_execution_capture.rs"]
mod tests;

#[cfg(feature = "test-faults")]
pub struct ShutdownExecutionCaptureProbe(ShutdownExecutionCapture);

#[cfg(feature = "test-faults")]
impl ShutdownExecutionCaptureProbe {
    pub fn new(
        service: &ProjectionConnectionService,
        fence: &crate::process_admission::ProcessAdmissionFenceTestProbe,
    ) -> Result<Self, String> {
        service
            .begin_shutdown_execution_capture(fence.fence())
            .map(Self)
            .map_err(|error| error.to_string())
    }

    pub fn refresh(&mut self, service: &ProjectionConnectionService) -> Result<(), String> {
        self.0
            .refresh(service, &ProjectionCancellationToken::new())
            .map_err(|error| error.to_string())
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.0.ordinary.len(), self.0.compactions.len())
    }

    pub fn ordinary(
        &self,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
    ) -> Option<crate::cas_projection::test_faults::TerminalCompletionProbe> {
        self.0
            .ordinary
            .iter()
            .find(|captured| captured.observer.thread_id() == thread && captured.turn_id() == turn)
            .map(|captured| {
                crate::cas_projection::test_faults::TerminalCompletionProbe(
                    captured.observer.clone(),
                )
            })
    }

    pub fn compaction_settled(
        &self,
        service: &ProjectionConnectionService,
        operation: CompactionOperationId,
    ) -> Result<Option<bool>, String> {
        self.0
            .compactions
            .get(&operation)
            .map(|captured| captured.is_settled(service))
            .transpose()
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShutdownExecutionCaptureError {
    #[error(transparent)]
    Process(#[from] ProcessAdmissionError),
    #[error(transparent)]
    Work(#[from] ProcessWorkError),
    #[error(transparent)]
    Compaction(#[from] CompactionWorkError),
    #[error(transparent)]
    Durable(#[from] SyndicReadError),
}

pub(crate) struct ShutdownExecutionCapture {
    service_generation: ProjectionServiceGeneration,
    fence: ProcessAdmissionFence,
    ordinary_limit: usize,
    compaction_limit: usize,
    ordinary: Vec<ShutdownTerminalCompletion>,
    compactions: BTreeMap<CompactionOperationId, ShutdownCompactionObligation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownCompactionObligation {
    service_generation: ProjectionServiceGeneration,
    operation: CompactionOperationWorkFact,
}

impl ProjectionConnectionService {
    pub(crate) fn begin_shutdown_execution_capture(
        &self,
        fence: &ProcessAdmissionFence,
    ) -> Result<ShutdownExecutionCapture, ShutdownExecutionCaptureError> {
        validate_service(self)?;
        validate_fence(self, fence)?;
        let compaction_limit = self
            .context_compaction
            .as_ref()
            .ok_or(ProcessWorkError::Closed)?
            .shutdown_obligation_capacity();
        let ordinary_limit = usize::from(self.config.worker_capacity());
        ordinary_limit
            .checked_add(compaction_limit)
            .and_then(|count| count.checked_mul(CAPTURE_PAGE_BYTES))
            .ok_or(ProcessWorkError::CountOverflow)?;
        Ok(ShutdownExecutionCapture {
            service_generation: self.service_generation,
            fence: fence.clone(),
            ordinary_limit,
            compaction_limit,
            ordinary: Vec::new(),
            compactions: BTreeMap::new(),
        })
    }
}

impl ShutdownExecutionCapture {
    pub(crate) fn ordinary(
        &self,
    ) -> impl Iterator<Item = (SyndicThreadId, &ShutdownTerminalCompletion)> {
        self.ordinary
            .iter()
            .map(|completion| (completion.observer.thread_id(), completion))
    }

    pub(crate) fn ordinary_completion(
        &self,
        service: &ProjectionConnectionService,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
    ) -> Result<Option<crate::cas_projection::ordinary::TerminalHistoryCompletion>, ProcessWorkError>
    {
        if service.service_generation != self.service_generation {
            return Err(ProcessWorkError::ForeignSources);
        }
        for captured in &self.ordinary {
            if captured.observer.thread_id() == thread
                && captured.turn_id() == turn
                && let Some(completion) = captured.completion(service, thread, turn)?
            {
                return Ok(Some(completion));
            }
        }
        Ok(None)
    }

    pub(crate) fn compactions(&self) -> impl Iterator<Item = &ShutdownCompactionObligation> {
        self.compactions.values()
    }

    pub(crate) fn refresh(
        &mut self,
        service: &ProjectionConnectionService,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), ShutdownExecutionCaptureError> {
        self.refresh_with_confirmation(service, cancellation, || {})
    }

    fn refresh_with_confirmation(
        &mut self,
        service: &ProjectionConnectionService,
        cancellation: &ProjectionCancellationToken,
        before_confirmation: impl FnOnce(),
    ) -> Result<(), ShutdownExecutionCaptureError> {
        check_cancelled(cancellation)?;
        if service.service_generation != self.service_generation {
            return Err(ProcessWorkError::ForeignSources.into());
        }
        validate_service(service)?;
        validate_fence(service, &self.fence)?;
        let command = service
            .live_home_command()
            .map_err(|_| ProcessWorkError::Closed)?;
        let durable = service
            .storage
            .revision(command.home())
            .map_err(SyndicReadError::from)?;
        let flights = FlightRegistry::work_revision().map_err(ProcessWorkError::from)?;
        let controls = service.compaction_work_revision()?;
        let ordinary = FlightRegistry::terminal_completions(
            service.home_id,
            service.home_generation,
            service.service_generation,
            flights,
            self.ordinary_limit,
        )?;
        let limit = SyndicPointReadLimit::new(CAPTURE_PAGE_BYTES)
            .expect("fixed execution capture point limit");
        let mut compactions = BTreeMap::new();
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = service.compaction_work_page(
                &controls,
                cursor.as_ref(),
                CompactionWorkPageLimits::new(256, CAPTURE_PAGE_BYTES)?,
            )?;
            for row in page.records() {
                let Some(fact) = row
                    .compaction
                    .as_ref()
                    .and_then(|fact| fact.operation.as_ref())
                else {
                    continue;
                };
                let Some(operation) = service.storage.compaction_operation(
                    command.home(),
                    fact.operation_id,
                    limit,
                )?
                else {
                    continue;
                };
                if row.thread_id != fact.target.thread_id()
                    || !matches_operation(service, fact, &operation)
                {
                    return Err(ProcessWorkError::ForeignSources.into());
                }
                if compactions.len() == self.compaction_limit
                    && !compactions.contains_key(&fact.operation_id)
                {
                    return Err(ProcessWorkError::SourceBoundExceeded.into());
                }
                let captured = ShutdownCompactionObligation {
                    service_generation: self.service_generation,
                    operation: fact.clone(),
                };
                if compactions
                    .insert(fact.operation_id, captured.clone())
                    .is_some_and(|previous| previous != captured)
                {
                    return Err(ProcessWorkError::ForeignSources.into());
                }
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        before_confirmation();
        service.validate_compaction_work_revision(&controls)?;
        if FlightRegistry::work_revision().map_err(ProcessWorkError::from)? != flights
            || service
                .storage
                .revision(command.home())
                .map_err(SyndicReadError::from)?
                != durable
        {
            return Err(ProcessWorkError::StaleRevision.into());
        }
        validate_service(service)?;
        validate_fence(service, &self.fence)?;
        check_cancelled(cancellation)?;
        let new_ordinary = ordinary
            .iter()
            .filter(|observer| {
                !self
                    .ordinary
                    .iter()
                    .any(|captured| captured.observer == **observer)
            })
            .count();
        let new_compactions = compactions
            .keys()
            .filter(|id| !self.compactions.contains_key(id))
            .count();
        if new_ordinary > self.ordinary_limit - self.ordinary.len()
            || new_compactions > self.compaction_limit - self.compactions.len()
        {
            return Err(ProcessWorkError::SourceBoundExceeded.into());
        }
        for (id, captured) in &compactions {
            if self
                .compactions
                .get(id)
                .is_some_and(|previous| previous != captured)
            {
                return Err(ProcessWorkError::ForeignSources.into());
            }
        }
        for observer in ordinary {
            if !self
                .ordinary
                .iter()
                .any(|captured| captured.observer == observer)
            {
                self.ordinary.push(ShutdownTerminalCompletion {
                    service_generation: self.service_generation,
                    observer,
                });
            }
        }
        self.compactions.extend(compactions);
        Ok(())
    }
}

impl ShutdownCompactionObligation {
    pub(crate) fn operation(&self) -> &CompactionOperationWorkFact {
        &self.operation
    }

    pub(crate) fn is_settled(
        &self,
        service: &ProjectionConnectionService,
    ) -> Result<bool, ShutdownExecutionCaptureError> {
        if service.service_generation != self.service_generation {
            return Err(ProcessWorkError::ForeignSources.into());
        }
        validate_service(service)?;
        let command = service
            .live_home_command()
            .map_err(|_| ProcessWorkError::Closed)?;
        let case = service.storage.compaction_recovery_read(
            command.home(),
            self.operation.operation_id,
            SyndicPointReadLimit::new(CAPTURE_PAGE_BYTES)
                .expect("fixed execution capture point limit"),
        )?;
        let settled = match case {
            Some(CompactionRecoveryCase::Settled(operation)) => {
                if !matches_operation(service, &self.operation, &operation) {
                    return Err(ProcessWorkError::ForeignSources.into());
                }
                true
            }
            _ => false,
        };
        validate_service(service)?;
        Ok(settled)
    }
}

fn matches_operation(
    service: &ProjectionConnectionService,
    fact: &CompactionOperationWorkFact,
    operation: &CompactionOperationRecord,
) -> bool {
    operation.id() == fact.operation_id
        && operation.home_id() == service.home_id
        && operation.attempt() == fact.attempt
        && operation.target() == &fact.target
}

fn validate_service(service: &ProjectionConnectionService) -> Result<(), ProcessWorkError> {
    let command = service
        .live_home_command()
        .map_err(|_| ProcessWorkError::Closed)?;
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home())?;
    if coordinator.home_id() != service.home_id
        || coordinator.home_generation() != service.home_generation
    {
        return Err(ProcessWorkError::ForeignSources);
    }
    Ok(())
}

fn validate_fence(
    service: &ProjectionConnectionService,
    fence: &ProcessAdmissionFence,
) -> Result<(), ProcessAdmissionError> {
    match service
        .command_authorizer
        .validate_process_settlement_fence(fence)
    {
        Ok(()) | Err(ProcessAdmissionError::Unsettled) => Ok(()),
        Err(error) => Err(error),
    }
}
