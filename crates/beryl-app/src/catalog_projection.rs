use beryl_home_store::{CommandBuildError, HomeCommand, HomeStore, ReadError};
use beryl_model::SyndicThreadId;
use beryl_state::{
    BerylState, CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimKind,
    CatalogClaimSummary, CatalogExecutionSummary, CatalogFacts, CatalogFreshness,
    CatalogLineageSummary, CatalogPointReadLimit, CatalogReadError, CatalogResolvedTitle,
    CatalogRowExpectation, CatalogSourceRevisions, CatalogValueError, RuntimeRootCatalogSource,
    RuntimeRootCatalogSourceError, ThreadClaimCatalogSource, ThreadClaimCatalogSourceError,
    ThreadClaimState, UnixMillis,
};
use syndic_storage::{
    ExactThreadCatalogSummary, PreparedThreadCatalogSummaryReplacement, SyndicReadError,
    SyndicStorage, ThreadArchiveState, ThreadCatalogSummaryPreparation, ThreadCatalogSummaryRecord,
    ThreadCatalogTitleSource, ThreadLineageDepth,
};

/// Stable outcome of preparing one explicit compact thread-catalog rebuild.
pub enum ThreadCatalogProjectionPreparation {
    /// The named Syndic thread does not exist in the stable source revision.
    ThreadMissing,
    /// Syndic and Beryl already agree at one stable home revision.
    ExactCurrent,
    /// One all-or-nothing source-fenced publication is ready for execution.
    Publish(HomeCommand),
}

impl ThreadCatalogProjectionPreparation {
    /// Returns the publication command, when any durable change is required.
    #[must_use]
    pub fn into_command(self) -> Option<HomeCommand> {
        match self {
            Self::Publish(command) => Some(command),
            Self::ThreadMissing | Self::ExactCurrent => None,
        }
    }
}

/// Failure while joining exact Syndic and Beryl sources into one catalog command.
#[derive(Debug, thiserror::Error)]
pub enum CatalogProjectionBuildError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Command(#[from] CommandBuildError),
    #[error(transparent)]
    SyndicRead(#[from] SyndicReadError),
    #[error(transparent)]
    CatalogRead(#[from] CatalogReadError),
    #[error(transparent)]
    RuntimeRootSource(#[from] RuntimeRootCatalogSourceError),
    #[error(transparent)]
    ThreadClaimSource(#[from] ThreadClaimCatalogSourceError),
    #[error(transparent)]
    CatalogValue(#[from] CatalogValueError),
    #[error("Syndic execution binding disagrees with Beryl runtime/root authority")]
    ExecutionBindingMismatch,
    #[error("Syndic compact lineage facts are structurally inconsistent")]
    LineageMismatch,
    #[error("thread claim source names a different Syndic thread")]
    ThreadClaimMismatch,
    #[error("the Beryl home changed while the catalog join was prepared")]
    ConcurrentPreparation,
}

enum SyndicPlan {
    Validate(ExactThreadCatalogSummary),
    Rebuild(PreparedThreadCatalogSummaryReplacement),
}

impl SyndicPlan {
    const fn is_validation(&self) -> bool {
        matches!(self, Self::Validate(_))
    }
}

/// Prepares one explicit non-GUI catalog rebuild through a stable cross-domain source fence.
pub fn prepare_thread_catalog_projection(
    store: &HomeStore,
    syndic: &SyndicStorage,
    state: &BerylState,
    thread_id: SyndicThreadId,
) -> Result<ThreadCatalogProjectionPreparation, CatalogProjectionBuildError> {
    let home_revision = store.home_revision()?;
    let Some(prepared_summary) = syndic.prepare_thread_catalog_summary(store, thread_id)? else {
        return if store.home_revision()? == home_revision {
            Ok(ThreadCatalogProjectionPreparation::ThreadMissing)
        } else {
            Err(CatalogProjectionBuildError::ConcurrentPreparation)
        };
    };
    let (summary, syndic_plan) = match prepared_summary {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => {
            (exact.summary().clone(), SyndicPlan::Validate(exact))
        }
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => (
            prepared.replacement().clone(),
            SyndicPlan::Rebuild(prepared),
        ),
    };

    let runtime_revision = state.runtime_roots().revision(store)?;
    let runtime_source = state.runtime_roots().catalog_source(
        store,
        summary.execution().runtime_id(),
        summary.execution().root_id(),
    )?;
    validate_execution_binding(&summary, &runtime_source)?;

    let session_revision = state.session().revision(store)?;
    let claim_source = state
        .session()
        .thread_claim_catalog_source(store, thread_id)?;
    let (claim, claim_revision) = project_claim(thread_id, claim_source)?;

    let facts = project_facts(&summary, &runtime_source, claim)?;
    let sources = CatalogSourceRevisions::new(
        summary.revision(),
        runtime_source.runtime().revision(),
        runtime_source.root().revision(),
        claim_revision,
    );

    let catalog_revision = state.catalog().revision(store)?;
    let current = state
        .catalog()
        .row(store, thread_id, CatalogPointReadLimit::schema_maximum())?;
    if store.home_revision()? != home_revision {
        return Err(CatalogProjectionBuildError::ConcurrentPreparation);
    }
    if syndic_plan.is_validation()
        && current.as_ref().is_some_and(|row| {
            row.freshness() == CatalogFreshness::Current
                && row.sources() == sources
                && row.facts() == &facts
        })
    {
        return Ok(ThreadCatalogProjectionPreparation::ExactCurrent);
    }

    let expectation = current
        .as_ref()
        .map_or(CatalogRowExpectation::Missing, |row| {
            CatalogRowExpectation::Revision(row.revision())
        });
    let publication = beryl_state::PublishCatalogRow::new(thread_id, expectation, sources, facts)?;
    let mut command = HomeCommand::new(home_revision);
    command.add(state.catalog().publish(catalog_revision, publication))?;
    match syndic_plan {
        SyndicPlan::Validate(exact) => {
            command.add_validation(syndic.validate_current_thread_catalog_summary(exact))?;
        }
        SyndicPlan::Rebuild(prepared) => {
            command.add(syndic.rebuild_thread_catalog_summary(prepared))?;
        }
    }
    command.add_validation(
        state
            .runtime_roots()
            .validate_catalog_source(runtime_revision, runtime_source),
    )?;
    command.add_validation(
        state
            .session()
            .validate_thread_claim_catalog_source(session_revision, claim_source),
    )?;
    Ok(ThreadCatalogProjectionPreparation::Publish(command))
}

fn validate_execution_binding(
    summary: &ThreadCatalogSummaryRecord,
    source: &RuntimeRootCatalogSource,
) -> Result<(), CatalogProjectionBuildError> {
    let binding = summary.execution();
    if binding.runtime_id() != source.runtime().runtime_id()
        || binding.root_id() != source.root().root_id()
        || source.root().runtime_id() != source.runtime().runtime_id()
        || binding.root_path() != source.root().canonical_path()
    {
        return Err(CatalogProjectionBuildError::ExecutionBindingMismatch);
    }
    Ok(())
}

fn project_claim(
    thread_id: SyndicThreadId,
    source: ThreadClaimCatalogSource,
) -> Result<(CatalogClaimSummary, Option<beryl_model::ClaimRevision>), CatalogProjectionBuildError>
{
    if source.thread_id() != thread_id {
        return Err(CatalogProjectionBuildError::ThreadClaimMismatch);
    }
    let Some(claim) = source.claim() else {
        return Ok((CatalogClaimSummary::Unclaimed, None));
    };
    if claim.thread_id() != thread_id {
        return Err(CatalogProjectionBuildError::ThreadClaimMismatch);
    }
    let kind = match claim.state() {
        ThreadClaimState::Active => CatalogClaimKind::Active,
        ThreadClaimState::Restoring => CatalogClaimKind::Restoring,
    };
    Ok((
        CatalogClaimSummary::claimed(claim.window_id(), kind),
        Some(claim.revision()),
    ))
}

fn project_facts(
    summary: &ThreadCatalogSummaryRecord,
    source: &RuntimeRootCatalogSource,
    claim: CatalogClaimSummary,
) -> Result<CatalogFacts, CatalogProjectionBuildError> {
    let title = match summary.title() {
        None => CatalogResolvedTitle::absent(),
        Some(title) => match title.source() {
            ThreadCatalogTitleSource::Generated => CatalogResolvedTitle::generated(title.text())?,
            ThreadCatalogTitleSource::HistoryDerived => {
                CatalogResolvedTitle::history_derived(title.text())?
            }
        },
    };
    let archive = match summary.archive() {
        ThreadArchiveState::Ordinary => CatalogArchiveSummary::Ordinary,
        ThreadArchiveState::BranchDiscussionOpen => CatalogArchiveSummary::BranchDiscussionOpen,
        ThreadArchiveState::BranchDiscussionArchived { .. } => {
            CatalogArchiveSummary::BranchDiscussionArchived
        }
    };
    let lineage = match (summary.parent_thread_id(), summary.lineage_depth()) {
        (None, depth) if depth == ThreadLineageDepth::FIRST => CatalogLineageSummary::TopLevel,
        (Some(parent), depth) if depth != ThreadLineageDepth::FIRST => {
            CatalogLineageSummary::descendant(parent, depth.get(), summary.lineage_digest())?
        }
        (None, _) | (Some(_), _) => return Err(CatalogProjectionBuildError::LineageMismatch),
    };
    let runtime = source.runtime();
    let root = source.root();
    let execution = CatalogExecutionSummary::new(
        runtime.runtime_id(),
        root.root_id(),
        runtime.environment_label(),
        runtime.canonical_executable().clone(),
        root.display_path().clone(),
        CatalogAvailabilitySummary::new(
            runtime.availability().availability(),
            root.availability().availability(),
        ),
    )?;
    CatalogFacts::new(
        title,
        execution,
        archive,
        UnixMillis::new(summary.last_activity_at().unix_millis()),
        summary.complete(),
        claim,
        lineage,
    )
    .map_err(Into::into)
}
