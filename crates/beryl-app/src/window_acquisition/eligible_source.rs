use super::*;
use crate::catalog_projection::{project_facts, validate_execution_binding};
use beryl_state::{CatalogFreshness, CatalogPointReadLimit};
use syndic_storage::ThreadCatalogSummaryPreparation;

#[derive(Debug, thiserror::Error)]
pub(super) enum AcquisitionSourceError {
    #[error(transparent)]
    Projection(#[from] CatalogProjectionBuildError),
    #[error(transparent)]
    Mutation(#[from] syndic_storage::SyndicMutationError),
    #[error("the acquired thread needs an exact catalog projection repair")]
    CatalogRepair(SyndicThreadId),
    #[error("the acquired thread disagrees with its original source")]
    Conflict,
}

impl RuntimeBackedWindowAcquisitionService {
    pub(super) fn inspect_natural_reused_source(
        &self,
        thread: SyndicThreadId,
        execution: &ExecutionBinding,
    ) -> Result<
        Option<(EligibleEmptyThreadCandidate, EligibleEmptyThreadOutcome)>,
        AcquisitionSourceError,
    > {
        let Some(candidate) = self
            .syndic
            .inspect_eligible_empty_thread(&self.store, thread, execution)
            .map_err(CatalogProjectionBuildError::from)?
        else {
            return Ok(None);
        };
        let source = self
            .syndic
            .prepare_thread_catalog_summary(&self.store, thread)
            .map_err(CatalogProjectionBuildError::from)?
            .ok_or(AcquisitionSourceError::Conflict)?;
        let ThreadCatalogSummaryPreparation::ExactCurrent(ref exact) = source else {
            return Err(AcquisitionSourceError::CatalogRepair(thread));
        };
        self.validate_reused_catalog(exact.summary())?;
        let outcome = self.syndic.prepare_eligible_empty_thread_outcome(
            &self.store,
            candidate.clone(),
            source,
        )?;
        Ok(Some((candidate, outcome)))
    }

    fn validate_reused_catalog(
        &self,
        summary: &ThreadCatalogSummaryRecord,
    ) -> Result<(), AcquisitionSourceError> {
        let thread = summary.thread_id();
        let runtime = self
            .state
            .runtime_roots()
            .catalog_source(
                &self.store,
                summary.execution().runtime_id(),
                summary.execution().root_id(),
            )
            .map_err(CatalogProjectionBuildError::from)?;
        validate_execution_binding(summary, &runtime)?;
        let claim = self
            .state
            .session()
            .thread_claim_catalog_source(&self.store, thread)
            .map_err(CatalogProjectionBuildError::from)?
            .claim()
            .ok_or(AcquisitionSourceError::Conflict)?;
        if claim.state() != beryl_state::ThreadClaimState::Active {
            return Err(AcquisitionSourceError::Conflict);
        }
        let facts = project_facts(
            summary,
            &runtime,
            CatalogClaimSummary::claimed(claim.window_id(), beryl_state::CatalogClaimKind::Active),
        )?;
        let sources = CatalogSourceRevisions::new(
            summary.revision(),
            runtime.runtime().revision(),
            runtime.root().revision(),
            Some(claim.revision()),
        );
        let row = self
            .state
            .catalog()
            .row(&self.store, thread, CatalogPointReadLimit::schema_maximum())
            .map_err(CatalogProjectionBuildError::from)?
            .ok_or(AcquisitionSourceError::CatalogRepair(thread))?;
        if row.freshness() != CatalogFreshness::Current
            || row.sources() != sources
            || row.facts() != &facts
        {
            return Err(AcquisitionSourceError::CatalogRepair(thread));
        }
        Ok(())
    }
}
