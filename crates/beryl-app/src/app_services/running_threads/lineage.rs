use super::*;
use crate::thread_lineage::{LineageBreadcrumb, LineagePage, LineagePageRequest};

impl PublishedRunningThreadsReader {
    pub(crate) fn lineage_head_current(
        &self,
        selected: crate::main_window::MainWindowComposerSelectionIdentity,
        expected: &syndic_storage::ThreadLineageHead,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<RunningThreadsObservation, String> {
        let observed = self.observe()?;
        let (home, state, syndic) = self.activation_sources().ok_or("Lineage source retired")?;
        let head = syndic
            .thread_lineage_head(
                &home,
                selected.claim().thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65_536)
                    .map_err(|_| "Lineage point limit is invalid")?,
            )
            .map_err(|_| "Selected lineage is unavailable")?;
        let claim = state
            .session()
            .window_claim_catalog_source(&home, selected.window_id())
            .map_err(|_| "Selected lineage claim is unavailable")?
            .claim()
            .ok_or("Selected lineage claim is missing")?;
        if cancellation.is_cancelled()
            || head.as_ref() != Some(expected)
            || claim.thread_id() != selected.claim().thread_id()
            || claim.revision() != selected.claim().revision()
            || claim.generation() != selected.claim().generation()
            || claim.state() != beryl_state::ThreadClaimState::Active
        {
            return Err("Selected lineage changed".into());
        }
        self.elect(&observed, || ())?;
        Ok(observed)
    }
    pub(crate) fn lineage_page(
        &self,
        selected: crate::main_window::MainWindowComposerSelectionIdentity,
        head: &syndic_storage::ThreadLineageHead,
        request: LineagePageRequest,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(RunningThreadsObservation, LineagePage), String> {
        let observed = self.observe()?;
        let (home, state, syndic) = self.activation_sources().ok_or("Lineage source retired")?;
        if cancellation.is_cancelled() || head.leaf_thread_id() != selected.claim().thread_id() {
            return Err("Lineage selection changed".into());
        }
        let claim = state
            .session()
            .window_claim_catalog_source(&home, selected.window_id())
            .map_err(|_| "Lineage selected claim is unavailable")?
            .claim()
            .ok_or("Lineage selected claim is missing")?;
        if claim.thread_id() != selected.claim().thread_id()
            || claim.generation() != selected.claim().generation()
            || claim.revision() != selected.claim().revision()
            || claim.state() != beryl_state::ThreadClaimState::Active
        {
            return Err("Lineage selected claim changed".into());
        }
        let cursor = head
            .cursor_at(request.start)
            .map_err(|_| "Lineage position is invalid")?
            .ok_or("Lineage page starts past its last parent")?;
        let max_items = usize::try_from(request.end.saturating_sub(request.start))
            .unwrap_or(32)
            .min(32);
        if max_items == 0 {
            return Err("Lineage page is empty".into());
        }
        let source = syndic
            .thread_lineage_page(
                &home,
                head,
                cursor,
                beryl_home_store::CursorReadLimits::new(max_items, 65_536)
                    .map_err(|_| "Lineage page limit is invalid")?,
            )
            .map_err(|_| "Canonical lineage is unavailable")?;
        let mut rows = Vec::with_capacity(source.records().len());
        let mut payload = 0_usize;
        for entry in source.records() {
            if cancellation.is_cancelled() {
                return Err("Lineage read cancelled".into());
            }
            let row = compact_breadcrumb(&home, &state, &syndic, selected.window_id(), *entry);
            payload = payload
                .checked_add(row.presentation_bytes())
                .ok_or("Lineage page capacity exceeded")?;
            if payload > crate::thread_lineage::LINEAGE_PAGE_BYTES {
                return Err("Lineage page capacity exceeded".into());
            }
            rows.push(row);
        }
        let next_ordinal = request
            .start
            .checked_add(rows.len() as u64)
            .ok_or("Lineage position overflowed")?;
        if next_ordinal > request.end || next_ordinal > head.total_parent_count() || rows.is_empty()
        {
            return Err("Lineage page positions disagree".into());
        }
        if cancellation.is_cancelled() {
            return Err("Lineage read cancelled".into());
        }
        self.elect(&observed, || ())?;
        Ok((
            observed,
            LineagePage {
                request,
                rows,
                next_ordinal,
            },
        ))
    }
}

fn compact_breadcrumb(
    home: &beryl_home_store::HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    window: beryl_model::WindowId,
    entry: syndic_storage::ThreadLineageEntry,
) -> LineageBreadcrumb {
    let thread = entry.thread_id();
    let unknown = |reason: &str| LineageBreadcrumb {
        thread,
        title: "Unknown thread".into(),
        reason: Some(reason.into()),
    };
    let summary = match syndic.authenticate_thread_catalog_summary(home, thread) {
        Ok(syndic_storage::FrozenThreadCatalogSummaryAuthentication::Current(summary)) => summary,
        Ok(syndic_storage::FrozenThreadCatalogSummaryAuthentication::ThreadMissing) => {
            return unknown("This parent thread is missing.");
        }
        Ok(syndic_storage::FrozenThreadCatalogSummaryAuthentication::RebuildRequired(_)) => {
            return unknown("This parent's title metadata needs rebuilding.");
        }
        Err(_) => return unknown("This parent's title metadata is unavailable."),
    };
    if summary.lineage_depth() != entry.depth()
        || summary.lineage_digest() != entry.lineage_digest()
    {
        return unknown("This parent's lineage metadata changed.");
    }
    let title = summary
        .title()
        .map_or("Untitled", |title| title.text())
        .to_owned();
    let reason = (|| -> Result<Option<String>, &'static str> {
        let claim = state
            .session()
            .thread_claim_catalog_source(home, thread)
            .map_err(|_| "This parent's claim metadata is unavailable.")?
            .claim();
        if let Some(claim) = claim {
            return Ok(Some(
                if claim.window_id() == window {
                    "This parent is already selected in this window."
                } else {
                    "This parent is open in another window."
                }
                .into(),
            ));
        }
        let execution = state
            .runtime_roots()
            .catalog_source(
                home,
                summary.execution().runtime_id(),
                summary.execution().root_id(),
            )
            .map_err(|_| "This parent's runtime or root metadata is unavailable.")?;
        crate::catalog_projection::validate_execution_binding(&summary, &execution)
            .map_err(|_| "This parent's execution binding is unavailable.")?;
        if execution.runtime().availability().availability() != beryl_model::Availability::Available
        {
            return Ok(Some("This parent's runtime is unavailable.".into()));
        }
        if execution.root().availability().availability() != beryl_model::Availability::Available {
            return Ok(Some("This parent's root is unavailable.".into()));
        }
        if matches!(
            summary.archive(),
            syndic_storage::ThreadArchiveState::BranchDiscussionArchived { .. }
        ) {
            return Ok(Some("This parent discussion is archived.".into()));
        }
        Ok(None)
    })()
    .unwrap_or_else(|reason| Some(reason.into()));
    LineageBreadcrumb {
        thread,
        title,
        reason: reason.map(|reason| reason.chars().take(512).collect()),
    }
}
