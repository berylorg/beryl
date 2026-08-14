use std::{
    fmt,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::ManagedBackendClientConnector;

use super::{
    composer_image_labels::ComposerImageLabelObservations,
    transcript_history::{TranscriptHistoryBackend, load_thread_history_page},
};

const COMPOSER_IMAGE_LABEL_SCAN_MAX_PAGES: usize = 512;

pub(super) enum ComposerImageLabelScanUpdate {
    Finished(ComposerImageLabelScanOutcome),
}

pub(super) enum ComposerImageLabelScanOutcome {
    Completed {
        thread_id: String,
        observations: ComposerImageLabelObservations,
    },
    Failed {
        thread_id: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComposerImageLabelScanResult {
    pub(crate) observations: ComposerImageLabelObservations,
    pub(crate) pages_scanned: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ComposerImageLabelScanError<E> {
    Backend(E),
    PageLimitExceeded { page_limit: usize },
}

pub(super) fn spawn_composer_image_label_scan_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    timeout: Duration,
) -> Receiver<ComposerImageLabelScanUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_composer_image_label_scan_worker(connector, thread_id, timeout, sender)
    });
    receiver
}

pub(crate) fn scan_composer_image_labels<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    scan_composer_image_labels_with_page_limit(
        backend,
        thread_id,
        timeout,
        COMPOSER_IMAGE_LABEL_SCAN_MAX_PAGES,
    )
}

pub(crate) fn scan_composer_image_labels_with_page_limit<B>(
    backend: &mut B,
    thread_id: &str,
    timeout: Duration,
    page_limit: usize,
) -> Result<ComposerImageLabelScanResult, ComposerImageLabelScanError<B::Error>>
where
    B: TranscriptHistoryBackend,
{
    let mut observations = ComposerImageLabelObservations::default();
    let mut pages_scanned = 0usize;
    let mut cursor: Option<String> = None;

    loop {
        if pages_scanned >= page_limit {
            return Err(ComposerImageLabelScanError::PageLimitExceeded { page_limit });
        }

        let page = load_thread_history_page(backend, thread_id, cursor.as_deref(), timeout)
            .map_err(ComposerImageLabelScanError::Backend)?;
        pages_scanned = pages_scanned.saturating_add(1);
        observations.observe_turns(&page.turns);
        cursor = page.older_cursor;

        if cursor.is_none() {
            return Ok(ComposerImageLabelScanResult {
                observations,
                pages_scanned,
            });
        }
    }
}

fn run_composer_image_label_scan_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
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

    let outcome = match scan_composer_image_labels(&mut session, &thread_id, timeout) {
        Ok(result) => ComposerImageLabelScanOutcome::Completed {
            thread_id,
            observations: result.observations,
        },
        Err(error) => ComposerImageLabelScanOutcome::Failed {
            thread_id,
            message: format!("Beryl could not scan thread image labels: {error}"),
        },
    };
    let _ = sender.send(ComposerImageLabelScanUpdate::Finished(outcome));
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
        }
    }
}
