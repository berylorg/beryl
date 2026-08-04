use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, HomeStore, ReadError,
};
use beryl_model::{SyndicContentId, SyndicItemId, SyndicTurnId};
use sha2::{Digest, Sha256};

use crate::{
    CanonicalItemKind, CanonicalItemRecord, ContentChunkRecord, ContentLifecycle,
    ContentManifestRecord, ContentReference, ContentTextSpanRecord, ThreadCatalogTitle,
    ThreadCatalogTitleSource, ThreadRecord, TurnItemIndexRecord, TurnItemOrdinal, TurnKind,
    TurnRecord,
    codec::{
        CanonicalItemsFamily, ContentChunkKey, ContentChunksFamily, ContentTextSpanKey,
        ContentTextSpansFamily, ExactCodec, Family, TurnItemKey, TurnItemsFamily, TurnsFamily,
        family_point_limit,
    },
    domain::{SyndicDomain, SyndicStorage},
};

const TEXT_SPAN_PAGE_MAX_ITEMS: usize = 64;
const TEXT_SPAN_PAGE_MAX_BYTES: usize = 65_536;

mod snapshot;

#[derive(Clone, Copy)]
pub(crate) enum HistoryTitlePath {
    ThreadRules,
    EntireSelectedPath,
}

pub(crate) enum HistoryTitleReadError {
    Read(ReadError),
    Invariant(&'static str),
}

impl From<ReadError> for HistoryTitleReadError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

pub(crate) struct StoreTitleSnapshot<'a> {
    pub(crate) storage: SyndicStorage,
    pub(crate) store: &'a HomeStore,
}

pub(crate) struct DomainTitleSnapshot<'a, 'b> {
    pub(crate) reader: &'a DomainReader<'b, SyndicDomain>,
}

pub(crate) struct TextSpanPage {
    records: Vec<ContentTextSpanRecord>,
    has_more: bool,
}

pub(crate) trait TitleSnapshot {
    fn turn(&self, id: SyndicTurnId) -> Result<Option<TurnRecord>, HistoryTitleReadError>;
    fn first_item(
        &self,
        turn: SyndicTurnId,
    ) -> Result<Option<TurnItemIndexRecord>, HistoryTitleReadError>;
    fn item(&self, id: SyndicItemId) -> Result<Option<CanonicalItemRecord>, HistoryTitleReadError>;
    fn manifest(
        &self,
        content: SyndicContentId,
    ) -> Result<Option<ContentManifestRecord>, HistoryTitleReadError>;
    fn chunk(
        &self,
        key: ContentChunkKey,
    ) -> Result<Option<ContentChunkRecord>, HistoryTitleReadError>;
    fn text_spans(
        &self,
        content: SyndicContentId,
        after: Option<u64>,
        through: u64,
    ) -> Result<TextSpanPage, HistoryTitleReadError>;
}

pub(crate) fn derive_history_title(
    snapshot: &impl TitleSnapshot,
    thread: &ThreadRecord,
    path: HistoryTitlePath,
) -> Result<Option<ThreadCatalogTitle>, HistoryTitleReadError> {
    let Some(mut turn_id) = thread.committed_tail() else {
        return Ok(None);
    };
    let mut expected_depth = None;
    let mut earliest_content = None;
    let mut at_tail = true;
    loop {
        let turn = snapshot
            .turn(turn_id)?
            .ok_or(HistoryTitleReadError::Invariant(
                "history-title selected-path turn is missing",
            ))?;
        if turn.id() != turn_id || expected_depth.is_some_and(|depth| turn.depth().get() != depth) {
            return Err(HistoryTitleReadError::Invariant(
                "history-title selected-path turn identity or depth disagrees",
            ));
        }
        if at_tail && turn.chain_digest() != thread.selected_path_digest() {
            return Err(HistoryTitleReadError::Invariant(
                "history-title selected tail digest disagrees with its thread",
            ));
        }
        at_tail = false;
        if turn.kind() == TurnKind::OrdinaryUser
            && eligible_origin(thread, &turn, path)
            && let Some(content) = eligible_user_content(snapshot, &turn)?
        {
            earliest_content = Some(content);
        }
        let Some(parent_id) = turn.parent().turn() else {
            if turn.depth().get() != 1 {
                return Err(HistoryTitleReadError::Invariant(
                    "history-title selected path ended above its root",
                ));
            }
            break;
        };
        let parent_depth =
            turn.depth()
                .get()
                .checked_sub(1)
                .ok_or(HistoryTitleReadError::Invariant(
                    "history-title selected-path depth underflowed",
                ))?;
        turn_id = parent_id;
        expected_depth = Some(parent_depth);
    }

    let Some(content) = earliest_content else {
        return Ok(None);
    };
    let prefix = read_logical_prefix(snapshot, content)?;
    let Some(text) = normalize_history_title(&prefix) else {
        return Ok(None);
    };
    ThreadCatalogTitle::new(text, ThreadCatalogTitleSource::HistoryDerived)
        .map(Some)
        .map_err(|_| {
            HistoryTitleReadError::Invariant(
                "history-title normalization produced an invalid compact title",
            )
        })
}

fn eligible_origin(thread: &ThreadRecord, turn: &TurnRecord, path: HistoryTitlePath) -> bool {
    matches!(path, HistoryTitlePath::EntireSelectedPath)
        || thread.parent_thread_id().is_none()
        || turn.origin_thread_id() == thread.id()
}

fn eligible_user_content(
    snapshot: &impl TitleSnapshot,
    turn: &TurnRecord,
) -> Result<Option<ContentReference>, HistoryTitleReadError> {
    let Some(index) = snapshot.first_item(turn.id())? else {
        return Ok(None);
    };
    if index.turn_id() != turn.id() || index.ordinal() != TurnItemOrdinal::FIRST {
        return Err(HistoryTitleReadError::Invariant(
            "history-title first-item index identity disagrees",
        ));
    }
    let item = snapshot
        .item(index.item_id())?
        .ok_or(HistoryTitleReadError::Invariant(
            "history-title first canonical item is missing",
        ))?;
    if item.id() != index.item_id()
        || item.revision() != index.item_revision()
        || item.turn_id() != turn.id()
        || item.ordinal() != TurnItemOrdinal::FIRST
    {
        return Err(HistoryTitleReadError::Invariant(
            "history-title first canonical item identity disagrees",
        ));
    }
    if item.kind() != CanonicalItemKind::UserInput {
        return Ok(None);
    }
    let content = item
        .presentation_content()
        .ok_or(HistoryTitleReadError::Invariant(
            "history-title user input has no canonical content",
        ))?;
    let manifest = snapshot
        .manifest(content.id())?
        .ok_or(HistoryTitleReadError::Invariant(
            "history-title canonical content manifest is missing",
        ))?;
    if manifest.id() != content.id()
        || manifest.owner().is_some()
        || manifest.lifecycle() != ContentLifecycle::Sealed
        || manifest.sealed_reference() != Some(content)
    {
        return Err(HistoryTitleReadError::Invariant(
            "history-title canonical input is not exact sealed content",
        ));
    }
    Ok(Some(content))
}

fn read_logical_prefix(
    snapshot: &impl TitleSnapshot,
    content: ContentReference,
) -> Result<String, HistoryTitleReadError> {
    let through = content
        .summary()
        .logical_utf8_bytes()
        .min(crate::HISTORY_DERIVED_TITLE_SCAN_MAX_BYTES as u64);
    if through == 0 {
        return Ok(String::new());
    }
    let mut output =
        Vec::with_capacity(usize::try_from(through).expect("history-title scan bound fits usize"));
    let mut logical = 0_u64;
    let mut after = None;
    while logical < through {
        let page = snapshot.text_spans(content.id(), after, through)?;
        if page.records.is_empty() {
            return Err(HistoryTitleReadError::Invariant(
                "history-title logical content has an indexed gap",
            ));
        }
        for span in page.records {
            if logical >= through {
                break;
            }
            after = Some(span.logical_start());
            append_title_span(snapshot, content, span, through, &mut logical, &mut output)?;
            if logical < through && span.logical_end() > logical {
                return String::from_utf8(output).map_err(|_| {
                    HistoryTitleReadError::Invariant(
                        "history-title logical prefix is not valid UTF-8",
                    )
                });
            }
        }
        if logical < through && !page.has_more {
            return Err(HistoryTitleReadError::Invariant(
                "history-title logical content ended before its manifest frontier",
            ));
        }
    }
    String::from_utf8(output).map_err(|_| {
        HistoryTitleReadError::Invariant("history-title logical prefix is not valid UTF-8")
    })
}

fn append_title_span(
    snapshot: &impl TitleSnapshot,
    content: ContentReference,
    span: ContentTextSpanRecord,
    through: u64,
    logical: &mut u64,
    output: &mut Vec<u8>,
) -> Result<(), HistoryTitleReadError> {
    if span.content_id() != content.id()
        || span.logical_start() != *logical
        || span.logical_end() > content.summary().logical_utf8_bytes()
        || span.encoded_end() > content.summary().encoded_bytes()
        || span.chunk_ordinal().get() > content.summary().chunk_count()
        || span.chunk_start() > span.encoded_start()
    {
        return Err(HistoryTitleReadError::Invariant(
            "history-title logical text span frontier disagrees",
        ));
    }
    let chunk = snapshot
        .chunk(ContentChunkKey {
            owner: content.id(),
            ordinal: span.chunk_ordinal(),
        })?
        .ok_or(HistoryTitleReadError::Invariant(
            "history-title logical text chunk is missing",
        ))?;
    if chunk.content_id() != content.id()
        || chunk.ordinal() != span.chunk_ordinal()
        || <[u8; 32]>::from(Sha256::digest(chunk.bytes())) != *chunk.digest()
    {
        return Err(HistoryTitleReadError::Invariant(
            "history-title logical text chunk identity or digest disagrees",
        ));
    }
    let start = usize::try_from(span.encoded_start() - span.chunk_start()).map_err(|_| {
        HistoryTitleReadError::Invariant("history-title text span start overflowed")
    })?;
    let end = usize::try_from(span.encoded_end() - span.chunk_start())
        .map_err(|_| HistoryTitleReadError::Invariant("history-title text span end overflowed"))?;
    let source = chunk
        .bytes()
        .get(start..end)
        .ok_or(HistoryTitleReadError::Invariant(
            "history-title text span lies outside its chunk",
        ))?;
    if <[u8; 32]>::from(Sha256::digest(source)) != span.digest() {
        return Err(HistoryTitleReadError::Invariant(
            "history-title text span digest disagrees",
        ));
    }
    let source = std::str::from_utf8(source).map_err(|_| {
        HistoryTitleReadError::Invariant("history-title text span is not valid UTF-8")
    })?;
    let mut local_end = usize::try_from(through.min(span.logical_end()) - span.logical_start())
        .map_err(|_| HistoryTitleReadError::Invariant("history-title text offset overflowed"))?;
    while local_end > 0 && !source.is_char_boundary(local_end) {
        local_end -= 1;
    }
    output.extend_from_slice(&source.as_bytes()[..local_end]);
    let local_end = u64::try_from(local_end).map_err(|_| {
        HistoryTitleReadError::Invariant("history-title logical text offset overflowed")
    })?;
    *logical =
        span.logical_start()
            .checked_add(local_end)
            .ok_or(HistoryTitleReadError::Invariant(
                "history-title logical text offset overflowed",
            ))?;
    Ok(())
}

fn normalize_history_title(prefix: &str) -> Option<String> {
    let mut line = NormalizedLine::default();
    let mut chars = prefix.chars().peekable();
    while let Some(value) = chars.next() {
        if value == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
            if let Some(title) = line.finish() {
                return Some(title);
            }
            line = NormalizedLine::default();
        } else if value == '\n' {
            if let Some(title) = line.finish() {
                return Some(title);
            }
            line = NormalizedLine::default();
        } else if value.is_whitespace() {
            line.whitespace();
        } else if !value.is_control() {
            line.scalar(value);
        }
    }
    line.finish()
}

#[derive(Default)]
struct NormalizedLine {
    retained: String,
    normalized_scalars: usize,
    in_whitespace: bool,
    has_alphanumeric: bool,
}

impl NormalizedLine {
    fn whitespace(&mut self) {
        if !self.in_whitespace {
            self.emit(' ');
            self.in_whitespace = true;
        }
    }

    fn scalar(&mut self, value: char) {
        self.in_whitespace = false;
        self.has_alphanumeric |= value.is_alphanumeric();
        self.emit(value);
    }

    fn emit(&mut self, value: char) {
        if self.normalized_scalars < crate::HISTORY_DERIVED_TITLE_MAX_SCALARS
            && self.retained.len() + value.len_utf8() <= crate::THREAD_TITLE_MAX_BYTES
        {
            self.retained.push(value);
        }
        self.normalized_scalars = self.normalized_scalars.saturating_add(1);
    }

    fn finish(mut self) -> Option<String> {
        if !self.has_alphanumeric {
            return None;
        }
        while self
            .retained
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            self.retained.pop();
        }
        (!self.retained.is_empty()).then_some(self.retained)
    }
}
