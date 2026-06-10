use std::{
    fmt,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, SortDirection, ThreadTurnsListOptions, TurnInfo, TurnItemsView,
};

use super::{
    composer_image_labels::{
        ComposerImageLabelHistoryFrontier, ComposerImageLabelHistoryFrontierBuilder,
        ComposerImageLabelObservations,
    },
    transcript_history::{THREAD_HISTORY_PAGE_LIMIT, TranscriptHistoryBackend},
};

const COMPOSER_IMAGE_LABEL_SCAN_MAX_PAGES: usize = 512;

pub(super) enum ComposerImageLabelScanUpdate {
    Finished(ComposerImageLabelScanOutcome),
}

#[allow(dead_code)]
pub(super) enum ComposerImageLabelValidationUpdate {
    Finished(ComposerImageLabelValidationWorkerOutcome),
}

pub(super) enum ComposerImageLabelScanOutcome {
    Completed {
        thread_id: String,
        observations: ComposerImageLabelObservations,
        frontier: ComposerImageLabelHistoryFrontier,
    },
    Failed {
        thread_id: String,
        message: String,
    },
}

#[allow(dead_code)]
pub(super) enum ComposerImageLabelValidationWorkerOutcome {
    Completed {
        thread_id: String,
        validation: ComposerImageLabelFrontierValidationResult,
    },
    Failed {
        thread_id: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerImageLabelScanResult {
    pub(crate) observations: ComposerImageLabelObservations,
    pub(crate) frontier: ComposerImageLabelHistoryFrontier,
    pub(crate) pages_scanned: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImageLabelScanPlan {
    FullCurrentHistory,
    AppendOnlySuffix {
        expected_appended_turn_count: usize,
        previous_newest_turn_id: Option<String>,
        frontier: ComposerImageLabelHistoryFrontier,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImageLabelScanError<E> {
    Backend(E),
    PageLimitExceeded {
        page_limit: usize,
    },
    AppendOnlyBoundaryMissing {
        previous_newest_turn_id: String,
    },
    AppendOnlySuffixChanged {
        expected_appended_turn_count: usize,
        observed_appended_turn_count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerImageLabelFrontierValidationResult {
    pub(crate) outcome: ComposerImageLabelFrontierValidationOutcome,
    pub(crate) pages_scanned: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImageLabelFrontierValidationOutcome {
    CacheValid {
        frontier: ComposerImageLabelHistoryFrontier,
    },
    AppendOnly {
        appended_turn_count: usize,
        previous_newest_turn_id: Option<String>,
        frontier: ComposerImageLabelHistoryFrontier,
    },
    UnknownMutation {
        frontier: ComposerImageLabelHistoryFrontier,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImageLabelFrontierValidationError<E> {
    Backend(E),
    PageLimitExceeded {
        page_limit: usize,
    },
    UnexpectedLoadedTurn {
        turn_id: String,
        items_view: TurnItemsView,
        item_count: usize,
    },
}

pub(super) fn spawn_composer_image_label_scan_worker_for_plan(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    plan: ComposerImageLabelScanPlan,
    timeout: Duration,
) -> Receiver<ComposerImageLabelScanUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_composer_image_label_scan_worker(connector, thread_id, plan, timeout, sender)
    });
    receiver
}

#[allow(dead_code)]
pub(super) fn spawn_composer_image_label_validation_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    cached_frontier: Option<ComposerImageLabelHistoryFrontier>,
    timeout: Duration,
) -> Receiver<ComposerImageLabelValidationUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_composer_image_label_validation_worker(
            connector,
            thread_id,
            cached_frontier,
            timeout,
            sender,
        )
    });
    receiver
}

#[allow(dead_code)]
pub(crate) fn scan_composer_image_labels<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    scan_composer_image_labels_for_plan(
        backend,
        thread_id,
        ComposerImageLabelScanPlan::FullCurrentHistory,
        timeout,
    )
}

#[allow(dead_code)]
pub(crate) fn scan_composer_image_labels_with_page_limit<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
    page_limit: usize,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    scan_composer_image_labels_for_plan_with_page_limit(
        backend,
        thread_id,
        ComposerImageLabelScanPlan::FullCurrentHistory,
        timeout,
        page_limit,
    )
}

pub(crate) fn scan_composer_image_labels_for_plan<B>(
    backend: &mut B,
    thread_id: &str,
    plan: ComposerImageLabelScanPlan,
    timeout: Duration,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    scan_composer_image_labels_for_plan_with_page_limit(
        backend,
        thread_id,
        plan,
        timeout,
        COMPOSER_IMAGE_LABEL_SCAN_MAX_PAGES,
    )
}

pub(crate) fn scan_composer_image_labels_for_plan_with_page_limit<B>(
    backend: &mut B,
    thread_id: &str,
    plan: ComposerImageLabelScanPlan,
    timeout: Duration,
    page_limit: usize,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    match plan {
        ComposerImageLabelScanPlan::FullCurrentHistory => {
            scan_full_composer_image_label_history(backend, thread_id, timeout, page_limit)
        }
        ComposerImageLabelScanPlan::AppendOnlySuffix {
            expected_appended_turn_count,
            previous_newest_turn_id,
            frontier,
        } => scan_append_only_composer_image_label_suffix(
            backend,
            thread_id,
            timeout,
            page_limit,
            expected_appended_turn_count,
            previous_newest_turn_id,
            frontier,
        ),
    }
}

fn scan_full_composer_image_label_history<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
    page_limit: usize,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    let mut observations = ComposerImageLabelObservations::default();
    let mut frontier = ComposerImageLabelHistoryFrontierBuilder::default();
    let mut pages_scanned = 0usize;
    let mut cursor: Option<String> = None;

    loop {
        if pages_scanned >= page_limit {
            return Err(ComposerImageLabelScanError::PageLimitExceeded { page_limit });
        }

        let options = composer_image_label_scan_page_options(cursor.as_deref());
        let page = backend
            .list_thread_turns(thread_id, &options, timeout)
            .map_err(ComposerImageLabelScanError::Backend)?;
        pages_scanned = pages_scanned.saturating_add(1);
        for turn in page.data {
            frontier.observe_turn_desc(&turn);
            observations.observe_turn(&turn);
        }
        cursor = page.next_cursor;

        if cursor.is_none() {
            return Ok(ComposerImageLabelScanResult {
                observations,
                frontier: frontier.finish(),
                pages_scanned,
            });
        }
    }
}

fn scan_append_only_composer_image_label_suffix<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
    page_limit: usize,
    expected_appended_turn_count: usize,
    previous_newest_turn_id: Option<String>,
    validated_frontier: ComposerImageLabelHistoryFrontier,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    let Some(previous_newest_turn_id) = previous_newest_turn_id else {
        return scan_full_composer_image_label_history(backend, thread_id, timeout, page_limit);
    };

    let mut observations = ComposerImageLabelObservations::default();
    let mut pages_scanned = 0usize;
    let mut cursor: Option<String> = None;
    let mut observed_appended_turn_count = 0usize;

    loop {
        if pages_scanned >= page_limit {
            return Err(ComposerImageLabelScanError::PageLimitExceeded { page_limit });
        }

        let options = composer_image_label_scan_page_options(cursor.as_deref());
        let page = backend
            .list_thread_turns(thread_id, &options, timeout)
            .map_err(ComposerImageLabelScanError::Backend)?;
        pages_scanned = pages_scanned.saturating_add(1);
        let next_cursor = page.next_cursor;

        for turn in page.data {
            if turn.id == previous_newest_turn_id {
                if observed_appended_turn_count != expected_appended_turn_count {
                    return Err(ComposerImageLabelScanError::AppendOnlySuffixChanged {
                        expected_appended_turn_count,
                        observed_appended_turn_count,
                    });
                }
                return Ok(ComposerImageLabelScanResult {
                    observations,
                    frontier: validated_frontier,
                    pages_scanned,
                });
            }

            observed_appended_turn_count = observed_appended_turn_count.saturating_add(1);
            if observed_appended_turn_count > expected_appended_turn_count {
                return Err(ComposerImageLabelScanError::AppendOnlySuffixChanged {
                    expected_appended_turn_count,
                    observed_appended_turn_count,
                });
            }
            observations.observe_turn(&turn);
        }

        cursor = next_cursor;
        if cursor.is_none() {
            return Err(ComposerImageLabelScanError::AppendOnlyBoundaryMissing {
                previous_newest_turn_id,
            });
        }
    }
}

pub(crate) fn validate_composer_image_label_frontier<B>(
    backend: &mut B,
    thread_id: &str,
    cached_frontier: Option<&ComposerImageLabelHistoryFrontier>,
    timeout: Duration,
) -> Result<
    ComposerImageLabelFrontierValidationResult,
    ComposerImageLabelFrontierValidationError<B::Error>,
>
where
    B: TranscriptHistoryBackend,
{
    validate_composer_image_label_frontier_with_page_limit(
        backend,
        thread_id,
        cached_frontier,
        timeout,
        COMPOSER_IMAGE_LABEL_SCAN_MAX_PAGES,
    )
}

pub(crate) fn validate_composer_image_label_frontier_with_page_limit<B>(
    backend: &mut B,
    thread_id: &str,
    cached_frontier: Option<&ComposerImageLabelHistoryFrontier>,
    timeout: Duration,
    page_limit: usize,
) -> Result<
    ComposerImageLabelFrontierValidationResult,
    ComposerImageLabelFrontierValidationError<B::Error>,
>
where
    B: TranscriptHistoryBackend,
{
    let mut validator = ComposerImageLabelFrontierValidator::new(cached_frontier);
    let mut pages_scanned = 0usize;
    let mut cursor: Option<String> = None;

    loop {
        if pages_scanned >= page_limit {
            return Err(
                ComposerImageLabelFrontierValidationError::PageLimitExceeded { page_limit },
            );
        }

        let options = composer_image_label_frontier_validation_page_options(cursor.as_deref());
        let page = backend
            .list_thread_turns(thread_id, &options, timeout)
            .map_err(ComposerImageLabelFrontierValidationError::Backend)?;
        pages_scanned = pages_scanned.saturating_add(1);
        ensure_validation_page_is_not_loaded(&page.data)?;
        validator.observe_turns_desc(&page.data);
        cursor = page.next_cursor;

        if cursor.is_none() {
            return Ok(ComposerImageLabelFrontierValidationResult {
                outcome: validator.finish(),
                pages_scanned,
            });
        }
    }
}

fn ensure_validation_page_is_not_loaded<E>(
    turns: &[TurnInfo],
) -> Result<(), ComposerImageLabelFrontierValidationError<E>> {
    for turn in turns {
        if turn.items_view != TurnItemsView::NotLoaded || !turn.items.is_empty() {
            return Err(
                ComposerImageLabelFrontierValidationError::UnexpectedLoadedTurn {
                    turn_id: turn.id.clone(),
                    items_view: turn.items_view,
                    item_count: turn.items.len(),
                },
            );
        }
    }
    Ok(())
}

pub(crate) fn composer_image_label_scan_page_options(
    cursor: Option<&str>,
) -> ThreadTurnsListOptions {
    let options = ThreadTurnsListOptions::page(THREAD_HISTORY_PAGE_LIMIT)
        .with_sort_direction(SortDirection::Desc)
        .with_items_view(TurnItemsView::Full);
    match cursor {
        Some(cursor) => options.with_cursor(cursor),
        None => options,
    }
}

pub(crate) fn composer_image_label_frontier_validation_page_options(
    cursor: Option<&str>,
) -> ThreadTurnsListOptions {
    let options = ThreadTurnsListOptions::page(THREAD_HISTORY_PAGE_LIMIT)
        .with_sort_direction(SortDirection::Desc)
        .with_items_view(TurnItemsView::NotLoaded);
    match cursor {
        Some(cursor) => options.with_cursor(cursor),
        None => options,
    }
}

struct ComposerImageLabelFrontierValidator<'a> {
    cached_frontier: Option<&'a ComposerImageLabelHistoryFrontier>,
    current_builder: ComposerImageLabelHistoryFrontierBuilder,
    cached_suffix_builder: Option<ComposerImageLabelHistoryFrontierBuilder>,
    prefix_turn_count: usize,
    found_cached_newest: bool,
}

impl<'a> ComposerImageLabelFrontierValidator<'a> {
    fn new(cached_frontier: Option<&'a ComposerImageLabelHistoryFrontier>) -> Self {
        Self {
            cached_frontier,
            current_builder: ComposerImageLabelHistoryFrontierBuilder::default(),
            cached_suffix_builder: None,
            prefix_turn_count: 0,
            found_cached_newest: false,
        }
    }

    fn observe_turns_desc(&mut self, turns: &[TurnInfo]) {
        for turn in turns {
            self.observe_turn_desc(turn);
        }
    }

    fn observe_turn_desc(&mut self, turn: &TurnInfo) {
        self.current_builder.observe_turn_desc(turn);

        let Some(cached_frontier) = self.cached_frontier else {
            return;
        };
        if cached_frontier.is_empty() {
            return;
        }

        if !self.found_cached_newest {
            if cached_frontier.newest_turn_id.as_deref() == Some(turn.id.as_str()) {
                self.found_cached_newest = true;
                self.cached_suffix_builder =
                    Some(ComposerImageLabelHistoryFrontierBuilder::default());
            } else {
                self.prefix_turn_count = self.prefix_turn_count.saturating_add(1);
            }
        }

        if let Some(builder) = &mut self.cached_suffix_builder {
            builder.observe_turn_desc(turn);
        }
    }

    fn finish(self) -> ComposerImageLabelFrontierValidationOutcome {
        let current_frontier = self.current_builder.finish();
        let Some(cached_frontier) = self.cached_frontier else {
            if current_frontier.is_empty() {
                return ComposerImageLabelFrontierValidationOutcome::CacheValid {
                    frontier: current_frontier,
                };
            }
            return ComposerImageLabelFrontierValidationOutcome::UnknownMutation {
                frontier: current_frontier,
            };
        };

        if cached_frontier.is_empty() {
            if current_frontier.is_empty() {
                return ComposerImageLabelFrontierValidationOutcome::CacheValid {
                    frontier: current_frontier,
                };
            }
            return ComposerImageLabelFrontierValidationOutcome::AppendOnly {
                appended_turn_count: current_frontier.scanned_turn_count,
                previous_newest_turn_id: None,
                frontier: current_frontier,
            };
        }

        let Some(cached_suffix_frontier) =
            self.cached_suffix_builder.map(|builder| builder.finish())
        else {
            return ComposerImageLabelFrontierValidationOutcome::UnknownMutation {
                frontier: current_frontier,
            };
        };

        let cached_suffix_matches = cached_suffix_frontier.scanned_turn_count
            == cached_frontier.scanned_turn_count
            && cached_suffix_frontier.turn_identity_digest == cached_frontier.turn_identity_digest;
        if !cached_suffix_matches {
            return ComposerImageLabelFrontierValidationOutcome::UnknownMutation {
                frontier: current_frontier,
            };
        }

        if self.prefix_turn_count == 0 {
            ComposerImageLabelFrontierValidationOutcome::CacheValid {
                frontier: current_frontier,
            }
        } else {
            ComposerImageLabelFrontierValidationOutcome::AppendOnly {
                appended_turn_count: self.prefix_turn_count,
                previous_newest_turn_id: cached_frontier.newest_turn_id.clone(),
                frontier: current_frontier,
            }
        }
    }
}

fn run_composer_image_label_scan_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    plan: ComposerImageLabelScanPlan,
    timeout: Duration,
    sender: mpsc::Sender<ComposerImageLabelScanUpdate>,
) {
    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(ComposerImageLabelScanUpdate::Finished(
                ComposerImageLabelScanOutcome::Failed {
                    thread_id,
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    let outcome = match scan_composer_image_labels_for_plan(&mut session, &thread_id, plan, timeout)
    {
        Ok(result) => ComposerImageLabelScanOutcome::Completed {
            thread_id,
            observations: result.observations,
            frontier: result.frontier,
        },
        Err(error) => ComposerImageLabelScanOutcome::Failed {
            thread_id,
            message: format!("Beryl could not scan thread image labels: {error}"),
        },
    };
    let _ = sender.send(ComposerImageLabelScanUpdate::Finished(outcome));
}

#[allow(dead_code)]
fn run_composer_image_label_validation_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    cached_frontier: Option<ComposerImageLabelHistoryFrontier>,
    timeout: Duration,
    sender: mpsc::Sender<ComposerImageLabelValidationUpdate>,
) {
    let mut session = match connector.connect_request_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            let _ = sender.send(ComposerImageLabelValidationUpdate::Finished(
                ComposerImageLabelValidationWorkerOutcome::Failed {
                    thread_id,
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                },
            ));
            return;
        }
    };

    let outcome = match validate_composer_image_label_frontier(
        &mut session,
        &thread_id,
        cached_frontier.as_ref(),
        timeout,
    ) {
        Ok(validation) => ComposerImageLabelValidationWorkerOutcome::Completed {
            thread_id,
            validation,
        },
        Err(error) => ComposerImageLabelValidationWorkerOutcome::Failed {
            thread_id,
            message: format!("Beryl could not validate thread image-label cache: {error}"),
        },
    };
    let _ = sender.send(ComposerImageLabelValidationUpdate::Finished(outcome));
}

impl<E> fmt::Display for ComposerImageLabelScanError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::PageLimitExceeded { page_limit } => write!(
                formatter,
                "thread history exceeded the image-label scan limit of {page_limit} pages"
            ),
            Self::AppendOnlyBoundaryMissing {
                previous_newest_turn_id,
            } => write!(
                formatter,
                "thread history changed before image-label suffix scan reached cached boundary turn {previous_newest_turn_id}"
            ),
            Self::AppendOnlySuffixChanged {
                expected_appended_turn_count,
                observed_appended_turn_count,
            } => write!(
                formatter,
                "thread history changed during image-label suffix scan; expected {expected_appended_turn_count} appended turns, observed {observed_appended_turn_count}"
            ),
        }
    }
}

impl<E> fmt::Display for ComposerImageLabelFrontierValidationError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::PageLimitExceeded { page_limit } => write!(
                formatter,
                "thread history exceeded the image-label validation limit of {page_limit} pages"
            ),
            Self::UnexpectedLoadedTurn {
                turn_id,
                items_view,
                item_count,
            } => write!(
                formatter,
                "image-label validation expected notLoaded turn {turn_id}, got {items_view:?} with {item_count} items"
            ),
        }
    }
}
