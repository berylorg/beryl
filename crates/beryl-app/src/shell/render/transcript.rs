mod activity_caret;
mod block_fallback;
mod block_markdown;
mod code_panel_controls;
mod image_markdown;
mod inline_markdown;
mod item_blocks;
mod markdown_cache;
mod markdown_copy;
mod media_blocks;
mod media_cache;
mod media_preload;
mod nested_scroll;
mod selection_context;
mod selection_highlight;
mod stream_projection;
mod text_blocks;
mod turn_blocks;
mod turn_item_media_units;
mod turn_media_units;
mod turn_user_media_units;

use std::{
    cell::Cell,
    cell::RefCell,
    collections::HashMap,
    collections::HashSet,
    rc::Rc,
    sync::Arc,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use gpui::{
    AnyElement, App, AsyncApp, Bounds, ClipboardItem, Context, DispatchPhase, Entity, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, Image, KeyBinding, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels, Render, ScrollHandle,
    ScrollWheelEvent, SharedString, Task, TextLayout, TextRun, WeakEntity, Window, anchored,
    canvas, div, fill, img, point, prelude::*, px, rgb,
};
use tracing::{Level, debug};

use crate::AppearanceSettings;
use crate::diagnostic_dynamic_tools::{
    MediaDiagnosticEvent, MediaDiagnosticLog, PreviewStateDiagnostic, VisibleMediaDiagnostics,
    VisibleMediaSnapshot,
};
use crate::shell::{
    ScrollbarRegion, ShellView,
    execution_detail::{TranscriptRenderMetrics, TurnExecutionRecord},
    image_preview_popup,
    syntax_highlighting::{SyntaxHighlightCache, SyntaxHighlightCacheStats},
    transcript_anchor::{self, TranscriptSubmitAnchorSnapshot},
    transcript_branch_menu_state::TranscriptImageMenuTarget,
    transcript_edit_mode_state::TranscriptEditModeSnapshot,
    transcript_image_preview::{
        TranscriptImagePreviewData, TranscriptImagePreviewUpdate,
        spawn_transcript_image_preview_worker,
    },
    transcript_markdown::{
        TranscriptMarkdownCache, TranscriptMarkdownCacheKey, TranscriptMarkdownCacheStats,
    },
    transcript_media::{
        TranscriptMediaCache, TranscriptMediaCacheKey, TranscriptMediaCacheStats,
        TranscriptMediaLayoutInput, TranscriptMediaSource, transcript_media_layout_metrics,
    },
    transcript_presentation::TranscriptActivityCaret,
    transcript_presentation::{
        transcript_frame_preload_range, transcript_frame_presentation_range,
    },
    transcript_quote_popup::{self, TranscriptQuotePopupState},
    transcript_selection::{
        TranscriptSelectionState, TranscriptTextLineKey, TranscriptTextPoint,
        VisibleTranscriptTextFrame, VisibleTranscriptTextLine, vertical_hit_candidate_range,
    },
};

use self::code_panel_controls::TranscriptCodePanelState;
use self::image_markdown::markdown_source_with_image_marker_placeholders;
use self::inline_markdown::{TranscriptSelectableImageMarker, TranscriptSelectableTextLine};
use self::media_blocks::TranscriptMediaRenderLayout;
use self::media_cache::TranscriptMediaRenderContext;
use self::nested_scroll::TranscriptNestedScrollOwnership;
use self::selection_highlight::wrapped_line_selection_highlight_bounds;
use self::stream_projection::{TranscriptStreamProjection, TranscriptStreamProjectionContext};
use self::{
    activity_caret::{
        ActivityCaretBlinkSchedule, ActivityCaretBlinkState, ActivityCaretMotion,
        platform_caret_blink_interval,
    },
    markdown_cache::TranscriptMarkdownRenderContext,
    text_blocks::{
        empty_state, older_history_loading_state, pending_thread_activation_state,
        released_history_placeholder_state,
    },
    turn_blocks::{
        collect_turn_card_markdown_code_panel_ids, render_turn_card, user_prompt_block_path,
    },
};
use super::super::virtual_list::{
    ListOffset, ListScrollEvent, ListScrollPosition, ListState, list,
};
use super::scrollbars::{
    ScrollDirection, ScrollbarInteraction, ScrollbarScrollState, ScrollbarVisibilityState,
    ScrollbarVisibilityUpdateCallback, render_interactive_vertical_scrollbar,
};
use super::{
    code_panel,
    code_panel_projection_cache::{CodePanelProjectionCache, CodePanelProjectionCacheStats},
    common::panel_shell,
};
use crate::memory_diagnostics::{self, MemoryMilestone, RetainedStateSnapshot};
use crate::shell::transcript_markdown::markdown_code_panel_id_belongs_to_row;

const SLOW_TRANSCRIPT_FRAME_THRESHOLD: Duration = Duration::from_millis(8);
const SLOW_TRANSCRIPT_TURN_BUILD_THRESHOLD: Duration = Duration::from_millis(1);
const TURN_ROW_HORIZONTAL_PADDING: f32 = 24.0;
const BORDERED_CODE_PANEL_HORIZONTAL_PADDING: f32 = 24.0;
const TRANSCRIPT_CODE_PANEL_MIN_HEIGHT: f32 = 64.0;
const TRANSCRIPT_CODE_PANEL_DEFAULT_MAX_HEIGHT: f32 = 360.0;
const TRANSCRIPT_CODE_PANEL_MAX_HEIGHT_RATIO: f32 = 0.7;
const CODE_PANEL_INTERACTION_STATE_MAX_ENTRIES: usize = 512;
const TRANSCRIPT_KEY_CONTEXT: &str = "TranscriptPanel";
const TRANSCRIPT_SELECTION_HIGHLIGHT_COLOR: gpui::Rgba = gpui::Rgba {
    r: 0.23,
    g: 0.51,
    b: 0.96,
    a: 0.32,
};

gpui::actions!(
    beryl_transcript,
    [CopyTranscriptSelection, ClearTranscriptSelection]
);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(
            "ctrl-c",
            CopyTranscriptSelection,
            Some(TRANSCRIPT_KEY_CONTEXT),
        ),
        KeyBinding::new(
            "ctrl-insert",
            CopyTranscriptSelection,
            Some(TRANSCRIPT_KEY_CONTEXT),
        ),
        KeyBinding::new(
            "escape",
            ClearTranscriptSelection,
            Some(TRANSCRIPT_KEY_CONTEXT),
        ),
    ]);
}

#[derive(Clone, Copy)]
pub(super) struct TranscriptCodeLayout {
    pub transcript_bordered_panel_columns: usize,
    pub resizable_panel_max_height: Pixels,
}

impl TranscriptCodeLayout {
    fn for_transcript_size(
        transcript_width: Pixels,
        transcript_height: Pixels,
        window: &Window,
    ) -> Self {
        let transcript_bordered_panel_width = (transcript_width
            - px(TURN_ROW_HORIZONTAL_PADDING + BORDERED_CODE_PANEL_HORIZONTAL_PADDING))
        .max(px(0.0));

        Self {
            transcript_bordered_panel_columns: code_panel::smart_wrap_columns_for_width(
                transcript_bordered_panel_width,
                window,
            ),
            resizable_panel_max_height: transcript_code_panel_max_height(transcript_height),
        }
    }
}

pub(crate) struct TranscriptPanel {
    shell: Entity<ShellView>,
    focus_handle: FocusHandle,
    soft_wrapped_panel_keys: HashSet<String>,
    resized_panel_heights: HashMap<String, Pixels>,
    markdown_cache: Rc<RefCell<TranscriptMarkdownCache>>,
    syntax_highlight_cache: Rc<RefCell<SyntaxHighlightCache>>,
    code_panel_projection_cache: Rc<RefCell<CodePanelProjectionCache>>,
    media_cache: Rc<RefCell<TranscriptMediaCache>>,
    media_events: Rc<RefCell<MediaDiagnosticLog>>,
    visible_media: Rc<RefCell<VisibleMediaDiagnostics>>,
    markdown_cache_scope: Option<TranscriptMarkdownCacheScope>,
    stream_projection: Rc<RefCell<TranscriptStreamProjection>>,
    code_panel_scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
    code_panel_scrollbar_visibility: HashMap<String, ScrollbarVisibilityState>,
    nested_scroll_ownership: TranscriptNestedScrollOwnership,
    nested_code_panel_selected_during_mouse_down: bool,
    code_panel_resize_drag: Option<CodePanelResizeDragState>,
    layout_bounds: Option<Bounds<Pixels>>,
    text_selection: TranscriptSelectionState,
    visible_text_frame: VisibleTranscriptTextFrame,
    next_visible_text_frame: VisibleTranscriptTextFrame,
    visible_text_geometry: HashMap<TranscriptTextLineKey, TranscriptTextLineGeometry>,
    next_visible_text_geometry: HashMap<TranscriptTextLineKey, TranscriptTextLineGeometry>,
    visible_text_geometry_viewport_bounds: Option<Bounds<Pixels>>,
    visible_text_hit_geometry: Vec<TranscriptTextLineHitGeometry>,
    next_visible_text_hit_geometry: Vec<TranscriptTextLineHitGeometry>,
    activity_caret_blink: ActivityCaretBlinkState,
    activity_caret_blink_task: Option<Task<()>>,
    quote_popup: TranscriptQuotePopupState,
    handled_transcript_reset_generation: u64,
    memory_logged_transcript_reset_generation: u64,
    handled_content_release_generation: u64,
    current_workspace_id: Option<BerylWorkspaceId>,
    image_preview_popup: Option<TranscriptImagePreviewPopupState>,
    image_preview_receiver: Option<Receiver<TranscriptImagePreviewUpdate>>,
    next_image_preview_request_id: u64,
    promoted_media: Option<TranscriptMediaRenderIdentity>,
    validated_image_menu_target: Option<TranscriptImageMenuTarget>,
}

pub(crate) struct TranscriptPanelDiagnosticSnapshot {
    markdown_stats: TranscriptMarkdownCacheStats,
    syntax_highlight_stats: SyntaxHighlightCacheStats,
    code_panel_projection_stats: CodePanelProjectionCacheStats,
    media_stats: TranscriptMediaCacheStats,
    stream_projection_counts: stream_projection::TranscriptStreamProjectionRetainedCounts,
    pub(crate) visible_media: VisibleMediaSnapshot,
    pub(crate) media_events: crate::diagnostic_dynamic_tools::MediaEventSnapshot,
}

impl TranscriptPanelDiagnosticSnapshot {
    pub(crate) fn add_retained_counts(&self, retained_state: &mut RetainedStateSnapshot) {
        retained_state.markdown_cache_entries = Some(self.markdown_stats.entries);
        retained_state.markdown_cache_pending_entries = Some(self.markdown_stats.pending_entries);
        retained_state.markdown_source_bytes = Some(self.markdown_stats.source_bytes);
        retained_state.markdown_estimated_retained_bytes =
            Some(self.markdown_stats.estimated_retained_bytes);
        retained_state.markdown_in_flight_source_bytes =
            Some(self.markdown_stats.in_flight_source_bytes);
        retained_state.markdown_displayed_source_bytes =
            Some(self.markdown_stats.displayed_source_bytes);
        retained_state.markdown_parsed_source_bytes = Some(self.markdown_stats.parsed_source_bytes);
        retained_state.markdown_estimated_structure_bytes =
            Some(self.markdown_stats.markdown_estimated_structure_bytes);
        retained_state.markdown_blocks = Some(self.markdown_stats.markdown_blocks);
        retained_state.markdown_inlines = Some(self.markdown_stats.markdown_inlines);
        retained_state.markdown_media_requests = Some(self.markdown_stats.markdown_media_requests);
        retained_state.syntax_highlight_cache_entries = Some(self.syntax_highlight_stats.entries);
        retained_state.syntax_highlight_represented_source_bytes =
            Some(self.syntax_highlight_stats.represented_source_bytes);
        retained_state.syntax_highlight_estimated_retained_bytes =
            Some(self.syntax_highlight_stats.estimated_retained_bytes);
        retained_state.syntax_highlight_tokens = Some(self.syntax_highlight_stats.tokens);
        retained_state.media_cache_entries = Some(self.media_stats.entries);
        retained_state.media_cache_pending_entries = Some(self.media_stats.pending_entries);
        retained_state.media_cache_loaded_entries = Some(self.media_stats.loaded_entries);
        retained_state.media_cache_loaded_retained_byte_entries =
            Some(self.media_stats.loaded_retained_byte_entries);
        retained_state.media_cache_loaded_source_backed_file_entries =
            Some(self.media_stats.loaded_source_backed_file_entries);
        retained_state.media_cache_loaded_native_generated_source_backed_file_entries = Some(
            self.media_stats
                .loaded_native_generated_source_backed_file_entries,
        );
        retained_state.media_cache_loaded_native_generated_retained_byte_entries = Some(
            self.media_stats
                .loaded_native_generated_retained_byte_entries,
        );
        retained_state.media_cache_loaded_image_bytes = Some(self.media_stats.loaded_image_bytes);
        retained_state.media_cache_decoded_image_bytes_estimate =
            Some(self.media_stats.decoded_image_bytes_estimate);
        retained_state.media_cache_thumbnail_count = Some(self.media_stats.thumbnail_count);
        retained_state.stream_projection_entries = Some(self.stream_projection_counts.entries);
        retained_state.stream_projection_key_bytes = Some(self.stream_projection_counts.key_bytes);
        retained_state.stream_projection_text_bytes =
            Some(self.stream_projection_counts.text_bytes);
        retained_state.stream_projection_uncommitted_entries =
            Some(self.stream_projection_counts.uncommitted_entries);
        if let Some(total) = retained_state.retained_payload_bytes_lower_bound.as_mut() {
            *total = total
                .saturating_add(self.markdown_stats.estimated_retained_bytes)
                .saturating_add(self.syntax_highlight_stats.estimated_retained_bytes)
                .saturating_add(self.code_panel_projection_stats.estimated_retained_bytes)
                .saturating_add(self.media_stats.loaded_image_bytes)
                .saturating_add(self.media_stats.decoded_image_bytes_estimate)
                .saturating_add(self.stream_projection_counts.key_bytes)
                .saturating_add(self.stream_projection_counts.text_bytes);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct TranscriptMediaRenderIdentity {
    row_identity: String,
    key: TranscriptMediaCacheKey,
    source: TranscriptMediaRenderIdentitySource,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TranscriptMediaRenderIdentitySource {
    MarkdownImage {
        alt: String,
        destination: String,
        title: Option<String>,
    },
    NativeImageGeneration {
        id: String,
    },
}

#[derive(Clone)]
pub(super) struct TranscriptMediaPromotionState {
    promoted: Option<TranscriptMediaRenderIdentity>,
    target_row_rendered: Rc<Cell<bool>>,
    target_identity_rendered: Rc<Cell<bool>>,
}

#[derive(Clone)]
pub(super) struct TranscriptImageMenuRenderState {
    target: Option<TranscriptImageMenuTarget>,
    target_row_rendered: Rc<Cell<bool>>,
    rendered_loaded_target: Rc<RefCell<Option<TranscriptImageMenuTarget>>>,
}

impl TranscriptMediaRenderIdentity {
    pub(super) fn new(
        row_identity: impl Into<String>,
        key: TranscriptMediaCacheKey,
        source: &TranscriptMediaSource,
    ) -> Self {
        let source = match source {
            TranscriptMediaSource::MarkdownImage {
                alt,
                destination,
                title,
            } => TranscriptMediaRenderIdentitySource::MarkdownImage {
                alt: alt.clone(),
                destination: destination.clone(),
                title: title.clone(),
            },
            TranscriptMediaSource::NativeImageGeneration { id, .. } => {
                TranscriptMediaRenderIdentitySource::NativeImageGeneration { id: id.clone() }
            }
        };
        Self {
            row_identity: row_identity.into(),
            key,
            source,
        }
    }

    pub(super) fn row_identity(&self) -> &str {
        self.row_identity.as_str()
    }

    pub(super) fn key(&self) -> &TranscriptMediaCacheKey {
        &self.key
    }

    pub(super) fn image_menu_identity(&self) -> String {
        format!("{self:?}")
    }
}

impl TranscriptMediaPromotionState {
    fn new(promoted: Option<TranscriptMediaRenderIdentity>) -> Self {
        Self {
            promoted,
            target_row_rendered: Rc::new(Cell::new(false)),
            target_identity_rendered: Rc::new(Cell::new(false)),
        }
    }

    pub(super) fn promoted(&self) -> Option<&TranscriptMediaRenderIdentity> {
        self.promoted.as_ref()
    }

    pub(super) fn note_row_rendered(&self, row_identity: &str) {
        if self
            .promoted
            .as_ref()
            .is_some_and(|identity| identity.row_identity() == row_identity)
        {
            self.target_row_rendered.set(true);
        }
    }

    pub(super) fn note_identity_rendered(&self, identity: &TranscriptMediaRenderIdentity) {
        if self.promoted.as_ref() == Some(identity) {
            self.target_identity_rendered.set(true);
        }
    }

    fn rendered_target_row_without_identity(&self) -> bool {
        self.promoted.is_some()
            && self.target_row_rendered.get()
            && !self.target_identity_rendered.get()
    }
}

impl TranscriptImageMenuRenderState {
    fn new(target: Option<TranscriptImageMenuTarget>) -> Self {
        Self {
            target,
            target_row_rendered: Rc::new(Cell::new(false)),
            rendered_loaded_target: Rc::new(RefCell::new(None)),
        }
    }

    pub(super) fn note_row_rendered(&self, row_identity: &str) {
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.row_identity() == row_identity)
        {
            self.target_row_rendered.set(true);
        }
    }

    pub(super) fn note_loaded_image_rendered(&self, target: &TranscriptImageMenuTarget) {
        if self
            .target
            .as_ref()
            .is_some_and(|open_target| open_target.matches_loaded_image(target))
        {
            *self.rendered_loaded_target.borrow_mut() = Some(target.clone());
        }
    }

    fn rendered_loaded_target(&self) -> Option<TranscriptImageMenuTarget> {
        self.rendered_loaded_target.borrow().clone()
    }

    fn target_not_rendered_loaded(&self) -> bool {
        self.target.is_some() && self.rendered_loaded_target.borrow().is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TranscriptMarkdownCacheScope {
    workspace: WorkspaceId,
    selected_thread_id: Option<String>,
}

#[derive(Clone)]
struct CodePanelResizeDragState {
    panel_key: String,
    panel_top: Pixels,
    pointer_offset: Pixels,
}

#[derive(Clone)]
struct TranscriptTextLineGeometry {
    bounds: Bounds<Pixels>,
    layout: TextLayout,
    display_text_len: usize,
}

#[derive(Clone)]
struct TranscriptTextLineHitGeometry {
    key: TranscriptTextLineKey,
    order: usize,
    bounds: Bounds<Pixels>,
    layout: TextLayout,
    display_text_len: usize,
    image_markers: Vec<TranscriptSelectableImageMarker>,
}

struct TranscriptImagePreviewPopupState {
    request_id: u64,
    label: String,
    position: gpui::Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    status: TranscriptImagePreviewPopupStatus,
}

enum TranscriptImagePreviewPopupStatus {
    Loading,
    Loaded(TranscriptImagePreviewData),
    Unavailable(String),
}

#[derive(Clone)]
pub(crate) struct TranscriptPanelSnapshot {
    pub workspace_id: Option<BerylWorkspaceId>,
    pub workspace: WorkspaceId,
    pub appearance: AppearanceSettings,
    pub selected_thread_present: bool,
    pub selected_thread_id: Option<String>,
    pub pending_thread_activation_label: Option<String>,
    pub transcript_width: Pixels,
    pub transcript_list_state: ListState,
    pub submit_anchor: Option<TranscriptSubmitAnchorSnapshot>,
    pub loaded_history_anchor_pending: bool,
    pub older_history_loading: bool,
    pub metrics: Option<TranscriptRenderMetrics>,
    pub activity_caret: Option<TranscriptActivityCaret>,
    pub transcript_edit_mode: Option<TranscriptEditModeSnapshot>,
    pub transcript_reset_generation: u64,
    pub content_release_generation: u64,
    pub content_release_row_identities: Vec<String>,
}

impl Focusable for TranscriptPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TranscriptPanel {
    pub(crate) fn new(shell: Entity<ShellView>, cx: &mut Context<Self>) -> Self {
        Self {
            shell,
            focus_handle: cx.focus_handle(),
            soft_wrapped_panel_keys: HashSet::new(),
            resized_panel_heights: HashMap::new(),
            markdown_cache: Rc::new(RefCell::new(TranscriptMarkdownCache::default())),
            syntax_highlight_cache: Rc::new(RefCell::new(SyntaxHighlightCache::default())),
            code_panel_projection_cache: Rc::new(RefCell::new(CodePanelProjectionCache::default())),
            media_cache: Rc::new(RefCell::new(TranscriptMediaCache::default())),
            media_events: Rc::new(RefCell::new(MediaDiagnosticLog::default())),
            visible_media: Rc::new(RefCell::new(VisibleMediaDiagnostics::default())),
            markdown_cache_scope: None,
            stream_projection: Rc::new(RefCell::new(TranscriptStreamProjection::default())),
            code_panel_scroll_handles: Rc::new(RefCell::new(HashMap::new())),
            code_panel_scrollbar_visibility: HashMap::new(),
            nested_scroll_ownership: TranscriptNestedScrollOwnership::default(),
            nested_code_panel_selected_during_mouse_down: false,
            code_panel_resize_drag: None,
            layout_bounds: None,
            text_selection: TranscriptSelectionState::default(),
            visible_text_frame: VisibleTranscriptTextFrame::default(),
            next_visible_text_frame: VisibleTranscriptTextFrame::default(),
            visible_text_geometry: HashMap::new(),
            next_visible_text_geometry: HashMap::new(),
            visible_text_geometry_viewport_bounds: None,
            visible_text_hit_geometry: Vec::new(),
            next_visible_text_hit_geometry: Vec::new(),
            activity_caret_blink: ActivityCaretBlinkState::default(),
            activity_caret_blink_task: None,
            quote_popup: TranscriptQuotePopupState::default(),
            handled_transcript_reset_generation: 0,
            memory_logged_transcript_reset_generation: 0,
            handled_content_release_generation: 0,
            current_workspace_id: None,
            image_preview_popup: None,
            image_preview_receiver: None,
            next_image_preview_request_id: 1,
            promoted_media: None,
            validated_image_menu_target: None,
        }
    }

    pub(crate) fn diagnostic_snapshot(&self) -> TranscriptPanelDiagnosticSnapshot {
        let mut visible_media = self.visible_media.borrow().snapshot();
        visible_media.preview.transcript_image_preview = self.transcript_preview_diagnostic();
        TranscriptPanelDiagnosticSnapshot {
            markdown_stats: self.markdown_cache.borrow().stats(),
            syntax_highlight_stats: self.syntax_highlight_cache.borrow().stats(),
            code_panel_projection_stats: self.code_panel_projection_cache.borrow().stats(),
            media_stats: self.media_cache.borrow().stats(),
            stream_projection_counts: self.stream_projection.borrow().retained_counts(),
            visible_media,
            media_events: self.media_events.borrow().snapshot(),
        }
    }

    fn transcript_preview_diagnostic(&self) -> Option<PreviewStateDiagnostic> {
        let popup = self.image_preview_popup.as_ref()?;
        let (state, compressed_bytes) = match &popup.status {
            TranscriptImagePreviewPopupStatus::Loading => ("loading", None),
            TranscriptImagePreviewPopupStatus::Unavailable(_) => ("unavailable", None),
            TranscriptImagePreviewPopupStatus::Loaded(_) => ("loaded", None),
        };
        Some(PreviewStateDiagnostic {
            state: state.to_string(),
            compressed_bytes,
        })
    }

    fn toggle_code_panel_soft_wrap(&mut self, panel_key: String, cx: &mut Context<Self>) {
        if !self.soft_wrapped_panel_keys.insert(panel_key.clone()) {
            self.soft_wrapped_panel_keys.remove(&panel_key);
        }
        cx.notify();
    }

    fn code_panel_height(&self, panel_key: &str) -> Option<Pixels> {
        self.resized_panel_heights.get(panel_key).copied()
    }

    fn begin_code_panel_resize(
        &mut self,
        panel_key: String,
        panel_top: Pixels,
        current_height: Pixels,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let handle_top = panel_top + current_height;
        self.resized_panel_heights
            .entry(panel_key.clone())
            .or_insert(current_height);
        self.code_panel_resize_drag = Some(CodePanelResizeDragState {
            panel_key,
            panel_top,
            pointer_offset: event.position.y - handle_top,
        });
        cx.notify();
    }

    fn update_code_panel_resize(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.code_panel_resize_drag.clone() else {
            return;
        };

        let desired_handle_top = event.position.y - drag.pointer_offset;
        let desired_height = desired_handle_top - drag.panel_top;
        let clamped_height = code_panel::clamp_resizable_code_panel_height(
            desired_height,
            px(TRANSCRIPT_CODE_PANEL_MIN_HEIGHT),
            Some(self.code_panel_max_height()),
        );
        if self.code_panel_height(drag.panel_key.as_str()) == Some(clamped_height) {
            return;
        }

        self.resized_panel_heights
            .insert(drag.panel_key.clone(), clamped_height);
        cx.notify();
    }

    fn end_code_panel_resize(&mut self, cx: &mut Context<Self>) {
        if self.code_panel_resize_drag.is_none() {
            return;
        }

        self.code_panel_resize_drag = None;
        cx.notify();
    }

    pub(super) fn select_nested_code_panel(&mut self, panel_key: String, cx: &mut Context<Self>) {
        self.nested_code_panel_selected_during_mouse_down = true;
        if self.nested_scroll_ownership.select_panel(panel_key) {
            cx.notify();
        }
    }

    fn clear_nested_code_panel_selection(&mut self, cx: &mut Context<Self>) {
        if self.nested_scroll_ownership.clear_to_transcript() {
            cx.notify();
        }
    }

    fn begin_text_span_frame(&mut self) {
        self.next_visible_text_frame.clear();
        self.next_visible_text_geometry.clear();
        self.next_visible_text_hit_geometry.clear();
    }

    fn register_selectable_text_line(
        &mut self,
        line: TranscriptSelectableTextLine,
        bounds: Bounds<Pixels>,
        layout: TextLayout,
    ) {
        self.register_selectable_copy_line(line.clone());
        let key = line.key;
        let order = line.order;
        let display_text_len = line.display_text_len;
        let image_markers = line.image_markers;
        self.next_visible_text_geometry.insert(
            key.clone(),
            TranscriptTextLineGeometry {
                bounds,
                layout: layout.clone(),
                display_text_len,
            },
        );
        self.next_visible_text_hit_geometry
            .push(TranscriptTextLineHitGeometry {
                key,
                order,
                bounds,
                layout,
                display_text_len,
                image_markers,
            });
    }

    fn register_selectable_copy_line(&mut self, line: TranscriptSelectableTextLine) {
        self.next_visible_text_frame
            .insert_line(VisibleTranscriptTextLine::with_copy_text(
                line.key.clone(),
                line.order,
                line.display_text,
                line.copy_text,
                line.break_before,
            ));
    }

    fn finish_text_span_frame(&mut self, viewport_bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.next_visible_text_frame.finish_insertions();
        self.next_visible_text_hit_geometry.sort_by_key(|geometry| {
            (
                geometry.bounds.top(),
                geometry.bounds.bottom(),
                geometry.order,
            )
        });
        self.visible_text_frame = std::mem::take(&mut self.next_visible_text_frame);
        self.visible_text_geometry = std::mem::take(&mut self.next_visible_text_geometry);
        let viewport_changed = self.visible_text_geometry_viewport_bounds != Some(viewport_bounds);
        self.visible_text_geometry_viewport_bounds = Some(viewport_bounds);
        self.visible_text_hit_geometry = std::mem::take(&mut self.next_visible_text_hit_geometry);
        if self
            .text_selection
            .sync_visible_frame(&self.visible_text_frame)
        {
            if !self.text_selection.has_selected_text() {
                self.quote_popup.clear_selection();
            }
            cx.notify();
        } else if viewport_changed
            && self.text_selection.has_selected_text()
            && !self.text_selection.is_dragging()
        {
            cx.notify();
        }
    }

    fn preload_transcript_media_range(
        &mut self,
        preload_range: std::ops::Range<usize>,
        workspace: &WorkspaceId,
        media_context: TranscriptMediaRenderContext,
        markdown_context: TranscriptMarkdownRenderContext,
        stream_projection_context: TranscriptStreamProjectionContext,
        media_layout: TranscriptMediaRenderLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.visible_media
            .borrow_mut()
            .begin_preload_frame(preload_range.clone());
        let rows = self
            .shell
            .read(cx)
            .conversation_surface()
            .map(|surface| {
                preload_range
                    .clone()
                    .filter_map(|index| surface.transcript_presentation().turn_at(index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for row in rows {
            let row_identity = row.identity.as_str().to_string();
            let media_context = media_context.clone().for_row(row_identity.clone());
            media_preload::preload_turn_media_runs(
                row.index,
                workspace,
                row.turn,
                row_identity.as_str(),
                markdown_context.clone(),
                media_context,
                stream_projection_context.clone(),
                media_layout,
                window,
                cx,
            );
        }
    }

    fn clear_text_selection(&mut self, cx: &mut Context<Self>) {
        let selection_changed = self.text_selection.clear();
        let popup_changed = self.quote_popup.clear_selection();
        if selection_changed || popup_changed {
            cx.notify();
        }
    }

    fn begin_text_selection(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let nested_code_panel_selected =
            std::mem::take(&mut self.nested_code_panel_selected_during_mouse_down);
        let changed = if let Some(point) = self.text_point_for_position(event.position, true) {
            if !nested_code_panel_selected {
                self.clear_nested_code_panel_selection(cx);
            }
            window.focus(&self.focus_handle);
            if event.click_count >= 2 {
                self.text_selection
                    .select_word(point, &self.visible_text_frame)
            } else {
                self.text_selection.begin(point, &self.visible_text_frame)
            }
        } else {
            if !nested_code_panel_selected {
                self.clear_nested_code_panel_selection(cx);
            }
            self.text_selection.clear()
        };
        if changed {
            self.quote_popup.note_selection_mutated();
            cx.notify();
        }
        changed
    }

    fn handle_transcript_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(marker) = self.image_marker_for_position(event.position) {
            self.open_image_marker_preview(marker, event.position, cx);
            window.focus(&self.focus_handle);
            return true;
        }

        self.begin_text_selection(event, window, cx)
    }

    fn handle_transcript_turn_context_mouse_down(
        &mut self,
        row_index: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.text_selection.has_selected_text() {
            return false;
        }

        self.shell.update(cx, |shell, cx| {
            shell.open_transcript_branch_menu_for_row(
                row_index,
                false,
                None,
                event.position,
                window,
                cx,
            )
        })
    }

    fn handle_transcript_image_context_mouse_down(
        &mut self,
        row_identity: &str,
        image_target: TranscriptImageMenuTarget,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.text_selection.has_selected_text() {
            return false;
        }

        self.validate_transcript_image_menu_target(&image_target);
        self.shell.update(cx, |shell, cx| {
            let Some(row_index) = shell.conversation_surface().and_then(|surface| {
                surface
                    .transcript_presentation()
                    .row_index_for_identity(row_identity)
            }) else {
                return false;
            };

            shell.open_transcript_branch_menu_for_row(
                row_index,
                false,
                Some(image_target),
                event.position,
                window,
                cx,
            )
        })
    }

    fn update_text_selection(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !event.dragging() || !self.text_selection.is_dragging() {
            return;
        }

        let Some(point) = self.text_point_for_position(event.position, false) else {
            return;
        };
        if self.text_selection.extend(point, &self.visible_text_frame) {
            self.quote_popup.note_selection_mutated();
            cx.notify();
        }
    }

    fn end_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.text_selection.finish_drag() {
            cx.notify();
        }
    }

    fn accept_quote_popup_for_current_selection(&mut self, cx: &mut Context<Self>) {
        let Some(selected_text) = self.text_selection.selected_text() else {
            return;
        };

        let accepted = self.shell.update(cx, |shell, cx| {
            shell.insert_transcript_quote_into_draft(selected_text, cx)
        });
        if accepted {
            cx.notify();
        }
    }

    fn record_quote_popup_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        let _ = self.quote_popup.set_popup_bounds(bounds);
    }

    fn open_image_marker_preview(
        &mut self,
        marker: TranscriptSelectableImageMarker,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.clear_text_selection(cx);
        self.close_image_preview_popup(cx);
        let request_id = self.next_image_preview_request_id;
        self.next_image_preview_request_id = self.next_image_preview_request_id.saturating_add(1);
        let status = if marker.preview_state
            == crate::shell::execution_detail::TranscriptImagePreviewState::Unavailable
        {
            TranscriptImagePreviewPopupStatus::Unavailable(
                "Image data is no longer available".to_string(),
            )
        } else if let (Some(persistence), Some(workspace_id), Some(asset_id)) = (
            self.shell.read(cx).workspace_persistence_for_worker(),
            self.current_workspace_id.clone(),
            marker.asset_id.clone(),
        ) {
            self.image_preview_receiver = Some(spawn_transcript_image_preview_worker(
                persistence,
                workspace_id,
                asset_id,
                request_id,
            ));
            TranscriptImagePreviewPopupStatus::Loading
        } else {
            TranscriptImagePreviewPopupStatus::Unavailable(
                "Image data is no longer available".to_string(),
            )
        };

        self.image_preview_popup = Some(TranscriptImagePreviewPopupState {
            request_id,
            label: marker.label,
            position,
            bounds: None,
            status,
        });
        self.media_events
            .borrow_mut()
            .record(MediaDiagnosticEvent::new("transcript_image_preview_opened"));
        cx.notify();
    }

    fn close_image_preview_popup(&mut self, cx: &mut Context<Self>) {
        if let Some(popup) = self.image_preview_popup.take() {
            self.image_preview_receiver = None;
            if let TranscriptImagePreviewPopupStatus::Loaded(data) = popup.status {
                data.image().remove_asset(cx);
            }
            self.media_events
                .borrow_mut()
                .record(MediaDiagnosticEvent::new("transcript_image_preview_closed"));
            cx.notify();
        }
    }

    pub(crate) fn close_transient_popups_for_dynamic_tool(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.image_preview_popup.is_none() {
            return false;
        }
        self.close_image_preview_popup(cx);
        true
    }

    pub(super) fn toggle_promoted_media(
        &mut self,
        identity: TranscriptMediaRenderIdentity,
        cx: &mut Context<Self>,
    ) {
        let previous = self.promoted_media.clone();
        self.promoted_media = if self.promoted_media.as_ref() == Some(&identity) {
            None
        } else {
            Some(identity.clone())
        };

        if let Some(previous) = previous.as_ref() {
            self.invalidate_transcript_row_measurement(previous.row_identity(), cx);
        }
        self.invalidate_transcript_row_measurement(identity.row_identity(), cx);
        cx.notify();
    }

    pub(crate) fn transcript_image_menu_target_validated(
        &self,
        target: &TranscriptImageMenuTarget,
    ) -> bool {
        self.validated_image_menu_target
            .as_ref()
            .is_some_and(|validated| validated.matches_loaded_image(target))
    }

    fn validate_transcript_image_menu_target(&mut self, target: &TranscriptImageMenuTarget) {
        self.validated_image_menu_target = Some(target.clone());
    }

    fn clear_validated_transcript_image_menu_target(&mut self) {
        self.validated_image_menu_target = None;
    }

    fn clear_promoted_media(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(previous) = self.promoted_media.take() else {
            return false;
        };
        self.invalidate_transcript_row_measurement(previous.row_identity(), cx);
        true
    }

    fn clear_promoted_media_for_released_rows(
        &mut self,
        row_identities: &HashSet<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let should_clear = self
            .promoted_media
            .as_ref()
            .is_some_and(|identity| row_identities.contains(identity.row_identity()));
        should_clear && self.clear_promoted_media(cx)
    }

    fn invalidate_transcript_row_measurement(&self, row_identity: &str, cx: &mut Context<Self>) {
        let Some((list_state, row_index)) =
            self.shell
                .read(cx)
                .conversation_surface()
                .and_then(|surface| {
                    surface
                        .transcript_presentation()
                        .row_index_for_identity(row_identity)
                        .map(|row_index| (surface.transcript_list_state(), row_index))
                })
        else {
            return;
        };
        list_state.invalidate_item_measurement(row_index);
    }

    fn record_image_preview_popup_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(popup) = self.image_preview_popup.as_mut() {
            popup.bounds = bounds;
        }
    }

    fn handle_image_preview_popup_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let should_close = self.image_preview_popup.as_ref().is_some_and(|popup| {
            image_preview_popup::should_dismiss_for_mouse_down(popup.bounds, event.position)
        });
        if should_close {
            self.close_image_preview_popup(cx);
        }
    }

    fn handle_image_preview_popup_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.image_preview_popup.is_none() || event.keystroke.key.as_str() != "escape" {
            return false;
        }

        self.close_image_preview_popup(cx);
        true
    }

    fn poll_image_preview_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(receiver) = self.image_preview_receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(TranscriptImagePreviewUpdate::Finished { request_id, result }) => {
                self.image_preview_receiver = None;
                let mut completed_event = None;
                if let Some(popup) = self.image_preview_popup.as_mut()
                    && popup.request_id == request_id
                {
                    let mut event = MediaDiagnosticEvent::new("transcript_image_preview_loaded");
                    popup.status = match result {
                        Ok(data) => {
                            event.outcome = Some("loaded".to_string());
                            TranscriptImagePreviewPopupStatus::Loaded(data)
                        }
                        Err(message) => {
                            event.outcome = Some("unavailable".to_string());
                            event.detail = Some(message.clone());
                            TranscriptImagePreviewPopupStatus::Unavailable(message)
                        }
                    };
                    completed_event = Some(event);
                    popup.bounds = None;
                    cx.notify();
                }
                if let Some(event) = completed_event {
                    self.media_events.borrow_mut().record(event);
                }
            }
            Err(TryRecvError::Empty) => window.request_animation_frame(),
            Err(TryRecvError::Disconnected) => {
                self.image_preview_receiver = None;
                if let Some(popup) = self.image_preview_popup.as_mut() {
                    popup.status = TranscriptImagePreviewPopupStatus::Unavailable(
                        "Image preview worker stopped before loading image data.".to_string(),
                    );
                    popup.bounds = None;
                    cx.notify();
                }
            }
        }
    }

    fn copy_text_selection_action(
        &mut self,
        _: &CopyTranscriptSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_text_selection_to_clipboard(cx);
    }

    fn clear_text_selection_action(
        &mut self,
        _: &ClearTranscriptSelection,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_text_selection(cx);
    }

    fn copy_text_selection_to_clipboard(&self, cx: &mut Context<Self>) -> bool {
        let Some(selected_text) = self.text_selection.selected_text() else {
            return false;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selected_text.to_string()));
        true
    }

    fn text_point_for_position(
        &self,
        position: gpui::Point<Pixels>,
        strict_horizontal_hit: bool,
    ) -> Option<TranscriptTextPoint> {
        let geometry = self.text_hit_geometry_for_position(position, strict_horizontal_hit)?;

        let display_offset = match geometry.layout.index_for_position(position) {
            Ok(offset) | Err(offset) => offset.min(geometry.display_text_len),
        };
        Some(TranscriptTextPoint::new(
            geometry.key.clone(),
            display_offset,
        ))
    }

    fn text_hit_geometry_for_position(
        &self,
        position: gpui::Point<Pixels>,
        strict_horizontal_hit: bool,
    ) -> Option<&TranscriptTextLineHitGeometry> {
        let candidates = vertical_hit_candidate_range(
            self.visible_text_hit_geometry.as_slice(),
            position.y,
            |geometry| geometry.bounds.top(),
            |geometry| geometry.bounds.bottom(),
        );

        self.visible_text_hit_geometry[candidates]
            .iter()
            .find(|geometry| {
                if strict_horizontal_hit {
                    geometry.bounds.contains(&position)
                } else {
                    position.y >= geometry.bounds.top() && position.y <= geometry.bounds.bottom()
                }
            })
    }

    fn image_marker_for_position(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<TranscriptSelectableImageMarker> {
        let geometry = self.text_hit_geometry_for_position(position, true)?;
        let display_offset = match geometry.layout.index_for_position(position) {
            Ok(offset) | Err(offset) => offset.min(geometry.display_text_len),
        };
        geometry
            .image_markers
            .iter()
            .find(|marker| {
                display_offset >= marker.display_range.start
                    && display_offset < marker.display_range.end
            })
            .cloned()
    }

    fn selected_text_highlight_bounds(&self) -> Vec<Bounds<Pixels>> {
        self.text_selection
            .selected_line_ranges(&self.visible_text_frame)
            .into_iter()
            .flat_map(|range| {
                self.highlight_bounds_for_range(&range.key, range.start, range.end)
                    .into_iter()
            })
            .collect()
    }

    fn render_selected_text_highlights(&self, entity: Entity<TranscriptPanel>) -> AnyElement {
        canvas(
            |_, _, _| (),
            move |_, _, window, cx| {
                let highlights = entity.update(cx, |view, _| view.selected_text_highlight_bounds());
                for bounds in highlights {
                    window.paint_quad(
                        fill(bounds, TRANSCRIPT_SELECTION_HIGHLIGHT_COLOR).corner_radii(px(2.0)),
                    );
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    fn selected_text_bounds(&self) -> Option<Bounds<Pixels>> {
        transcript_quote_popup::selection_bounds_union(
            self.text_selection
                .selected_line_ranges(&self.visible_text_frame)
                .into_iter()
                .flat_map(|range| {
                    self.highlight_bounds_for_range(&range.key, range.start, range.end)
                        .into_iter()
                }),
        )
    }

    fn render_quote_popup(
        &mut self,
        viewport_bounds: Bounds<Pixels>,
        entity: Entity<TranscriptPanel>,
    ) -> Option<AnyElement> {
        if self.text_selection.is_dragging() || !self.text_selection.has_selected_text() {
            return None;
        }
        if !transcript_quote_popup::selection_geometry_matches_viewport(
            self.visible_text_geometry_viewport_bounds,
            viewport_bounds,
        ) {
            return None;
        }

        let selection_bounds = self.selected_text_bounds()?;
        self.quote_popup
            .open_for_selection(selection_bounds, viewport_bounds);
        let position = self.quote_popup.position()?;
        let local_origin = point(
            position.x - viewport_bounds.origin.x,
            position.y - viewport_bounds.origin.y,
        );

        Some(
            div()
                .absolute()
                .left(local_origin.x)
                .top(local_origin.y)
                .w(transcript_quote_popup::popup_width())
                .h(transcript_quote_popup::popup_height())
                .occlude()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x1e3a5f))
                .bg(rgb(0x081120))
                .shadow_lg()
                .on_children_prepainted({
                    let entity = entity.clone();
                    move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, _| {
                            view.record_quote_popup_bounds(bounds);
                        });
                    }
                })
                .id("transcript-quote-popup")
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |view, cx| {
                            view.accept_quote_popup_for_current_selection(cx);
                        });
                        cx.stop_propagation();
                    }
                })
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(0xe2e8f0))
                        .child("Quote"),
                )
                .into_any_element(),
        )
    }

    fn render_image_preview_popup(&self, entity: Entity<TranscriptPanel>) -> Option<AnyElement> {
        let popup = self.image_preview_popup.as_ref()?;
        let image = match &popup.status {
            TranscriptImagePreviewPopupStatus::Loaded(data) => Some(data.image()),
            TranscriptImagePreviewPopupStatus::Loading
            | TranscriptImagePreviewPopupStatus::Unavailable(_) => None,
        };
        let status_text = match &popup.status {
            TranscriptImagePreviewPopupStatus::Loading => "Loading image".to_string(),
            TranscriptImagePreviewPopupStatus::Unavailable(message) => message.clone(),
            TranscriptImagePreviewPopupStatus::Loaded(_) => "Image unavailable".to_string(),
        };

        Some(
            anchored()
                .position(popup.position)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    div()
                        .on_children_prepainted(move |children, _, cx| {
                            let bounds = children.first().copied();
                            entity.update(cx, |view, _| {
                                view.record_image_preview_popup_bounds(bounds);
                            });
                        })
                        .child(
                            div()
                                .id(("transcript-image-preview-popup", popup.request_id))
                                .w(image_preview_popup::popup_width())
                                .h(image_preview_popup::popup_height())
                                .occlude()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(0x334155))
                                .bg(rgb(0x0f172a))
                                .shadow_lg()
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0x94a3b8))
                                        .child(format!("Image {}", popup.label)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_h(px(0.0))
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(rgb(0x334155))
                                        .bg(rgb(0x020617))
                                        .relative()
                                        .overflow_hidden()
                                        .child(match image {
                                            Some(image) => img(image)
                                                .absolute()
                                                .top_0()
                                                .left_0()
                                                .size_full()
                                                .object_fit(ObjectFit::Contain)
                                                .into_any_element(),
                                            None => div()
                                                .size_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_sm()
                                                .text_color(rgb(0x94a3b8))
                                                .child(status_text)
                                                .into_any_element(),
                                        }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn highlight_bounds_for_range(
        &self,
        key: &TranscriptTextLineKey,
        start: usize,
        end: usize,
    ) -> Vec<Bounds<Pixels>> {
        let Some(geometry) = self.visible_text_geometry.get(key) else {
            return Vec::new();
        };
        let display_start = selection_offset_to_display_offset(start, geometry);
        let display_end = selection_offset_to_display_offset(end, geometry);
        if display_start == display_end {
            return Vec::new();
        }

        let Some(line) = geometry.layout.line_layout_for_index(display_start) else {
            return Vec::new();
        };

        wrapped_line_selection_highlight_bounds(
            line.as_ref(),
            geometry.bounds.origin,
            geometry.layout.line_height(),
            geometry.bounds.size.width,
            display_start..display_end,
        )
    }

    fn retain_nested_code_panel_selection<'a>(
        &mut self,
        visible_panel_ids: impl IntoIterator<Item = &'a str>,
    ) {
        self.nested_scroll_ownership
            .retain_visible_panel_ids(visible_panel_ids);
    }

    fn retain_visible_code_panel_scroll_state(&mut self, visible_panel_ids: &HashSet<String>) {
        self.code_panel_scroll_handles
            .borrow_mut()
            .retain(|panel_id, _| visible_panel_ids.contains(panel_id));
        self.code_panel_scrollbar_visibility
            .retain(|panel_id, _| visible_panel_ids.contains(panel_id));
    }

    fn retain_visible_syntax_highlight_cache(&mut self, visible_panel_ids: &HashSet<String>) {
        self.syntax_highlight_cache
            .borrow_mut()
            .retain_owners(visible_panel_ids);
    }

    fn retain_visible_code_panel_projection_cache(&mut self, visible_panel_ids: &HashSet<String>) {
        self.code_panel_projection_cache
            .borrow_mut()
            .retain_owners(visible_panel_ids);
    }

    fn clear_code_panel_interaction_state(&mut self) {
        self.soft_wrapped_panel_keys.clear();
        self.resized_panel_heights.clear();
        self.code_panel_resize_drag = None;
    }

    fn clear_code_panel_interaction_state_for_released_rows(
        &mut self,
        row_identities: &HashSet<String>,
    ) -> bool {
        let previous_soft_wrap_count = self.soft_wrapped_panel_keys.len();
        let previous_height_count = self.resized_panel_heights.len();
        self.soft_wrapped_panel_keys.retain(|panel_id| {
            !row_identities
                .iter()
                .any(|row_identity| markdown_code_panel_id_belongs_to_row(panel_id, row_identity))
        });
        self.resized_panel_heights.retain(|panel_id, _| {
            !row_identities
                .iter()
                .any(|row_identity| markdown_code_panel_id_belongs_to_row(panel_id, row_identity))
        });
        if self.code_panel_resize_drag.as_ref().is_some_and(|drag| {
            row_identities.iter().any(|row_identity| {
                markdown_code_panel_id_belongs_to_row(&drag.panel_key, row_identity)
            })
        }) {
            self.code_panel_resize_drag = None;
        }
        previous_soft_wrap_count != self.soft_wrapped_panel_keys.len()
            || previous_height_count != self.resized_panel_heights.len()
    }

    fn clear_syntax_highlight_cache_for_released_rows(&mut self, row_identities: &HashSet<String>) {
        self.syntax_highlight_cache
            .borrow_mut()
            .release_owners_matching(|panel_id| {
                row_identities.iter().any(|row_identity| {
                    markdown_code_panel_id_belongs_to_row(panel_id, row_identity)
                })
            });
    }

    fn clear_code_panel_projection_cache_for_released_rows(
        &mut self,
        row_identities: &HashSet<String>,
    ) {
        self.code_panel_projection_cache
            .borrow_mut()
            .release_owners_matching(|panel_id| {
                row_identities.iter().any(|row_identity| {
                    markdown_code_panel_id_belongs_to_row(panel_id, row_identity)
                })
            });
    }

    fn prune_code_panel_interaction_state(&mut self, protected_panel_ids: &HashSet<String>) {
        let mut retained_panel_ids = self
            .soft_wrapped_panel_keys
            .iter()
            .chain(self.resized_panel_heights.keys())
            .cloned()
            .collect::<HashSet<_>>();
        if retained_panel_ids.len() <= CODE_PANEL_INTERACTION_STATE_MAX_ENTRIES {
            return;
        }

        let mut removable_panel_ids = retained_panel_ids
            .iter()
            .filter(|panel_id| !protected_panel_ids.contains(*panel_id))
            .cloned()
            .collect::<Vec<_>>();
        removable_panel_ids.sort();
        for panel_id in removable_panel_ids {
            if retained_panel_ids.len() <= CODE_PANEL_INTERACTION_STATE_MAX_ENTRIES {
                break;
            }
            self.soft_wrapped_panel_keys.remove(&panel_id);
            self.resized_panel_heights.remove(&panel_id);
            retained_panel_ids.remove(&panel_id);
        }
        if retained_panel_ids.len() <= CODE_PANEL_INTERACTION_STATE_MAX_ENTRIES {
            return;
        }

        let mut retained_count = retained_panel_ids.len();
        let mut protected_panel_ids = retained_panel_ids.into_iter().collect::<Vec<_>>();
        protected_panel_ids.sort();
        for panel_id in protected_panel_ids {
            if retained_count <= CODE_PANEL_INTERACTION_STATE_MAX_ENTRIES {
                break;
            }
            let removed = self.soft_wrapped_panel_keys.remove(&panel_id)
                | self.resized_panel_heights.remove(&panel_id).is_some();
            if removed {
                retained_count -= 1;
            }
        }
    }

    fn scoped_soft_wrapped_panel_keys(
        &self,
        visible_panel_ids: &HashSet<String>,
    ) -> HashSet<String> {
        visible_panel_ids
            .iter()
            .filter(|panel_id| self.soft_wrapped_panel_keys.contains(*panel_id))
            .cloned()
            .collect()
    }

    fn scoped_resized_panel_heights(
        &self,
        visible_panel_ids: &HashSet<String>,
    ) -> HashMap<String, Pixels> {
        visible_panel_ids
            .iter()
            .filter_map(|panel_id| {
                self.resized_panel_heights
                    .get(panel_id)
                    .copied()
                    .map(|height| (panel_id.clone(), height))
            })
            .collect()
    }

    fn scoped_code_panel_scrollbar_visibility(
        &mut self,
        visible_panel_ids: &HashSet<String>,
    ) -> HashMap<String, ScrollbarVisibilityState> {
        visible_panel_ids
            .iter()
            .map(|panel_id| {
                let state = self
                    .code_panel_scrollbar_visibility
                    .entry(panel_id.clone())
                    .or_default()
                    .clone();
                (panel_id.clone(), state)
            })
            .collect()
    }

    fn record_layout_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.layout_bounds = Some(bounds);
    }

    fn code_panel_max_height(&self) -> Pixels {
        let available_height = self
            .layout_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or_else(|| px(TRANSCRIPT_CODE_PANEL_DEFAULT_MAX_HEIGHT));
        transcript_code_panel_max_height(available_height)
    }

    fn sync_markdown_cache_scope(
        &mut self,
        workspace: WorkspaceId,
        selected_thread_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let scope = TranscriptMarkdownCacheScope {
            workspace,
            selected_thread_id,
        };
        if self.markdown_cache_scope.as_ref() == Some(&scope) {
            return;
        }

        self.markdown_cache.borrow_mut().clear();
        self.syntax_highlight_cache.borrow_mut().clear();
        self.code_panel_projection_cache.borrow_mut().clear();
        let evicted_images = self.media_cache.borrow_mut().clear();
        self.release_evicted_media_images(evicted_images, cx);
        self.stream_projection.borrow_mut().clear();
        self.clear_code_panel_interaction_state();
        self.nested_scroll_ownership.clear_to_transcript();
        self.text_selection.clear();
        self.quote_popup.clear_selection();
        self.visible_text_frame.clear();
        self.next_visible_text_frame.clear();
        self.visible_media.borrow_mut().clear();
        self.visible_text_geometry.clear();
        self.next_visible_text_geometry.clear();
        self.visible_text_geometry_viewport_bounds = None;
        self.visible_text_hit_geometry.clear();
        self.next_visible_text_hit_geometry.clear();
        self.promoted_media = None;
        self.validated_image_menu_target = None;
        self.markdown_cache_scope = Some(scope);
    }

    fn sync_transcript_reset_generation(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.handled_transcript_reset_generation == generation {
            return;
        }

        self.handled_transcript_reset_generation = generation;
        let mut reset_event = MediaDiagnosticEvent::new("transcript_reset");
        reset_event.detail = Some(generation.to_string());
        self.media_events.borrow_mut().record(reset_event);
        self.markdown_cache.borrow_mut().clear();
        self.syntax_highlight_cache.borrow_mut().clear();
        self.code_panel_projection_cache.borrow_mut().clear();
        let evicted_images = self.media_cache.borrow_mut().clear();
        self.release_evicted_media_images(evicted_images, cx);
        self.stream_projection.borrow_mut().clear();
        self.clear_code_panel_interaction_state();
        self.nested_scroll_ownership.clear_to_transcript();
        self.text_selection.clear();
        self.quote_popup.clear_selection();
        self.visible_text_frame.clear();
        self.next_visible_text_frame.clear();
        self.visible_text_geometry.clear();
        self.next_visible_text_geometry.clear();
        self.visible_text_geometry_viewport_bounds = None;
        self.visible_text_hit_geometry.clear();
        self.next_visible_text_hit_geometry.clear();
        self.promoted_media = None;
        self.validated_image_menu_target = None;
    }

    fn sync_content_releases(
        &mut self,
        generation: u64,
        row_identities: &[String],
        cx: &mut Context<Self>,
    ) {
        if self.handled_content_release_generation == generation {
            return;
        }
        self.handled_content_release_generation = generation;
        if row_identities.is_empty() {
            return;
        }

        let mut release_event = MediaDiagnosticEvent::new("transcript_content_release");
        release_event.image_count = Some(row_identities.len());
        release_event.detail = Some(generation.to_string());
        self.media_events.borrow_mut().record(release_event);
        let evicted_images = self.media_cache.borrow_mut().clear();
        self.release_evicted_media_images(evicted_images, cx);
        self.visible_media.borrow_mut().clear();
        self.validated_image_menu_target = None;
        let row_identities = row_identities.iter().cloned().collect();
        self.clear_syntax_highlight_cache_for_released_rows(&row_identities);
        self.clear_code_panel_projection_cache_for_released_rows(&row_identities);
        let code_panel_changed =
            self.clear_code_panel_interaction_state_for_released_rows(&row_identities);
        let promoted_changed = self.clear_promoted_media_for_released_rows(&row_identities, cx);
        let selection_changed = self
            .text_selection
            .clear_if_intersects_row_identities(&row_identities);
        let popup_changed = selection_changed && self.quote_popup.clear_selection();
        if selection_changed || popup_changed || promoted_changed || code_panel_changed {
            cx.notify();
        }
    }

    fn sync_activity_caret_blink(
        &mut self,
        caret_present: bool,
        blink_interval: Option<Duration>,
        cx: &mut Context<Self>,
    ) {
        let motion = ActivityCaretMotion::for_blink_interval(blink_interval);
        let before_generation = self.activity_caret_blink.generation();
        self.activity_caret_blink.sync(caret_present, motion);
        if self.activity_caret_blink.generation() != before_generation {
            self.activity_caret_blink_task = None;
        }
        self.schedule_activity_caret_blink(cx);
    }

    fn schedule_activity_caret_blink(&mut self, cx: &mut Context<Self>) {
        let Some(schedule) = self.activity_caret_blink.blink_schedule() else {
            return;
        };
        if self.activity_caret_blink_task.is_some() {
            return;
        }

        let ActivityCaretBlinkSchedule {
            generation,
            interval,
        } = schedule;
        let blink_task = cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor().timer(interval).await;
                let _ = view.update(&mut cx, |view: &mut Self, cx: &mut Context<Self>| {
                    view.advance_activity_caret_blink(generation, cx);
                });
            }
        });
        self.activity_caret_blink_task = Some(blink_task);
    }

    fn advance_activity_caret_blink(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.activity_caret_blink_task = None;
        if self.activity_caret_blink.advance(generation) {
            cx.notify();
        }
        self.schedule_activity_caret_blink(cx);
    }

    fn note_code_panel_scrollbar_activity(&mut self, panel_key: String, cx: &mut Context<Self>) {
        if self
            .nested_scroll_ownership
            .record_scrollbar_activity(panel_key.as_str())
        {
            cx.notify();
        }
    }

    fn record_code_panel_scrollbar_visibility(
        &mut self,
        panel_key: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self
            .code_panel_scrollbar_visibility
            .entry(panel_key)
            .or_default()
            .clone();
        let on_update = Self::code_panel_scrollbar_update_callback(cx.entity());
        state.record_viewport_activity(window, cx, on_update);
    }

    pub(super) fn code_panel_scrollbar_update_callback(
        entity: Entity<Self>,
    ) -> ScrollbarVisibilityUpdateCallback {
        Rc::new(move |_: &mut Window, cx: &mut App| {
            entity.update(cx, |_, cx| {
                cx.notify();
            });
        })
    }

    fn scroll_selected_nested_code_panel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(panel_key) = self
            .nested_scroll_ownership
            .selected_panel_id()
            .map(str::to_owned)
        else {
            return false;
        };
        let Some(handle) = self
            .code_panel_scroll_handles
            .borrow()
            .get(panel_key.as_str())
            .cloned()
        else {
            return false;
        };
        if !handle.bounds().contains(&event.position) {
            return false;
        }

        let next_offset = code_panel::code_panel_offset_after_scroll_delta(
            handle.offset(),
            handle.max_offset(),
            event.delta.pixel_delta(window.line_height()),
        );
        handle.set_offset(next_offset);
        self.record_code_panel_scrollbar_visibility(panel_key.clone(), window, cx);
        self.note_code_panel_scrollbar_activity(panel_key, cx);
        true
    }

    fn release_evicted_media_images(&self, images: Vec<Arc<Image>>, cx: &mut App) {
        let image_count = images.len();
        for image in images {
            image.remove_asset(cx);
        }
        if image_count > 0 {
            let mut event = MediaDiagnosticEvent::new("gpui_media_images_released");
            event.image_count = Some(image_count);
            self.media_events.borrow_mut().record(event);
        }
    }
}

impl Render for TranscriptPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_image_preview_update(window, cx);
        let Some(snapshot) = self.shell.read(cx).transcript_panel_snapshot() else {
            self.current_workspace_id = None;
            self.visible_media.borrow_mut().clear();
            self.media_events
                .borrow_mut()
                .record(MediaDiagnosticEvent::new("transcript_panel_cleared"));
            let evicted_images = self.media_cache.borrow_mut().clear();
            self.release_evicted_media_images(evicted_images, cx);
            self.nested_scroll_ownership.clear_to_transcript();
            self.nested_code_panel_selected_during_mouse_down = false;
            self.text_selection.clear();
            self.quote_popup.clear_selection();
            self.image_preview_popup = None;
            self.image_preview_receiver = None;
            self.promoted_media = None;
            self.validated_image_menu_target = None;
            self.visible_text_frame.clear();
            self.next_visible_text_frame.clear();
            self.visible_text_geometry.clear();
            self.next_visible_text_geometry.clear();
            self.visible_text_geometry_viewport_bounds = None;
            self.visible_text_hit_geometry.clear();
            self.next_visible_text_hit_geometry.clear();
            self.memory_logged_transcript_reset_generation = 0;
            self.sync_activity_caret_blink(false, None, cx);
            let shell = self.shell.read(cx);
            return panel_shell(&shell, div().size_full().min_h(px(0.0))).into_any_element();
        };
        self.current_workspace_id = snapshot.workspace_id.clone();
        self.sync_markdown_cache_scope(
            snapshot.workspace.clone(),
            snapshot.selected_thread_id.clone(),
            cx,
        );
        self.sync_transcript_reset_generation(snapshot.transcript_reset_generation, cx);
        self.sync_content_releases(
            snapshot.content_release_generation,
            &snapshot.content_release_row_identities,
            cx,
        );
        self.begin_text_span_frame();

        let shell = self.shell.clone();
        let has_turns = shell
            .read(cx)
            .conversation_surface()
            .is_some_and(|surface| !surface.transcript_presentation().is_empty());
        let pending_thread_activation_label = snapshot.pending_thread_activation_label.clone();
        let has_pending_thread_activation = pending_thread_activation_label.is_some();
        let entity = cx.entity();
        let transcript_list_state = snapshot.transcript_list_state.clone();
        let media_promotion_state = TranscriptMediaPromotionState::new(self.promoted_media.clone());
        let image_menu_render_state = TranscriptImageMenuRenderState::new(
            shell.read(cx).conversation_surface().and_then(|surface| {
                surface
                    .transcript_branch_menu()
                    .active()
                    .and_then(|open| open.image_target().cloned())
            }),
        );
        let markdown_context =
            TranscriptMarkdownRenderContext::new(self.markdown_cache.clone(), entity.clone());
        let media_context = TranscriptMediaRenderContext::new(
            self.media_cache.clone(),
            self.media_events.clone(),
            self.visible_media.clone(),
            entity.clone(),
            shell.read(cx).backend_client_connector(),
            Duration::from_secs(5),
            media_promotion_state.clone(),
            image_menu_render_state.clone(),
        );
        let stream_projection_context =
            TranscriptStreamProjectionContext::new(self.stream_projection.clone());
        let turn_count = shell
            .read(cx)
            .conversation_surface()
            .map(|surface| surface.transcript_presentation().len())
            .unwrap_or_default();
        let presentation_range =
            transcript_frame_presentation_range(&transcript_list_state, turn_count);
        self.visible_media.borrow_mut().begin_frame(
            snapshot.selected_thread_id.clone(),
            presentation_range.clone(),
        );
        if memory_diagnostics::enabled()
            && snapshot.pending_thread_activation_label.is_none()
            && snapshot.selected_thread_id.is_some()
            && snapshot.transcript_reset_generation
                != self.memory_logged_transcript_reset_generation
        {
            self.memory_logged_transcript_reset_generation = snapshot.transcript_reset_generation;
            let markdown_stats = self.markdown_cache.borrow().stats();
            let media_stats = self.media_cache.borrow().stats();
            let stream_projection_counts = self.stream_projection.borrow().retained_counts();
            let mut retained_state = shell.read_with(cx, |shell, app| {
                let mut retained_state = shell.retained_state_snapshot();
                shell.add_text_input_retained_counts(&mut retained_state, app);
                retained_state
            });
            retained_state.presentation_range_rows = Some(presentation_range.len());
            retained_state.markdown_cache_entries = Some(markdown_stats.entries);
            retained_state.markdown_cache_pending_entries = Some(markdown_stats.pending_entries);
            retained_state.markdown_source_bytes = Some(markdown_stats.source_bytes);
            retained_state.markdown_estimated_retained_bytes =
                Some(markdown_stats.estimated_retained_bytes);
            retained_state.markdown_in_flight_source_bytes =
                Some(markdown_stats.in_flight_source_bytes);
            retained_state.markdown_displayed_source_bytes =
                Some(markdown_stats.displayed_source_bytes);
            retained_state.markdown_parsed_source_bytes = Some(markdown_stats.parsed_source_bytes);
            retained_state.markdown_estimated_structure_bytes =
                Some(markdown_stats.markdown_estimated_structure_bytes);
            retained_state.markdown_blocks = Some(markdown_stats.markdown_blocks);
            retained_state.markdown_inlines = Some(markdown_stats.markdown_inlines);
            retained_state.markdown_media_requests = Some(markdown_stats.markdown_media_requests);
            let syntax_highlight_stats = self.syntax_highlight_cache.borrow().stats();
            let code_panel_projection_stats = self.code_panel_projection_cache.borrow().stats();
            retained_state.syntax_highlight_cache_entries = Some(syntax_highlight_stats.entries);
            retained_state.syntax_highlight_represented_source_bytes =
                Some(syntax_highlight_stats.represented_source_bytes);
            retained_state.syntax_highlight_estimated_retained_bytes =
                Some(syntax_highlight_stats.estimated_retained_bytes);
            retained_state.syntax_highlight_tokens = Some(syntax_highlight_stats.tokens);
            retained_state.media_cache_entries = Some(media_stats.entries);
            retained_state.media_cache_pending_entries = Some(media_stats.pending_entries);
            retained_state.media_cache_loaded_entries = Some(media_stats.loaded_entries);
            retained_state.media_cache_loaded_retained_byte_entries =
                Some(media_stats.loaded_retained_byte_entries);
            retained_state.media_cache_loaded_source_backed_file_entries =
                Some(media_stats.loaded_source_backed_file_entries);
            retained_state.media_cache_loaded_native_generated_source_backed_file_entries =
                Some(media_stats.loaded_native_generated_source_backed_file_entries);
            retained_state.media_cache_loaded_native_generated_retained_byte_entries =
                Some(media_stats.loaded_native_generated_retained_byte_entries);
            retained_state.media_cache_loaded_image_bytes = Some(media_stats.loaded_image_bytes);
            retained_state.media_cache_decoded_image_bytes_estimate =
                Some(media_stats.decoded_image_bytes_estimate);
            retained_state.media_cache_thumbnail_count = Some(media_stats.thumbnail_count);
            retained_state.stream_projection_entries = Some(stream_projection_counts.entries);
            retained_state.stream_projection_key_bytes = Some(stream_projection_counts.key_bytes);
            retained_state.stream_projection_text_bytes = Some(stream_projection_counts.text_bytes);
            retained_state.stream_projection_uncommitted_entries =
                Some(stream_projection_counts.uncommitted_entries);
            if let Some(total) = retained_state.retained_payload_bytes_lower_bound.as_mut() {
                *total = total
                    .saturating_add(markdown_stats.estimated_retained_bytes)
                    .saturating_add(syntax_highlight_stats.estimated_retained_bytes)
                    .saturating_add(code_panel_projection_stats.estimated_retained_bytes)
                    .saturating_add(media_stats.loaded_image_bytes)
                    .saturating_add(media_stats.decoded_image_bytes_estimate)
                    .saturating_add(stream_projection_counts.key_bytes)
                    .saturating_add(stream_projection_counts.text_bytes);
            }
            let mut milestone = MemoryMilestone::new("first_transcript_render_after_reset")
                .runtime(snapshot.workspace.runtime_mode().display_name())
                .turn_count(turn_count)
                .retained_state(retained_state);
            if let Some(workspace_id) = snapshot.workspace_id.as_ref() {
                milestone = milestone.workspace_id(workspace_id.as_str());
            }
            if let Some(thread_id) = snapshot.selected_thread_id.as_ref() {
                milestone = milestone.thread_id(thread_id.as_str());
            }
            milestone.log();
        }
        let panel_state = shell
            .read(cx)
            .conversation_surface()
            .map(|surface| {
                surface
                    .transcript_presentation()
                    .panel_state_for_range(presentation_range.clone())
            })
            .unwrap_or_default();
        let presentation_range_len = presentation_range.len();
        let panel_state_inspected_row_count = panel_state.inspected_row_count;
        let mut active_nested_code_panel_ids = panel_state.active_nested_code_panel_ids;
        let visible_rows = shell
            .read(cx)
            .conversation_surface()
            .map(|surface| {
                let presentation = surface.transcript_presentation();
                presentation_range
                    .clone()
                    .filter_map(|index| presentation.turn_at(index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for row in visible_rows {
            active_nested_code_panel_ids.extend(collect_turn_card_markdown_code_panel_ids(
                row.index,
                row.turn.as_ref(),
                row.identity.as_str(),
                markdown_context.clone(),
                stream_projection_context.clone(),
                cx,
            ));
        }
        self.retain_nested_code_panel_selection(
            active_nested_code_panel_ids.iter().map(String::as_str),
        );
        self.retain_visible_code_panel_scroll_state(&active_nested_code_panel_ids);
        self.retain_visible_syntax_highlight_cache(&active_nested_code_panel_ids);
        self.retain_visible_code_panel_projection_cache(&active_nested_code_panel_ids);
        self.prune_code_panel_interaction_state(&active_nested_code_panel_ids);
        let selected_nested_code_panel_id = Arc::new(
            self.nested_scroll_ownership
                .selected_panel_id()
                .map(str::to_string),
        );
        let soft_wrapped_panel_keys =
            Arc::new(self.scoped_soft_wrapped_panel_keys(&active_nested_code_panel_ids));
        let resized_panel_heights =
            Arc::new(self.scoped_resized_panel_heights(&active_nested_code_panel_ids));
        let code_panel_scroll_handles = self.code_panel_scroll_handles.clone();
        let code_panel_scrollbar_visibility =
            Arc::new(self.scoped_code_panel_scrollbar_visibility(&active_nested_code_panel_ids));
        let syntax_highlight_cache = self.syntax_highlight_cache.clone();
        let code_panel_projection_cache = self.code_panel_projection_cache.clone();
        let transcript_panel_height = self
            .layout_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or_else(|| px(TRANSCRIPT_CODE_PANEL_DEFAULT_MAX_HEIGHT));
        let code_layout = TranscriptCodeLayout::for_transcript_size(
            snapshot.transcript_width,
            transcript_panel_height,
            window,
        );
        let appearance = Arc::new(snapshot.appearance.clone());
        let media_layout =
            transcript_media_layout(snapshot.transcript_width, appearance.as_ref(), window);
        let selection_order = Rc::new(Cell::new(0usize));
        let narrative_copy_block_count = Rc::new(Cell::new(0usize));
        let trailing_scroll_allowance = snapshot
            .submit_anchor
            .as_ref()
            .and_then(|anchor| {
                let anchor_turn = shell.read(cx).conversation_surface().and_then(|surface| {
                    surface
                        .transcript_presentation()
                        .turn_at(anchor.turn_index)
                        .map(|row| row.turn)
                });
                anchor_turn.map(|turn| {
                    let prompt_block_path = user_prompt_block_path(anchor.fragment_index);
                    let prompt_key =
                        turn_markdown_key(anchor.turn_index, turn.as_ref(), &prompt_block_path);
                    let prompt_markdown_source = turn
                        .user_input_fragments()
                        .get(anchor.fragment_index)
                        .map(|fragment| {
                            markdown_source_with_image_marker_placeholders(
                                fragment.text.as_str(),
                                fragment.image_markers(),
                            )
                        })
                        .unwrap_or_else(|| anchor.user_input.clone());
                    let prompt_markdown =
                        markdown_context.markdown_for(prompt_key, &prompt_markdown_source, cx);
                    let preceding_markdown = turn
                        .user_input_fragments()
                        .iter()
                        .take(anchor.fragment_index)
                        .enumerate()
                        .filter(|(_, fragment)| !fragment.text.is_empty())
                        .map(|(fragment_index, fragment)| {
                            let block_path = user_prompt_block_path(fragment_index);
                            let key =
                                turn_markdown_key(anchor.turn_index, turn.as_ref(), &block_path);
                            let source = markdown_source_with_image_marker_placeholders(
                                fragment.text.as_str(),
                                fragment.image_markers(),
                            );
                            markdown_context.markdown_for(key, &source, cx)
                        })
                        .collect::<Vec<_>>();
                    let preceding_plans = preceding_markdown
                        .iter()
                        .map(|markdown| markdown.render_plan())
                        .collect::<Vec<_>>();
                    let offset = transcript_anchor::prompt_last_line_top_offset(
                        anchor,
                        preceding_plans.as_slice(),
                        prompt_markdown.render_plan(),
                        snapshot.transcript_width,
                        appearance.as_ref(),
                        code_layout.transcript_bordered_panel_columns,
                        window,
                    );
                    if anchor.force_viewport {
                        transcript_list_state.scroll_to(ListOffset {
                            item_ix: anchor.turn_index,
                            offset_in_item: offset,
                        });
                    }
                    let measured_content_below_anchor = transcript_list_state
                        .measured_item_size(anchor.turn_index)
                        .map(|size| (size.height - offset).max(px(0.0)));
                    transcript_anchor::trailing_scroll_slack(
                        transcript_panel_height,
                        measured_content_below_anchor,
                    )
                })
            })
            .unwrap_or_else(|| px(0.0));
        transcript_list_state.set_virtual_trailing_scroll_allowance(trailing_scroll_allowance);
        transcript_list_state.set_scroll_handler({
            let shell = shell.clone();
            move |event, window, cx| {
                shell.update(cx, |view, cx| {
                    view.release_transcript_submit_anchor(cx);
                    view.note_transcript_scroll_event(event, window, cx);
                });
            }
        });
        let profiler = TranscriptFrameProfile::enabled(
            snapshot.metrics,
            snapshot.selected_thread_id.clone(),
            presentation_range_len,
            panel_state_inspected_row_count,
            self.markdown_cache.borrow().stats(),
        );
        let workspace = snapshot.workspace.clone();
        let loaded_history_anchor_pending = snapshot.loaded_history_anchor_pending;
        let older_history_loading = snapshot.older_history_loading;
        let activity_caret = snapshot.activity_caret.clone();
        let transcript_edit_mode = snapshot.transcript_edit_mode.clone();
        self.sync_activity_caret_blink(
            activity_caret.is_some(),
            platform_caret_blink_interval(),
            cx,
        );
        let activity_caret_opacity = self.activity_caret_blink.opacity();
        let scrollbar_visibility = self.shell.read(cx).scrollbar_visibility_policy_for_entity(
            &ScrollbarRegion::Transcript,
            self.shell.clone(),
        );

        div()
            .relative()
            .size_full()
            .min_h(px(0.0))
            .key_context(TRANSCRIPT_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::copy_text_selection_action))
            .on_action(cx.listener(Self::clear_text_selection_action))
            .child(
                canvas(|bounds, _, _| bounds, {
                    let entity = entity.clone();
                    move |bounds, _, window, cx| {
                        entity.update(cx, |view, _| view.record_layout_bounds(bounds));
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |event: &MouseDownEvent, phase, _, cx| {
                                if phase != DispatchPhase::Capture {
                                    return;
                                }

                                entity.update(cx, |view, cx| {
                                    view.handle_image_preview_popup_mouse_down(event, cx);
                                });
                            }
                        });
                        window.on_key_event({
                            let entity = entity.clone();
                            move |event: &KeyDownEvent, phase, _, cx| {
                                if phase != DispatchPhase::Bubble {
                                    return;
                                }

                                let handled = entity.update(cx, |view, cx| {
                                    view.handle_image_preview_popup_key_down(event, cx)
                                });
                                if handled {
                                    cx.stop_propagation();
                                }
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |event: &MouseMoveEvent, _, _, cx| {
                                if !event.dragging() {
                                    return;
                                }

                                entity.update(cx, |view, cx| {
                                    view.update_code_panel_resize(event, cx);
                                    view.update_text_selection(event, cx);
                                });
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |_event: &MouseUpEvent, _, _, cx| {
                                entity.update(cx, |view, cx| {
                                    view.end_code_panel_resize(cx);
                                    view.end_text_selection(cx);
                                });
                            }
                        });
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(panel_shell(
                &self.shell.read(cx),
                div().size_full().min_h(px(0.0)).flex().flex_col().child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex()
                        .flex_col()
                        .when_some(pending_thread_activation_label, |this, label| {
                            this.child(
                                div()
                                    .px_3()
                                    .pt_4()
                                    .pb_3()
                                    .child(pending_thread_activation_state(&label)),
                            )
                        })
                        .when(!has_turns && !has_pending_thread_activation, |this| {
                            this.child(
                                div()
                                    .px_3()
                                    .py_4()
                                    .child(empty_state(snapshot.selected_thread_present)),
                            )
                        })
                        .when(has_turns && older_history_loading, |this| {
                            this.child(
                                div()
                                    .px_3()
                                    .pt_3()
                                    .pb_2()
                                    .child(older_history_loading_state()),
                            )
                        })
                        .when(has_turns, |this| {
                            this.child({
                                let row_entity = entity.clone();
                                let row_selected_nested_code_panel_id =
                                    selected_nested_code_panel_id.clone();
                                let mut scroll_region = div()
                                    .relative()
                                    .w_full()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .on_mouse_move({
                                        let shell = shell.clone();
                                        move |_, window, cx| {
                                            shell.update(cx, |view, cx| {
                                                view.note_scrollbar_activity(
                                                    ScrollbarRegion::Transcript,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    })
                                    .on_scroll_wheel({
                                        let shell = shell.clone();
                                        move |_, window, cx| {
                                            shell.update(cx, |view, cx| {
                                                view.note_scrollbar_activity(
                                                    ScrollbarRegion::Transcript,
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }
                                    })
                                    .on_mouse_down(MouseButton::Left, {
                                        let entity = entity.clone();
                                        move |event, window, cx| {
                                            let handled = entity.update(cx, |view, cx| {
                                                view.handle_transcript_mouse_down(event, window, cx)
                                            });
                                            if handled {
                                                cx.stop_propagation();
                                            }
                                        }
                                    })
                                    .on_mouse_up(MouseButton::Left, {
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |view, cx| {
                                                view.end_text_selection(cx);
                                            });
                                        }
                                    })
                                    .on_mouse_up_out(MouseButton::Left, {
                                        let entity = entity.clone();
                                        move |_, _, cx| {
                                            entity.update(cx, |view, cx| {
                                                view.end_text_selection(cx);
                                            });
                                        }
                                    })
                                    .on_children_prepainted({
                                        let profiler = profiler.clone();
                                        let shell = shell.clone();
                                        let entity = entity.clone();
                                        let markdown_cache = self.markdown_cache.clone();
                                        let media_promotion_state = media_promotion_state.clone();
                                        let image_menu_render_state =
                                            image_menu_render_state.clone();
                                        let transcript_list_state = transcript_list_state.clone();
                                        let preload_workspace = workspace.clone();
                                        let preload_media_context = media_context.clone();
                                        let preload_markdown_context = markdown_context.clone();
                                        let preload_stream_projection_context =
                                            stream_projection_context.clone();
                                        let preload_media_layout = media_layout;
                                        move |_, window, cx| {
                                            let viewport_bounds =
                                                transcript_list_state.viewport_bounds();
                                            entity.update(cx, |view, cx| {
                                                view.finish_text_span_frame(viewport_bounds, cx);
                                                let preload_range = transcript_frame_preload_range(
                                                    &transcript_list_state,
                                                    turn_count,
                                                    viewport_bounds.size.height * 0.5,
                                                );
                                                view.preload_transcript_media_range(
                                                    preload_range,
                                                    &preload_workspace,
                                                    preload_media_context.clone(),
                                                    preload_markdown_context.clone(),
                                                    preload_stream_projection_context.clone(),
                                                    preload_media_layout,
                                                    window,
                                                    cx,
                                                );
                                                if let Some(target) = image_menu_render_state
                                                    .rendered_loaded_target()
                                                {
                                                    view.validate_transcript_image_menu_target(
                                                        &target,
                                                    );
                                                } else if image_menu_render_state
                                                    .target_not_rendered_loaded()
                                                {
                                                    view.clear_validated_transcript_image_menu_target();
                                                }
                                                if media_promotion_state
                                                    .rendered_target_row_without_identity()
                                                {
                                                    let changed = view.clear_promoted_media(cx);
                                                    if changed {
                                                        cx.notify();
                                                    }
                                                }
                                            });
                                            if image_menu_render_state.target_not_rendered_loaded()
                                            {
                                                shell.update(cx, |view, cx| {
                                                    view.clear_stale_transcript_image_menu_target(
                                                        cx,
                                                    );
                                                });
                                            }
                                            if let Some(profiler) = profiler.as_ref() {
                                                profiler
                                                    .log_if_slow(markdown_cache.borrow().stats());
                                            }
                                            if loaded_history_anchor_pending {
                                                let shell = shell.clone();
                                                cx.defer(move |cx| {
                                                    shell.update(cx, |view, cx| {
                                                        view.install_loaded_history_transcript_anchor(cx);
                                                    });
                                                });
                                            }
                                        }
                                    })
                                    .child({
                                        let row_shell = shell.clone();
                                        list(transcript_list_state.clone(), move |index, _, cx| {
                                            let row_started = Instant::now();
                                            if index >= turn_count {
                                                return div().into_any_element();
                                            }

                                            let row = row_shell
                                                .read(cx)
                                                .conversation_surface()
                                                .and_then(|surface| {
                                                    surface
                                                        .transcript_presentation()
                                                        .turn_at(index)
                                                });

                                            let turn_text_chars = row
                                                .as_ref()
                                                .map_or(0, |row| row.turn.text_char_count());
                                            let turn_item_count = row
                                                .as_ref()
                                                .map_or(0, |row| row.turn.item_count());
                                            let placeholder_height =
                                                row.as_ref().and_then(|row| row.placeholder_height);
                                            let workspace = workspace.clone();
                                            let appearance = appearance.clone();
                                            let soft_wrapped_panel_keys =
                                                soft_wrapped_panel_keys.clone();
                                            let resized_panel_heights =
                                                resized_panel_heights.clone();
                                            let code_panel_scroll_handles =
                                                code_panel_scroll_handles.clone();
                                            let code_panel_scrollbar_visibility =
                                                code_panel_scrollbar_visibility.clone();
                                            let selected_nested_code_panel_id =
                                                row_selected_nested_code_panel_id.clone();
                                            let syntax_highlight_cache =
                                                syntax_highlight_cache.clone();
                                            let code_panel_projection_cache =
                                                code_panel_projection_cache.clone();
                                            let code_layout = code_layout;
                                            let media_layout = media_layout;
                                            let markdown_context = markdown_context.clone();
                                            let media_context = media_context.clone();
                                            let stream_projection_context =
                                                stream_projection_context.clone();
                                            let selection_order = selection_order.clone();
                                            let narrative_copy_block_count =
                                                narrative_copy_block_count.clone();
                                            let activity_caret = activity_caret.clone();
                                            let transcript_edit_mode = transcript_edit_mode.clone();
                                            let activity_caret_opacity = activity_caret_opacity;
                                            let element = row.map_or_else(
                                                || div().into_any_element(),
                                                |row| {
                                                    debug_assert_eq!(row.index, index);
                                                    let row_identity =
                                                        row.identity.as_str().to_string();
                                                    let media_context =
                                                        media_context.for_row(row_identity.clone());
                                                    let show_activity_caret =
                                                        activity_caret.as_ref().is_some_and(
                                                            |caret| {
                                                                caret.row_index == index
                                                                    && caret
                                                                        .row_identity
                                                                        .as_str()
                                                                        == row_identity.as_str()
                                                            },
                                                        );
                                                    let dimmed_for_edit = transcript_edit_mode
                                                        .as_ref()
                                                        .is_some_and(|edit| {
                                                            edit.dims_row(
                                                                row.turn.thread_id.as_deref(),
                                                                row.source_turn_index,
                                                            )
                                                        });
                                                    render_turn(
                                                        index,
                                                        &workspace,
                                                        appearance,
                                                        row.turn,
                                                        placeholder_height,
                                                        row_entity.clone(),
                                                        row_identity,
                                                        markdown_context,
                                                        media_context,
                                                        stream_projection_context,
                                                        soft_wrapped_panel_keys,
                                                        resized_panel_heights,
                                                        code_panel_scroll_handles,
                                                        code_panel_scrollbar_visibility,
                                                        selected_nested_code_panel_id,
                                                        syntax_highlight_cache,
                                                        code_panel_projection_cache,
                                                        code_layout,
                                                        media_layout,
                                                        selection_order,
                                                        narrative_copy_block_count,
                                                        show_activity_caret,
                                                        dimmed_for_edit,
                                                        activity_caret_opacity,
                                                        profiler.clone(),
                                                        cx,
                                                    )
                                                    .into_any_element()
                                                },
                                            );

                                            if let Some(profiler) = profiler.as_ref() {
                                                profiler.observe_turn(
                                                    index,
                                                    turn_text_chars,
                                                    turn_item_count,
                                                    row_started.elapsed(),
                                                );
                                            }

                                            element
                                        })
                                        .size_full()
                                    });
                                let bounds = transcript_list_state.viewport_bounds();
                                let max_offset = transcript_list_state.max_offset_for_scrollbar();
                                let offset = transcript_list_state.scroll_px_offset_for_scrollbar();
                                scroll_region = scroll_region
                                    .child(self.render_selected_text_highlights(entity.clone()));
                                if let Some(quote_popup) =
                                    self.render_quote_popup(bounds, entity.clone())
                                {
                                    scroll_region = scroll_region.child(quote_popup);
                                }
                                if let Some(image_popup) =
                                    self.render_image_preview_popup(entity.clone())
                                {
                                    scroll_region = scroll_region.child(image_popup);
                                }
                                let scrollbar_owner_update = {
                                    let shell = shell.clone();
                                    let transcript_list_state = transcript_list_state.clone();
                                    move |window: &mut Window, cx: &mut App| {
                                        let is_scrolled = !matches!(
                                            transcript_list_state.scroll_position(),
                                            ListScrollPosition::Bottom
                                        );
                                        let event = ListScrollEvent {
                                            visible_range: transcript_list_state.visible_range(),
                                            count: transcript_list_state.item_count(),
                                            is_scrolled,
                                        };
                                        shell.update(cx, |view, cx| {
                                            view.release_transcript_submit_anchor(cx);
                                            view.note_transcript_scroll_event(&event, window, cx);
                                        });
                                    }
                                };
                                let scrollbar_interaction = ScrollbarInteraction::new(
                                    {
                                        let transcript_list_state = transcript_list_state.clone();
                                        move || {
                                            Some(ScrollbarScrollState {
                                                viewport_bounds: transcript_list_state
                                                    .viewport_bounds(),
                                                max_offset: transcript_list_state
                                                    .max_offset_for_scrollbar(),
                                                scroll_offset: {
                                                    let offset = transcript_list_state
                                                        .scroll_px_offset_for_scrollbar();
                                                    point(px(0.0), -offset.y)
                                                },
                                            })
                                        }
                                    },
                                    {
                                        let transcript_list_state = transcript_list_state.clone();
                                        move |scroll_offset| {
                                            transcript_list_state.set_offset_from_scrollbar(point(
                                                px(0.0),
                                                -scroll_offset,
                                            ));
                                        }
                                    },
                                    {
                                        let transcript_list_state = transcript_list_state.clone();
                                        move |direction, distance| {
                                            let distance = match direction {
                                                ScrollDirection::Backward => -distance,
                                                ScrollDirection::Forward => distance,
                                            };
                                            transcript_list_state.scroll_by(distance);
                                        }
                                    },
                                    {
                                        let transcript_list_state = transcript_list_state.clone();
                                        move || {
                                            transcript_list_state.scrollbar_drag_started();
                                        }
                                    },
                                    {
                                        let transcript_list_state = transcript_list_state.clone();
                                        move || {
                                            transcript_list_state.scrollbar_drag_ended();
                                        }
                                    },
                                    scrollbar_owner_update,
                                );
                                if let Some(scrollbar) = render_interactive_vertical_scrollbar(
                                    "transcript-scrollbar",
                                    bounds.size.height,
                                    max_offset.height,
                                    -offset.y,
                                    scrollbar_visibility.clone(),
                                    scrollbar_interaction,
                                ) {
                                    scroll_region = scroll_region.child(scrollbar);
                                }
                                if selected_nested_code_panel_id.is_some() {
                                    scroll_region = scroll_region.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .size_full()
                                            .on_scroll_wheel({
                                                let entity = entity.clone();
                                                move |event, window, cx| {
                                                    let consumed = entity.update(cx, |view, cx| {
                                                        view.scroll_selected_nested_code_panel(
                                                            event, window, cx,
                                                        )
                                                    });
                                                    if consumed {
                                                        cx.stop_propagation();
                                                    }
                                                }
                                            }),
                                    );
                                }
                                scroll_region
                            })
                        }),
                ),
            ))
            .into_any_element()
    }
}

fn render_turn(
    index: usize,
    workspace: &WorkspaceId,
    appearance: Arc<AppearanceSettings>,
    turn: Arc<TurnExecutionRecord>,
    placeholder_height: Option<Pixels>,
    entity: Entity<TranscriptPanel>,
    row_identity: String,
    markdown_context: TranscriptMarkdownRenderContext,
    media_context: TranscriptMediaRenderContext,
    stream_projection_context: TranscriptStreamProjectionContext,
    soft_wrapped_panel_keys: Arc<HashSet<String>>,
    resized_panel_heights: Arc<HashMap<String, Pixels>>,
    code_panel_scroll_handles: Rc<RefCell<HashMap<String, ScrollHandle>>>,
    code_panel_scrollbar_visibility: Arc<HashMap<String, ScrollbarVisibilityState>>,
    selected_nested_code_panel_id: Arc<Option<String>>,
    syntax_highlight_cache: Rc<RefCell<SyntaxHighlightCache>>,
    code_panel_projection_cache: Rc<RefCell<CodePanelProjectionCache>>,
    code_layout: TranscriptCodeLayout,
    media_layout: TranscriptMediaRenderLayout,
    selection_order: Rc<Cell<usize>>,
    narrative_copy_block_count: Rc<Cell<usize>>,
    show_activity_caret: bool,
    dimmed_for_edit: bool,
    activity_caret_opacity: f32,
    profiler: Option<Rc<TranscriptFrameProfile>>,
    cx: &mut gpui::App,
) -> AnyElement {
    let row = div()
        .w_full()
        .px_3()
        .pb_3()
        .when(dimmed_for_edit, |this| this.opacity(0.48))
        .when(index == 0, |this| this.pt_4());

    if turn.is_released_history_placeholder() {
        let height = placeholder_height.unwrap_or_else(|| px(96.0)).max(px(64.0));
        return row
            .on_mouse_down(MouseButton::Right, {
                let entity = entity.clone();
                move |event, window, cx| {
                    let handled = entity.update(cx, |view, cx| {
                        view.handle_transcript_turn_context_mouse_down(index, event, window, cx)
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .h(height)
            .overflow_hidden()
            .child(released_history_placeholder_state())
            .into_any_element();
    }

    row.on_mouse_down(MouseButton::Right, {
        let entity = entity.clone();
        move |event, window, cx| {
            let handled = entity.update(cx, |view, cx| {
                view.handle_transcript_turn_context_mouse_down(index, event, window, cx)
            });
            if handled {
                cx.stop_propagation();
            }
        }
    })
    .when_some(profiler, |this, profiler| {
        let row_started = Instant::now();
        this.on_children_prepainted(move |_, _, _| {
            profiler.observe_turn_prepaint(index, row_started.elapsed());
        })
    })
    .child(render_turn_card(
        index,
        workspace,
        appearance,
        turn,
        TranscriptCodePanelState::new(
            entity,
            soft_wrapped_panel_keys,
            resized_panel_heights,
            code_panel_scroll_handles,
            code_panel_scrollbar_visibility,
            selected_nested_code_panel_id,
            syntax_highlight_cache,
            code_panel_projection_cache,
        ),
        markdown_context,
        media_context,
        stream_projection_context,
        code_layout,
        media_layout,
        row_identity.as_str(),
        selection_order,
        narrative_copy_block_count,
        show_activity_caret,
        activity_caret_opacity,
        cx,
    ))
    .into_any_element()
}

fn transcript_code_panel_max_height(transcript_height: Pixels) -> Pixels {
    let proportional_max = (transcript_height * TRANSCRIPT_CODE_PANEL_MAX_HEIGHT_RATIO)
        .max(px(TRANSCRIPT_CODE_PANEL_MIN_HEIGHT));
    proportional_max.min(px(TRANSCRIPT_CODE_PANEL_DEFAULT_MAX_HEIGHT))
}

fn transcript_media_layout(
    transcript_width: Pixels,
    appearance: &AppearanceSettings,
    window: &Window,
) -> TranscriptMediaRenderLayout {
    let metrics = transcript_media_layout_metrics(TranscriptMediaLayoutInput {
        transcript_width,
        row_horizontal_padding: px(TURN_ROW_HORIZONTAL_PADDING),
        conversation_m_advance: conversation_m_advance(appearance, window),
        window_scale: window.scale_factor(),
    });
    TranscriptMediaRenderLayout {
        padded_content_width: metrics.padded_content_width,
        conversation_m_advance: metrics.conversation_m_advance,
        window_scale: metrics.window_scale,
    }
}

fn conversation_m_advance(appearance: &AppearanceSettings, window: &Window) -> Pixels {
    let role = &appearance.conversation_text;
    let font = Font {
        family: SharedString::from(role.font_family.clone()),
        features: Default::default(),
        fallbacks: None,
        weight: FontWeight(role.font_weight as f32),
        style: FontStyle::Normal,
    };
    let run = TextRun {
        len: "M".len(),
        font,
        color: window.text_style().color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line("M".into(), px(role.font_size), &[run], None)
        .width
        .max(px(1.0))
}

fn selection_offset_to_display_offset(
    offset: usize,
    geometry: &TranscriptTextLineGeometry,
) -> usize {
    offset.min(geometry.display_text_len)
}

pub(super) fn turn_markdown_key(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    slot: &str,
) -> TranscriptMarkdownCacheKey {
    TranscriptMarkdownCacheKey::new(format!("{}:{slot}", turn_identity(turn_index, turn)))
}

pub(super) fn item_markdown_key(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
) -> TranscriptMarkdownCacheKey {
    TranscriptMarkdownCacheKey::new(format!(
        "{}:item:{item_id}:{slot}",
        turn_identity(turn_index, turn)
    ))
}

pub(super) fn indexed_item_markdown_key(
    turn_index: usize,
    turn: &TurnExecutionRecord,
    item_id: &str,
    slot: &str,
    item_index: usize,
) -> TranscriptMarkdownCacheKey {
    TranscriptMarkdownCacheKey::new(format!(
        "{}:item:{item_id}:{slot}:{item_index}",
        turn_identity(turn_index, turn)
    ))
}

fn turn_identity(turn_index: usize, turn: &TurnExecutionRecord) -> String {
    match (turn.thread_id.as_deref(), turn.turn_id.as_deref()) {
        (Some(thread_id), Some(turn_id)) => format!("thread:{thread_id}:turn:{turn_id}"),
        (Some(thread_id), None) => format!("thread:{thread_id}:turn-index:{turn_index}"),
        (None, Some(turn_id)) => format!("pending-thread:turn:{turn_id}"),
        (None, None) => format!("pending-thread:turn-index:{turn_index}"),
    }
}

struct TranscriptFrameProfile {
    started_at: Instant,
    total_turns: usize,
    total_item_count: usize,
    total_text_chars: usize,
    selected_thread_id: Option<String>,
    presentation_range_len: usize,
    panel_state_inspected_row_count: usize,
    markdown_cache_start: TranscriptMarkdownCacheStats,
    rendered_turn_count: Cell<usize>,
    largest_visible_turn_text_chars: Cell<usize>,
    largest_visible_turn_text_chars_index: Cell<usize>,
    largest_visible_turn_item_count: Cell<usize>,
    largest_visible_turn_item_count_index: Cell<usize>,
    total_turn_build: Cell<Duration>,
    slowest_turn_build: Cell<Duration>,
    slowest_turn_index: Cell<usize>,
    total_turn_prepaint: Cell<Duration>,
    slowest_turn_prepaint: Cell<Duration>,
    slowest_turn_prepaint_index: Cell<usize>,
}

impl TranscriptFrameProfile {
    fn enabled(
        metrics: Option<TranscriptRenderMetrics>,
        selected_thread_id: Option<String>,
        presentation_range_len: usize,
        panel_state_inspected_row_count: usize,
        markdown_cache_start: TranscriptMarkdownCacheStats,
    ) -> Option<Rc<Self>> {
        metrics.and_then(|metrics| {
            tracing::enabled!(Level::DEBUG).then(|| {
                Rc::new(Self {
                    started_at: Instant::now(),
                    total_turns: metrics.total_turns,
                    total_item_count: metrics.total_item_count,
                    total_text_chars: metrics.total_text_chars,
                    selected_thread_id,
                    presentation_range_len,
                    panel_state_inspected_row_count,
                    markdown_cache_start,
                    rendered_turn_count: Cell::new(0),
                    largest_visible_turn_text_chars: Cell::new(0),
                    largest_visible_turn_text_chars_index: Cell::new(0),
                    largest_visible_turn_item_count: Cell::new(0),
                    largest_visible_turn_item_count_index: Cell::new(0),
                    total_turn_build: Cell::new(Duration::ZERO),
                    slowest_turn_build: Cell::new(Duration::ZERO),
                    slowest_turn_index: Cell::new(0),
                    total_turn_prepaint: Cell::new(Duration::ZERO),
                    slowest_turn_prepaint: Cell::new(Duration::ZERO),
                    slowest_turn_prepaint_index: Cell::new(0),
                })
            })
        })
    }

    fn observe_turn(&self, index: usize, text_chars: usize, item_count: usize, elapsed: Duration) {
        self.rendered_turn_count
            .set(self.rendered_turn_count.get() + 1);
        self.total_turn_build
            .set(self.total_turn_build.get().saturating_add(elapsed));

        if text_chars > self.largest_visible_turn_text_chars.get() {
            self.largest_visible_turn_text_chars.set(text_chars);
            self.largest_visible_turn_text_chars_index.set(index);
        }
        if item_count > self.largest_visible_turn_item_count.get() {
            self.largest_visible_turn_item_count.set(item_count);
            self.largest_visible_turn_item_count_index.set(index);
        }

        if elapsed > self.slowest_turn_build.get() {
            self.slowest_turn_build.set(elapsed);
            self.slowest_turn_index.set(index);
        }
    }

    fn observe_turn_prepaint(&self, index: usize, elapsed: Duration) {
        self.total_turn_prepaint
            .set(self.total_turn_prepaint.get().saturating_add(elapsed));

        if elapsed > self.slowest_turn_prepaint.get() {
            self.slowest_turn_prepaint.set(elapsed);
            self.slowest_turn_prepaint_index.set(index);
        }
    }

    fn log_if_slow(&self, markdown_cache_stats: TranscriptMarkdownCacheStats) {
        let frame_elapsed = self.started_at.elapsed();
        let slowest_turn_build = self.slowest_turn_build.get();
        let slowest_turn_prepaint = self.slowest_turn_prepaint.get();
        if frame_elapsed < SLOW_TRANSCRIPT_FRAME_THRESHOLD
            && slowest_turn_build < SLOW_TRANSCRIPT_TURN_BUILD_THRESHOLD
            && slowest_turn_prepaint < SLOW_TRANSCRIPT_TURN_BUILD_THRESHOLD
        {
            return;
        }

        let markdown_delta = markdown_cache_stats.counter_delta_since(self.markdown_cache_start);
        debug!(
            selected_thread_id = self.selected_thread_id.as_deref().unwrap_or("<new-thread>"),
            transcript_turns = self.total_turns,
            total_loaded_turn_count = self.total_turns,
            transcript_items = self.total_item_count,
            transcript_text_chars = self.total_text_chars,
            presentation_range_len = self.presentation_range_len,
            panel_state_inspected_row_count = self.panel_state_inspected_row_count,
            visible_turn_renders = self.rendered_turn_count.get(),
            visible_rendered_turn_count = self.rendered_turn_count.get(),
            largest_visible_row_text_chars = self.largest_visible_turn_text_chars.get(),
            largest_visible_row_text_chars_index = self.largest_visible_turn_text_chars_index.get(),
            largest_visible_row_item_count = self.largest_visible_turn_item_count.get(),
            largest_visible_row_item_count_index = self.largest_visible_turn_item_count_index.get(),
            frame_ms = frame_elapsed.as_secs_f64() * 1000.0,
            turn_build_total_ms = self.total_turn_build.get().as_secs_f64() * 1000.0,
            turn_prepaint_total_ms = self.total_turn_prepaint.get().as_secs_f64() * 1000.0,
            slowest_turn_build_ms = slowest_turn_build.as_secs_f64() * 1000.0,
            slowest_turn_index = self.slowest_turn_index.get(),
            slowest_turn_prepaint_ms = slowest_turn_prepaint.as_secs_f64() * 1000.0,
            slowest_turn_prepaint_index = self.slowest_turn_prepaint_index.get(),
            markdown_cache_lookups = markdown_delta.lookups,
            markdown_cache_ready_hits = markdown_delta.ready_hits,
            markdown_cache_pending_hits = markdown_delta.pending_hits,
            markdown_cache_misses = markdown_delta.misses,
            markdown_cache_invalidations = markdown_delta.invalidations,
            markdown_parse_scheduled = markdown_delta.scheduled_parses,
            markdown_parse_completed = markdown_delta.completed_parses,
            markdown_parse_stale_completions = markdown_delta.stale_completions,
            markdown_cache_evictions = markdown_delta.evictions,
            markdown_parse_ms = markdown_delta.parse_micros as f64 / 1000.0,
            markdown_cache_entries = markdown_cache_stats.entries,
            markdown_cache_pending_entries = markdown_cache_stats.pending_entries,
            markdown_cache_source_bytes = markdown_cache_stats.source_bytes,
            markdown_cache_estimated_retained_bytes = markdown_cache_stats.estimated_retained_bytes,
            "slow transcript frame"
        );
    }
}
