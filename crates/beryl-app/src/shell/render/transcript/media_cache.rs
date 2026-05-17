use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use beryl_backend::ManagedBackendClientConnector;
use beryl_model::workspace::WorkspaceId;
use gpui::{App, AsyncApp, Entity, Image};
use tracing::debug;

use crate::diagnostic_dynamic_tools::{
    MediaDiagnosticEvent, MediaDiagnosticLog, VisibleMediaDiagnostics,
};
use crate::shell::transcript_media::{
    TranscriptMediaCache, TranscriptMediaCacheKey, TranscriptMediaFileReader,
    TranscriptMediaLoadOutcome, TranscriptMediaLoadRequest, TranscriptMediaSource,
};

use super::{
    TranscriptFrameProfile, TranscriptImageMenuRenderState, TranscriptMediaPromotionState,
    TranscriptPanel,
};

#[derive(Clone)]
pub(super) struct TranscriptMediaRenderContext {
    cache: Rc<RefCell<TranscriptMediaCache>>,
    events: Rc<RefCell<MediaDiagnosticLog>>,
    visible_media: Rc<RefCell<VisibleMediaDiagnostics>>,
    panel: Entity<TranscriptPanel>,
    connector: Option<ManagedBackendClientConnector>,
    timeout: Duration,
    row_identity: Option<String>,
    promotion: TranscriptMediaPromotionState,
    image_menu: TranscriptImageMenuRenderState,
    profiler: Option<Rc<TranscriptFrameProfile>>,
}

impl TranscriptMediaRenderContext {
    pub(super) fn new(
        cache: Rc<RefCell<TranscriptMediaCache>>,
        events: Rc<RefCell<MediaDiagnosticLog>>,
        visible_media: Rc<RefCell<VisibleMediaDiagnostics>>,
        panel: Entity<TranscriptPanel>,
        connector: Option<ManagedBackendClientConnector>,
        timeout: Duration,
        promotion: TranscriptMediaPromotionState,
        image_menu: TranscriptImageMenuRenderState,
    ) -> Self {
        Self {
            cache,
            events,
            visible_media,
            panel,
            connector,
            timeout,
            row_identity: None,
            promotion,
            image_menu,
            profiler: None,
        }
    }

    pub(super) fn with_profiler(mut self, profiler: Option<Rc<TranscriptFrameProfile>>) -> Self {
        self.profiler = profiler;
        self
    }

    pub(super) fn for_row(mut self, row_identity: String) -> Self {
        self.promotion.note_row_rendered(row_identity.as_str());
        self.image_menu.note_row_rendered(row_identity.as_str());
        self.row_identity = Some(row_identity);
        self
    }

    pub(super) fn media_for(
        &self,
        key: TranscriptMediaCacheKey,
        source: TranscriptMediaSource,
        execution_target: WorkspaceId,
        cx: &mut App,
    ) -> Arc<TranscriptMediaLoadOutcome> {
        let lookup_started = Instant::now();
        let source_kind = transcript_media_source_kind(&source);
        let lookup = self.cache.borrow_mut().lookup(
            key.clone(),
            source.clone(),
            execution_target,
            self.timeout,
        );
        let load_scheduled = lookup.load_request.is_some();
        let mut event = MediaDiagnosticEvent::new("transcript_media_lookup");
        event.key = Some(key.as_str().to_string());
        event.row_identity = self.row_identity.clone();
        event.source_kind = Some(transcript_media_source_kind(&source).to_string());
        event.outcome = Some(transcript_media_outcome_label(&lookup.outcome).to_string());
        if let Some(image) = lookup.outcome.loaded() {
            event.backing_kind = Some(image.diagnostic_backing_kind().to_string());
            event.image_id = image.image_id();
            event.image_asset_key_hash = image.image_asset_key_hash();
        }
        if load_scheduled {
            event.detail = Some("load_scheduled".to_string());
        }
        self.events.borrow_mut().record(event);
        debug!(
            source = source_kind,
            outcome = transcript_media_outcome_label(&lookup.outcome),
            load_scheduled,
            lookup_ms = elapsed_ms(lookup_started.elapsed()),
            "looked up transcript media"
        );
        if let Some(request) = lookup.load_request {
            schedule_media_load(
                self.panel.clone(),
                self.connector.clone(),
                self.timeout,
                self.row_identity.clone(),
                self.events.clone(),
                request,
                cx,
            );
        }
        release_evicted_media_images(lookup.evicted_images, self.events.clone(), cx);
        lookup.outcome
    }

    pub(super) fn panel(&self) -> Entity<TranscriptPanel> {
        self.panel.clone()
    }

    pub(super) fn promotion(&self) -> TranscriptMediaPromotionState {
        self.promotion.clone()
    }

    pub(super) fn image_menu(&self) -> TranscriptImageMenuRenderState {
        self.image_menu.clone()
    }

    pub(super) fn visible_media(&self) -> Rc<RefCell<VisibleMediaDiagnostics>> {
        self.visible_media.clone()
    }

    pub(super) fn observe_media_run_render(&self, elapsed: Duration) {
        if let Some(profiler) = self.profiler.as_ref() {
            profiler.observe_media_run_render(elapsed);
        }
    }
}

fn schedule_media_load(
    panel: Entity<TranscriptPanel>,
    connector: Option<ManagedBackendClientConnector>,
    timeout: Duration,
    row_identity: Option<String>,
    events: Rc<RefCell<MediaDiagnosticLog>>,
    request: TranscriptMediaLoadRequest,
    cx: &mut App,
) {
    let follow_up_connector = connector.clone();
    let load_task = cx.background_executor().spawn(async move {
        match connector {
            Some(connector) => {
                let connect_started = Instant::now();
                match connector.connect_request_client(timeout) {
                    Ok(mut session) => {
                        debug!(
                            backend_connect_ms = elapsed_ms(connect_started.elapsed()),
                            "connected transcript media backend reader"
                        );
                        request.load(&mut session)
                    }
                    Err(_) => {
                        debug!(
                            backend_connect_ms = elapsed_ms(connect_started.elapsed()),
                            "failed to connect transcript media backend reader"
                        );
                        request.load(&mut UnavailableMediaFileReader)
                    }
                }
            }
            None => request.load(&mut UnavailableMediaFileReader),
        }
    });
    cx.spawn(move |cx: &mut AsyncApp| {
        let mut cx = cx.clone();
        async move {
            let completion = load_task.await;
            let _ = panel.update(&mut cx, |view, cx| {
                let loaded_diagnostic = completion.loaded_image().map(|image| {
                    let dimensions = image.natural_dimensions();
                    (
                        image.format().mime_type().to_string(),
                        image.diagnostic_backing_kind().to_string(),
                        image.retained_compressed_bytes_len(),
                        image.retained_decoded_bytes_estimate(),
                        dimensions.width(),
                        dimensions.height(),
                        image.image_id(),
                        image.image_asset_key_hash(),
                    )
                });
                let result = view.media_cache.borrow_mut().complete_load(completion);
                let mut event = MediaDiagnosticEvent::new("transcript_media_load_completed");
                event.outcome = Some(
                    if result.display_changed {
                        "display_changed"
                    } else if result.stale {
                        "stale"
                    } else {
                        "unchanged"
                    }
                    .to_string(),
                );
                event.row_identity = row_identity.clone();
                if let Some((
                    format,
                    backing_kind,
                    compressed_bytes,
                    decoded_bytes_estimate,
                    natural_width,
                    natural_height,
                    image_id,
                    image_asset_key_hash,
                )) = loaded_diagnostic
                {
                    event.format = Some(format);
                    event.backing_kind = Some(backing_kind);
                    event.compressed_bytes = compressed_bytes;
                    event.decoded_bytes_estimate = decoded_bytes_estimate;
                    event.natural_width = Some(natural_width);
                    event.natural_height = Some(natural_height);
                    event.image_id = image_id;
                    event.image_asset_key_hash = image_asset_key_hash;
                }
                events.borrow_mut().record(event);
                release_evicted_media_images(result.evicted_images, events.clone(), cx);
                if let Some(request) = result.follow_up_request {
                    schedule_media_load(
                        cx.entity(),
                        follow_up_connector.clone(),
                        timeout,
                        row_identity.clone(),
                        events.clone(),
                        request,
                        cx,
                    );
                }
                if result.display_changed {
                    let mut row_measure_invalidated = false;
                    if let Some(row_identity) = row_identity.as_deref()
                        && let Some((list_state, row_index)) = view
                            .shell
                            .read(cx)
                            .conversation_surface()
                            .and_then(|surface| {
                                surface
                                    .transcript_presentation()
                                    .row_index_for_identity(row_identity)
                                    .map(|row_index| (surface.transcript_list_state(), row_index))
                            })
                    {
                        list_state.invalidate_item_measurement(row_index);
                        row_measure_invalidated = true;
                    }
                    debug!(
                        display_changed = result.display_changed,
                        stale = result.stale,
                        row_measure_invalidated,
                        "applied transcript media load completion"
                    );
                    cx.notify();
                } else {
                    debug!(
                        display_changed = result.display_changed,
                        stale = result.stale,
                        "applied transcript media load completion"
                    );
                }
            });
        }
    })
    .detach();
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn transcript_media_source_kind(source: &TranscriptMediaSource) -> &'static str {
    match source {
        TranscriptMediaSource::MarkdownImage { .. } => "markdown_image",
        TranscriptMediaSource::NativeImageGeneration { .. } => "native_generated_image",
    }
}

fn transcript_media_outcome_label(outcome: &TranscriptMediaLoadOutcome) -> &'static str {
    match outcome {
        TranscriptMediaLoadOutcome::Pending { .. } => "pending",
        TranscriptMediaLoadOutcome::Loaded(_) => "loaded",
        TranscriptMediaLoadOutcome::RenderNotSupported { .. } => "render_not_supported",
        TranscriptMediaLoadOutcome::TooLarge { .. } => "too_large",
        TranscriptMediaLoadOutcome::FileUnavailable { .. } => "file_unavailable",
        TranscriptMediaLoadOutcome::PathNotAllowed { .. } => "path_not_allowed",
    }
}

fn release_evicted_media_images(
    images: Vec<Arc<Image>>,
    events: Rc<RefCell<MediaDiagnosticLog>>,
    cx: &mut App,
) {
    if images.is_empty() {
        return;
    }
    let image_count = images.len();
    for image in images {
        image.remove_asset(cx);
    }
    debug!(
        image_count,
        "released evicted transcript media GPUI image assets"
    );
    let mut event = MediaDiagnosticEvent::new("gpui_media_images_released");
    event.image_count = Some(image_count);
    events.borrow_mut().record(event);
}

struct UnavailableMediaFileReader;

impl TranscriptMediaFileReader for UnavailableMediaFileReader {
    type Error = &'static str;

    fn read_file_bytes(&mut self, _path: &str, _timeout: Duration) -> Result<Vec<u8>, Self::Error> {
        Err("backend file reader unavailable")
    }
}
