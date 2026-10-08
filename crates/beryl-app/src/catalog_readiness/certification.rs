use super::*;
use beryl_home_store::CursorReadLimits;
use beryl_state::{CatalogFreshness, CatalogPointReadLimit, CatalogSourceRevisions};
use syndic_storage::FrozenThreadCatalogSummaryAuthentication;

pub(super) enum Certification {
    Ready { threads: usize },
    Repair(beryl_model::SyndicThreadId),
}

pub(super) fn certify(
    home: &HomeStore,
    syndic: &SyndicStorage,
    state: &BerylState,
    frozen: &FrozenHomeRead,
    cancellation: &CommandCancellation,
) -> Result<Certification, CatalogSourceReadError> {
    let limits = CursorReadLimits::new(16, 16 * beryl_state::CATALOG_MAX_STORED_RECENCY_BYTES)
        .expect("fixed catalog page limits are positive");
    let mut after = None;
    let mut threads = 0usize;
    let mut repair = None;
    loop {
        cancelled(cancellation)?;
        let page = syndic.frozen_threads_page(home, frozen, after, limits)?;
        for thread in page.records() {
            cancelled(cancellation)?;
            let id = thread.id();
            if !authenticate(home, syndic, state, frozen, id)? {
                repair.get_or_insert(id);
            }
            threads = threads
                .checked_add(1)
                .ok_or(CatalogSourceReadError::CountExhausted)?;
        }
        if !page.has_more() {
            break;
        }
        after = Some(
            page.records()
                .last()
                .ok_or(CatalogSourceReadError::EmptyContinuation)?
                .id(),
        );
    }
    let mut primary_count = 0usize;
    after = None;
    loop {
        cancelled(cancellation)?;
        let page = state
            .catalog()
            .frozen_primary_page(home, frozen, after, limits)?;
        for row in page.records() {
            cancelled(cancellation)?;
            if !authenticate(home, syndic, state, frozen, row.thread_id())? {
                repair.get_or_insert(row.thread_id());
            }
            primary_count = primary_count
                .checked_add(1)
                .ok_or(CatalogSourceReadError::CountExhausted)?;
        }
        if !page.has_more() {
            break;
        }
        after = Some(
            page.records()
                .last()
                .ok_or(CatalogSourceReadError::EmptyContinuation)?
                .thread_id(),
        );
    }
    let mut reverse_count = 0usize;
    let mut reverse_after = None;
    loop {
        cancelled(cancellation)?;
        let page = state
            .catalog()
            .frozen_recency_page(home, frozen, reverse_after, limits)?;
        reverse_count = reverse_count
            .checked_add(page.rows().len())
            .ok_or(CatalogSourceReadError::CountExhausted)?;
        if !page.has_more() {
            break;
        }
        reverse_after = Some(
            page.next_after()
                .ok_or(CatalogSourceReadError::EmptyContinuation)?,
        );
    }
    if primary_count != reverse_count || threads < primary_count {
        return Err(CatalogSourceReadError::CoverageMismatch);
    }
    if let Some(thread) = repair {
        return Ok(Certification::Repair(thread));
    }
    if threads != primary_count {
        return Err(CatalogSourceReadError::CoverageMismatch);
    }
    Ok(Certification::Ready { threads })
}

fn authenticate(
    home: &HomeStore,
    syndic: &SyndicStorage,
    state: &BerylState,
    frozen: &FrozenHomeRead,
    id: beryl_model::SyndicThreadId,
) -> Result<bool, CatalogSourceReadError> {
    let summary = match syndic.authenticate_frozen_thread_catalog_summary(home, frozen, id)? {
        FrozenThreadCatalogSummaryAuthentication::Current(summary) => summary,
        FrozenThreadCatalogSummaryAuthentication::RebuildRequired(_) => return Ok(false),
        FrozenThreadCatalogSummaryAuthentication::ThreadMissing => {
            return Err(CatalogSourceReadError::CanonicalSourceMissing);
        }
    };
    let runtime = state.runtime_roots().frozen_catalog_source(
        home,
        frozen,
        summary.execution().runtime_id(),
        summary.execution().root_id(),
    )?;
    crate::catalog_projection::validate_execution_binding(&summary, &runtime)?;
    let claim = state
        .session()
        .frozen_thread_claim_catalog_source(home, frozen, id)?;
    let (claim, revision) = crate::catalog_projection::project_claim(id, claim)?;
    let facts = crate::catalog_projection::project_facts(&summary, &runtime, claim)?;
    let sources = CatalogSourceRevisions::new(
        summary.revision(),
        runtime.runtime().revision(),
        runtime.root().revision(),
        revision,
    );
    Ok(state
        .catalog()
        .frozen_row(home, frozen, id, CatalogPointReadLimit::schema_maximum())?
        .is_some_and(|row| {
            row.freshness() == CatalogFreshness::Current
                && row.sources() == sources
                && row.facts() == &facts
        }))
}

fn cancelled(cancellation: &CommandCancellation) -> Result<(), CatalogSourceReadError> {
    if cancellation.is_cancelled() {
        Err(CatalogSourceReadError::Cancelled)
    } else {
        Ok(())
    }
}
