use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{Bounds, ClipboardItem, Image, ImageFormat, Pixels, Point};

use super::{
    execution_detail::{TurnExecutionRecord, UserInputFragment},
    transcript_edit_menu_state::{TranscriptEditMenuEntry, TranscriptEditRequest},
    transcript_presentation::TranscriptPresentedRow,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptBranchMenuState {
    open: Option<TranscriptBranchMenuOpen>,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptBranchMenuOpen {
    branch_target: Option<TranscriptBranchTarget>,
    edit_entry: Option<TranscriptEditMenuEntry>,
    image_target: Option<TranscriptImageMenuTarget>,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptBranchTarget {
    source_thread_id: String,
    source_turn_id: String,
    source_turn_index: usize,
    title_seed_fragments: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptBranchAction {
    SwitchTo,
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptBranchRequest {
    action: TranscriptBranchAction,
    target: TranscriptBranchTarget,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct TranscriptImageMenuTarget {
    row_identity: String,
    media_identity: String,
    alt: String,
    format: ImageFormat,
    source: TranscriptImageMenuSource,
    source_path: Option<String>,
}

#[derive(Clone, Debug)]
enum TranscriptImageMenuSource {
    RetainedBytes { bytes: Arc<[u8]>, image: Arc<Image> },
    File { path: PathBuf },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptBranchMenuOpenGate {
    pub(crate) transcript_selection_active: bool,
    pub(crate) source_thread_idle: bool,
    pub(crate) selected_thread_matches_target: bool,
    pub(crate) selected_thread_compaction_active: bool,
    pub(crate) pending_thread_activation: bool,
    pub(crate) branch_capability_available: bool,
}

impl TranscriptBranchMenuState {
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    #[allow(dead_code)]
    pub(crate) fn open_target(&mut self, target: TranscriptBranchTarget, position: Point<Pixels>) {
        self.open_menu(Some(target), None, None, position);
    }

    pub(crate) fn open_menu(
        &mut self,
        branch_target: Option<TranscriptBranchTarget>,
        edit_entry: Option<TranscriptEditMenuEntry>,
        image_target: Option<TranscriptImageMenuTarget>,
        position: Point<Pixels>,
    ) {
        if branch_target.is_none() && edit_entry.is_none() && image_target.is_none() {
            self.open = None;
            return;
        }

        self.open = Some(TranscriptBranchMenuOpen {
            branch_target,
            edit_entry,
            image_target,
            position,
            bounds: None,
        });
    }

    pub(crate) fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    pub(crate) fn active(&self) -> Option<&TranscriptBranchMenuOpen> {
        self.open.as_ref()
    }

    pub(crate) fn set_bounds(&mut self, bounds: Option<Bounds<Pixels>>) {
        if let Some(open) = self.open.as_mut() {
            open.bounds = bounds;
        }
    }

    pub(crate) fn should_dismiss_for_mouse_down(&self, position: Point<Pixels>) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| !open.bounds.is_some_and(|bounds| bounds.contains(&position)))
    }

    pub(crate) fn accept(
        &mut self,
        action: TranscriptBranchAction,
    ) -> Option<TranscriptBranchRequest> {
        let target = self.open.take()?.branch_target?;
        Some(TranscriptBranchRequest { action, target })
    }

    pub(crate) fn accept_edit(&mut self) -> Option<TranscriptEditRequest> {
        let entry = self.open.take()?.edit_entry?;
        entry.into_request()
    }

    pub(crate) fn accept_copy_image(&mut self) -> Option<TranscriptImageMenuTarget> {
        let target = self.open.take()?.image_target?;
        Some(target)
    }

    pub(crate) fn accept_save_image(&mut self) -> Option<TranscriptImageMenuTarget> {
        let target = self.open.take()?.image_target?;
        Some(target)
    }

    pub(crate) fn clear_image_target(&mut self) -> bool {
        let Some(open) = self.open.as_mut() else {
            return false;
        };

        if open.image_target.take().is_none() {
            return false;
        }
        if open.branch_target.is_none() && open.edit_entry.is_none() {
            self.open = None;
        }
        true
    }
}

impl TranscriptBranchMenuOpen {
    pub(crate) fn position(&self) -> Point<Pixels> {
        self.position
    }

    pub(crate) fn branch_target(&self) -> Option<&TranscriptBranchTarget> {
        self.branch_target.as_ref()
    }

    pub(crate) fn edit_entry(&self) -> Option<&TranscriptEditMenuEntry> {
        self.edit_entry.as_ref()
    }

    pub(crate) fn image_target(&self) -> Option<&TranscriptImageMenuTarget> {
        self.image_target.as_ref()
    }
}

impl TranscriptBranchTarget {
    #[cfg(test)]
    pub(crate) fn for_test(
        source_thread_id: impl Into<String>,
        source_turn_id: impl Into<String>,
        source_turn_index: usize,
        title_seed_fragments: Vec<String>,
    ) -> Self {
        Self {
            source_thread_id: source_thread_id.into(),
            source_turn_id: source_turn_id.into(),
            source_turn_index,
            title_seed_fragments,
        }
    }

    pub(crate) fn from_presented_row(row: &TranscriptPresentedRow) -> Option<Self> {
        let turn = row.turn.as_ref();
        if turn.is_released_history_placeholder() {
            return None;
        }

        let source_thread_id = turn.thread_id.clone()?;
        let source_turn_id = turn.turn_id.clone()?;
        let title_seed_fragments = title_seed_fragments_for_turn(turn);
        if title_seed_fragments.is_empty() {
            return None;
        }

        Some(Self {
            source_thread_id,
            source_turn_id,
            source_turn_index: row.source_turn_index,
            title_seed_fragments,
        })
    }

    pub(crate) fn source_thread_id(&self) -> &str {
        &self.source_thread_id
    }

    pub(crate) fn source_turn_id(&self) -> &str {
        &self.source_turn_id
    }

    #[allow(dead_code)]
    pub(crate) fn source_turn_index(&self) -> usize {
        self.source_turn_index
    }

    #[allow(dead_code)]
    pub(crate) fn title_seed_fragments(&self) -> &[String] {
        &self.title_seed_fragments
    }

    pub(crate) fn title_seed_text(&self) -> String {
        self.title_seed_fragments.join("\n\n")
    }
}

impl TranscriptBranchRequest {
    #[cfg(test)]
    pub(crate) fn for_test(action: TranscriptBranchAction, target: TranscriptBranchTarget) -> Self {
        Self { action, target }
    }

    pub(crate) fn action(&self) -> TranscriptBranchAction {
        self.action
    }

    pub(crate) fn target(&self) -> &TranscriptBranchTarget {
        &self.target
    }
}

#[allow(dead_code)]
impl TranscriptImageMenuTarget {
    pub(crate) fn new(
        row_identity: impl Into<String>,
        media_identity: impl Into<String>,
        alt: impl Into<String>,
        format: ImageFormat,
        bytes: impl Into<Arc<[u8]>>,
        image: Arc<Image>,
        source_path: Option<String>,
    ) -> Self {
        Self {
            row_identity: row_identity.into(),
            media_identity: media_identity.into(),
            alt: alt.into(),
            format,
            source: TranscriptImageMenuSource::RetainedBytes {
                bytes: bytes.into(),
                image,
            },
            source_path,
        }
    }

    pub(crate) fn new_file(
        row_identity: impl Into<String>,
        media_identity: impl Into<String>,
        alt: impl Into<String>,
        format: ImageFormat,
        path: PathBuf,
        source_path: Option<String>,
    ) -> Self {
        Self {
            row_identity: row_identity.into(),
            media_identity: media_identity.into(),
            alt: alt.into(),
            format,
            source: TranscriptImageMenuSource::File { path },
            source_path,
        }
    }

    pub(crate) fn row_identity(&self) -> &str {
        &self.row_identity
    }

    pub(crate) fn media_identity(&self) -> &str {
        &self.media_identity
    }

    pub(crate) fn matches_rendered_media(&self, row_identity: &str, media_identity: &str) -> bool {
        self.row_identity == row_identity && self.media_identity == media_identity
    }

    pub(crate) fn matches_loaded_image(&self, target: &Self) -> bool {
        self.matches_rendered_media(target.row_identity(), target.media_identity())
            && self.format == target.format
            && self.alt == target.alt
            && self.source_path == target.source_path
            && self.source.matches_loaded_source(&target.source)
    }

    pub(crate) fn alt(&self) -> &str {
        &self.alt
    }

    pub(crate) fn format(&self) -> ImageFormat {
        self.format
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.retained_bytes()
            .expect("transcript image bytes are only retained for byte-backed menu targets")
    }

    pub(crate) fn retained_bytes(&self) -> Option<&[u8]> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { bytes, .. } => Some(bytes.as_ref()),
            TranscriptImageMenuSource::File { .. } => None,
        }
    }

    pub(crate) fn bytes_arc(&self) -> Arc<[u8]> {
        self.retained_bytes_arc()
            .expect("transcript image bytes are only retained for byte-backed menu targets")
    }

    pub(crate) fn retained_bytes_arc(&self) -> Option<Arc<[u8]>> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { bytes, .. } => Some(bytes.clone()),
            TranscriptImageMenuSource::File { .. } => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn bytes_ptr(&self) -> *const u8 {
        self.retained_bytes()
            .expect("transcript image bytes are only retained for byte-backed menu targets")
            .as_ptr()
    }

    pub(crate) fn file_path(&self) -> Option<&Path> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { .. } => None,
            TranscriptImageMenuSource::File { path } => Some(path.as_path()),
        }
    }

    pub(crate) fn image(&self) -> Option<&Image> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { image, .. } => Some(image.as_ref()),
            TranscriptImageMenuSource::File { .. } => None,
        }
    }

    pub(crate) fn clipboard_item(&self) -> io::Result<ClipboardItem> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { image, .. } => {
                Ok(ClipboardItem::new_image(image.as_ref()))
            }
            TranscriptImageMenuSource::File { path } => {
                let bytes = fs::read(path)?;
                let image = Image::from_bytes(self.format, bytes);
                Ok(ClipboardItem::new_image(&image))
            }
        }
    }

    pub(crate) fn save_to_path(&self, path: PathBuf) -> io::Result<PathBuf> {
        match &self.source {
            TranscriptImageMenuSource::RetainedBytes { bytes, .. } => {
                fs::write(&path, bytes.as_ref())?;
            }
            TranscriptImageMenuSource::File { path: source } => {
                fs::copy(source, &path)?;
            }
        }
        Ok(path)
    }

    pub(crate) fn suggested_save_filename(&self) -> String {
        let base_name = self
            .source_path
            .as_deref()
            .and_then(source_path_file_name)
            .or_else(|| (!self.alt.trim().is_empty()).then_some(self.alt.as_str()))
            .unwrap_or("transcript-image");
        let sanitized = sanitize_suggested_file_name(base_name);

        if suggested_file_name_has_extension(&sanitized) {
            sanitized
        } else {
            format!("{sanitized}.{}", self.save_extension())
        }
    }

    pub(crate) fn save_extension(&self) -> &'static str {
        image_format_save_extension(self.format)
    }

    pub(crate) fn save_path_with_default_extension(&self, path: PathBuf) -> PathBuf {
        if path
            .extension()
            .is_some_and(|extension| !extension.is_empty())
        {
            path
        } else {
            path.with_extension(self.save_extension())
        }
    }

    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
}

impl TranscriptImageMenuSource {
    fn matches_loaded_source(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::RetainedBytes { bytes, .. },
                Self::RetainedBytes {
                    bytes: other_bytes, ..
                },
            ) => bytes.as_ref() == other_bytes.as_ref(),
            (Self::File { path }, Self::File { path: other_path }) => path == other_path,
            _ => false,
        }
    }
}

pub(crate) fn transcript_branch_menu_can_open(gate: TranscriptBranchMenuOpenGate) -> bool {
    !gate.transcript_selection_active
        && gate.source_thread_idle
        && gate.selected_thread_matches_target
        && !gate.selected_thread_compaction_active
        && !gate.pending_thread_activation
        && gate.branch_capability_available
}

fn title_seed_fragments_for_turn(turn: &TurnExecutionRecord) -> Vec<String> {
    turn.user_input_fragments()
        .iter()
        .filter_map(title_seed_text_for_fragment)
        .collect()
}

fn title_seed_text_for_fragment(fragment: &UserInputFragment) -> Option<String> {
    let source = fragment.text.as_str();
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0usize;
    let mut markers = fragment.image_markers().iter().collect::<Vec<_>>();
    markers.sort_by_key(|marker| marker.display_range().start);

    for marker in markers {
        let range = marker.display_range();
        if range.start < cursor
            || range.end > source.len()
            || !source.is_char_boundary(range.start)
            || !source.is_char_boundary(range.end)
        {
            continue;
        }
        output.push_str(&source[cursor..range.start]);
        output.push_str(marker.copy_text());
        cursor = range.end;
    }
    output.push_str(&source[cursor..]);

    let output = output.trim().to_string();
    (!output.is_empty()).then_some(output)
}

fn image_format_save_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tif",
    }
}

fn source_path_file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\'])
        .find(|segment| !segment.trim().is_empty())
}

fn sanitize_suggested_file_name(raw: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_space = false;

    for ch in raw.trim().chars() {
        let mapped = if ch.is_control()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        {
            ' '
        } else {
            ch
        };

        if mapped.is_whitespace() {
            if !previous_space {
                sanitized.push(' ');
                previous_space = true;
            }
        } else {
            sanitized.push(mapped);
            previous_space = false;
        }
    }

    let sanitized = sanitized.trim_matches([' ', '.']).to_string();
    if sanitized.is_empty() {
        "transcript-image".to_string()
    } else {
        sanitized
    }
}

fn suggested_file_name_has_extension(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .is_some_and(|extension| !extension.is_empty())
}
