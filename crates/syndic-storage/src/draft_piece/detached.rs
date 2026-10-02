use super::*;
use crate::SyndicStorage;
use beryl_home_store::{
    CommandCancellation, HomeStore, TemporaryReadError, TemporaryReadPool, TemporaryReadReader,
};

mod codec;
use codec::{HEADER_BYTES, MARKER_BYTES, decode_marker, encode_marker, marker_key};

#[derive(Debug, thiserror::Error)]
pub enum DetachedDraftReadErrorV1 {
    #[error("detached draft acquisition cancelled")]
    Cancelled,
    #[error("detached draft read limits exceeded")]
    Limits,
    #[error("detached draft source invariant failed")]
    Invariant,
    #[error("malformed detached draft request: {0:?}")]
    Malformed(DraftPieceMalformedRangeRequestV1),
    #[error("detached draft source read failed: {0}")]
    Source(#[from] DraftPieceRangeSourceErrorV1),
    #[error("detached draft backing failed: {0}")]
    Backing(#[from] TemporaryReadError),
}
type Result<T> = std::result::Result<T, DetachedDraftReadErrorV1>;

#[derive(Clone, Copy, Debug)]
pub struct DetachedDraftReadLimitsV1 {
    text_page_bytes: usize,
    marker_page_objects: usize,
    marker_page_bytes: usize,
}
impl DetachedDraftReadLimitsV1 {
    pub fn new(
        text_page_bytes: usize,
        marker_page_objects: usize,
        marker_page_bytes: usize,
    ) -> Result<Self> {
        if !(4..=DRAFT_PIECE_PAGE_MAX_BYTES).contains(&text_page_bytes)
            || !(1..=DRAFT_PIECE_PAGE_MAX_RECORDS).contains(&marker_page_objects)
            || !(1..=DRAFT_PIECE_PAGE_MAX_BYTES).contains(&marker_page_bytes)
        {
            return Err(DetachedDraftReadErrorV1::Limits);
        }
        Ok(Self {
            text_page_bytes,
            marker_page_objects,
            marker_page_bytes,
        })
    }
}
impl Default for DetachedDraftReadLimitsV1 {
    fn default() -> Self {
        Self {
            text_page_bytes: DRAFT_PIECE_PAGE_MAX_BYTES,
            marker_page_objects: DRAFT_PIECE_PAGE_MAX_RECORDS,
            marker_page_bytes: DRAFT_PIECE_PAGE_MAX_BYTES,
        }
    }
}

#[derive(Clone)]
pub struct DetachedDraftReadSourceV1 {
    binding: DraftEditorCandidateActivationBindingV1,
    backing: TemporaryReadReader,
    limits: DetachedDraftReadLimitsV1,
}

fn cancelled(cancellation: &CommandCancellation) -> Result<()> {
    if cancellation.is_cancelled() {
        Err(DetachedDraftReadErrorV1::Cancelled)
    } else {
        Ok(())
    }
}

impl SyndicStorage {
    pub fn export_detached_draft_read_source(
        &self,
        store: &HomeStore,
        binding: DraftEditorCandidateActivationBindingV1,
        pool: &TemporaryReadPool,
        limits: DetachedDraftReadLimitsV1,
        cancellation: &CommandCancellation,
    ) -> Result<DetachedDraftReadSourceV1> {
        cancelled(cancellation)?;
        if pool.limits().max_page_bytes()
            < limits
                .text_page_bytes
                .max(MARKER_BYTES)
                .max(HEADER_BYTES as usize)
        {
            return Err(DetachedDraftReadErrorV1::Limits);
        }
        self.candidate_draft_piece_text_demand(
            store,
            binding,
            DraftPieceTextDemandV1::Forward(0),
            4,
        )?;
        let root = binding.root();
        let summary = root.summary();
        let text_length = summary.logical_utf8_bytes();
        let marker_count = summary.marker_count();
        let total = marker_count
            .checked_mul(MARKER_BYTES as u64)
            .and_then(|n| n.checked_add(text_length))
            .and_then(|n| n.checked_add(HEADER_BYTES))
            .ok_or(DetachedDraftReadErrorV1::Limits)?;
        let mut writer = pool.begin(total)?;
        let mut header = [0; HEADER_BYTES as usize];
        header[..8].copy_from_slice(b"SYNDREAD");
        header[8..16].copy_from_slice(&text_length.to_le_bytes());
        header[16..24].copy_from_slice(&marker_count.to_le_bytes());
        header[24..32].copy_from_slice(&1u64.to_le_bytes());
        writer.write(0, &header)?;
        let mut at = 0;
        loop {
            cancelled(cancellation)?;
            let page = self.draft_piece_text_demand(
                store,
                root,
                DraftPieceTextDemandV1::Forward(at),
                limits.text_page_bytes,
            )?;
            if page.root() != root
                || page.start() != at
                || page.end() > text_length
                || page.end().checked_sub(at) != Some(page.bytes().len() as u64)
                || std::str::from_utf8(page.bytes()).is_err()
            {
                return Err(DetachedDraftReadErrorV1::Invariant);
            }
            writer.write(HEADER_BYTES + at, page.bytes())?;
            if page.end() == text_length {
                break;
            }
            if page.end() == at {
                return Err(DetachedDraftReadErrorV1::Invariant);
            }
            at = page.end();
        }
        let mut cursor = None;
        let mut previous = None;
        let mut previous_anchor = None;
        let mut count = 0u64;
        loop {
            cancelled(cancellation)?;
            let demand = DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::InclusiveRange {
                    start: 0,
                    end: text_length,
                },
                DraftPieceMarkerDirectionV1::Forward,
                cursor,
                limits.marker_page_objects,
                limits.marker_page_bytes,
            );
            let page = self.draft_piece_marker_demand(store, root, demand)?;
            if page.root() != root {
                return Err(DetachedDraftReadErrorV1::Invariant);
            }
            for marker in page.markers() {
                cancelled(cancellation)?;
                let key = marker_key(*marker);
                if marker.anchor() > text_length
                    || previous.is_some_and(|last| last >= key)
                    || count >= marker_count
                {
                    return Err(DetachedDraftReadErrorV1::Invariant);
                }
                if marker.anchor() != 0
                    && marker.anchor() != text_length
                    && previous_anchor != Some(marker.anchor())
                {
                    let boundary = self.draft_piece_text_demand(
                        store,
                        root,
                        DraftPieceTextDemandV1::Validate(marker.anchor()),
                        4,
                    )?;
                    if boundary.start() != marker.anchor() && boundary.end() != marker.anchor() {
                        return Err(DetachedDraftReadErrorV1::Invariant);
                    }
                }
                let offset = HEADER_BYTES + text_length + count * MARKER_BYTES as u64;
                writer.write(offset, &encode_marker(*marker))?;
                count += 1;
                previous = Some(key);
                previous_anchor = Some(marker.anchor());
            }
            if page.requested_side_complete() {
                if page.continuation().is_some() {
                    return Err(DetachedDraftReadErrorV1::Invariant);
                }
                break;
            }
            if page.markers().is_empty()
                || page.continuation() != previous
                || page.continuation() == cursor
            {
                return Err(DetachedDraftReadErrorV1::Invariant);
            }
            cursor = page.continuation();
        }
        if count != marker_count {
            return Err(DetachedDraftReadErrorV1::Invariant);
        }
        cancelled(cancellation)?;
        self.candidate_draft_piece_text_demand(
            store,
            binding,
            DraftPieceTextDemandV1::Forward(0),
            4,
        )?;
        let backing = writer.seal()?;
        cancelled(cancellation)?;
        Ok(DetachedDraftReadSourceV1 {
            binding,
            backing,
            limits,
        })
    }
}

impl DetachedDraftReadSourceV1 {
    pub const fn binding(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.binding
    }
    pub const fn root(&self) -> DraftPieceRootReferenceV1 {
        self.binding.root()
    }
    fn text(&self, at: u64, length: usize) -> Result<Vec<u8>> {
        Ok(self.backing.read(
            HEADER_BYTES
                .checked_add(at)
                .ok_or(DetachedDraftReadErrorV1::Invariant)?,
            length,
        )?)
    }
    fn boundary(&self, at: u64) -> Result<bool> {
        if at == self.root().summary().logical_utf8_bytes() {
            return Ok(true);
        }
        Ok(self.text(at, 1)?[0] & 0xc0 != 0x80)
    }
    pub fn text_demand(
        &self,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    ) -> Result<DraftPieceTextDemandResultV1> {
        if !(4..=self.limits.text_page_bytes).contains(&max_bytes) {
            return Err(DetachedDraftReadErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Limit,
            ));
        }
        let extent = self.root().summary().logical_utf8_bytes();
        let coordinate = match demand {
            DraftPieceTextDemandV1::Forward(n)
            | DraftPieceTextDemandV1::Backward(n)
            | DraftPieceTextDemandV1::Validate(n) => n,
        };
        if coordinate > extent {
            return Err(DetachedDraftReadErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Utf8Boundary,
            ));
        }
        let (start, end, bytes) = if extent == 0 {
            (0, 0, Vec::new())
        } else {
            match demand {
                DraftPieceTextDemandV1::Forward(start) => {
                    if !self.boundary(start)? {
                        return Err(DetachedDraftReadErrorV1::Malformed(
                            DraftPieceMalformedRangeRequestV1::Utf8Boundary,
                        ));
                    }
                    let length = (extent - start).min(max_bytes as u64) as usize;
                    let mut bytes = self.text(start, length)?;
                    if let Err(error) = std::str::from_utf8(&bytes) {
                        if error.error_len().is_some() {
                            return Err(DetachedDraftReadErrorV1::Invariant);
                        }
                        bytes.truncate(error.valid_up_to());
                    }
                    (start, start + bytes.len() as u64, bytes)
                }
                DraftPieceTextDemandV1::Backward(end) => {
                    if !self.boundary(end)? {
                        return Err(DetachedDraftReadErrorV1::Malformed(
                            DraftPieceMalformedRangeRequestV1::Utf8Boundary,
                        ));
                    }
                    let start = end.saturating_sub(max_bytes as u64);
                    let mut bytes = self.text(start, (end - start) as usize)?;
                    let skip = bytes.iter().take_while(|b| **b & 0xc0 == 0x80).count();
                    if skip > 3 {
                        return Err(DetachedDraftReadErrorV1::Invariant);
                    }
                    bytes.drain(..skip);
                    if std::str::from_utf8(&bytes).is_err() {
                        return Err(DetachedDraftReadErrorV1::Invariant);
                    }
                    (start + skip as u64, end, bytes)
                }
                DraftPieceTextDemandV1::Validate(candidate) => {
                    let mut start = if candidate == extent {
                        candidate - 1
                    } else {
                        candidate
                    };
                    let mut steps = 0;
                    while !self.boundary(start)? {
                        if start == 0 || steps == 3 {
                            return Err(DetachedDraftReadErrorV1::Invariant);
                        }
                        start -= 1;
                        steps += 1;
                    }
                    let lead = self.text(start, 1)?[0];
                    let width = match lead {
                        0..=0x7f => 1,
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => return Err(DetachedDraftReadErrorV1::Invariant),
                    };
                    let end = start
                        .checked_add(width)
                        .filter(|end| *end <= extent)
                        .ok_or(DetachedDraftReadErrorV1::Invariant)?;
                    let bytes = self.text(start, width as usize)?;
                    if std::str::from_utf8(&bytes).is_err() {
                        return Err(DetachedDraftReadErrorV1::Invariant);
                    }
                    (start, end, bytes)
                }
            }
        };
        Ok(DraftPieceTextDemandResultV1::new(
            self.root(),
            demand,
            start,
            end,
            bytes,
            if start == 0 {
                DraftPieceTextEdgeFactV1::DocumentStart
            } else {
                DraftPieceTextEdgeFactV1::Continuation(start)
            },
            if end == extent {
                DraftPieceTextEdgeFactV1::DocumentEnd
            } else {
                DraftPieceTextEdgeFactV1::Continuation(end)
            },
            0,
        ))
    }
    fn marker(&self, ordinal: u64) -> Result<DraftPieceMarkerAtV1> {
        if ordinal >= self.root().summary().marker_count() {
            return Err(DetachedDraftReadErrorV1::Invariant);
        }
        let offset = ordinal
            .checked_mul(MARKER_BYTES as u64)
            .and_then(|n| n.checked_add(self.root().summary().logical_utf8_bytes()))
            .and_then(|n| n.checked_add(HEADER_BYTES))
            .ok_or(DetachedDraftReadErrorV1::Invariant)?;
        decode_marker(&self.backing.read(offset, MARKER_BYTES)?)
    }
    fn lower_bound(&self, key: DraftCompositeSearchKeyV1) -> Result<u64> {
        let mut low = 0;
        let mut high = self.root().summary().marker_count();
        while low < high {
            let middle = low + (high - low) / 2;
            if marker_key(self.marker(middle)?) < key {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        Ok(low)
    }
    pub fn marker_demand(
        &self,
        demand: DraftPieceMarkerDemandV1,
    ) -> Result<DraftPieceMarkerDemandResultV1> {
        if !(1..=self.limits.marker_page_objects).contains(&demand.object_ceiling())
            || !(1..=self.limits.marker_page_bytes).contains(&demand.retained_byte_ceiling())
        {
            return Err(DetachedDraftReadErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Limit,
            ));
        }
        let (start, end, inclusive) = demand.scope().bounds();
        if start > end || end > self.root().summary().logical_utf8_bytes() {
            return Err(DetachedDraftReadErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Cursor,
            ));
        }
        let lower = self.lower_bound(DraftCompositeSearchKeyV1::BeforeMarkers(start))?;
        let upper = self.lower_bound(if inclusive {
            DraftCompositeSearchKeyV1::AfterMarkers(end)
        } else {
            DraftCompositeSearchKeyV1::BeforeMarkers(end)
        })?;
        let cursor = match demand.cursor() {
            Some(key) => {
                let index = self.lower_bound(key)?;
                if index < lower || index >= upper || marker_key(self.marker(index)?) != key {
                    return Err(DetachedDraftReadErrorV1::Malformed(
                        DraftPieceMalformedRangeRequestV1::Cursor,
                    ));
                }
                Some(index)
            }
            None => None,
        };
        let forward = demand.direction() == DraftPieceMarkerDirectionV1::Forward;
        let from = if forward {
            cursor.map_or(lower, |n| n + 1)
        } else {
            lower
        };
        let to = if forward {
            upper
        } else {
            cursor.unwrap_or(upper)
        };
        let available = to - from;
        let mut count = available.min(demand.object_ceiling() as u64);
        loop {
            let begin = if forward { from } else { to - count };
            let finish = begin + count;
            let preceding = if count == 0 {
                if forward {
                    demand.cursor().map_or(
                        DraftPieceMarkerEdgeFactV1::RangeStart,
                        DraftPieceMarkerEdgeFactV1::Marker,
                    )
                } else {
                    DraftPieceMarkerEdgeFactV1::RangeStart
                }
            } else if begin > lower {
                DraftPieceMarkerEdgeFactV1::Marker(marker_key(self.marker(begin - 1)?))
            } else {
                DraftPieceMarkerEdgeFactV1::RangeStart
            };
            let following = if count == 0 {
                if !forward {
                    demand.cursor().map_or(
                        DraftPieceMarkerEdgeFactV1::RangeEnd,
                        DraftPieceMarkerEdgeFactV1::Marker,
                    )
                } else {
                    DraftPieceMarkerEdgeFactV1::RangeEnd
                }
            } else if finish < upper {
                DraftPieceMarkerEdgeFactV1::Marker(marker_key(self.marker(finish)?))
            } else {
                DraftPieceMarkerEdgeFactV1::RangeEnd
            };
            let more = if forward {
                finish < upper
            } else {
                begin > lower
            };
            let continuation = if more && count != 0 {
                Some(marker_key(self.marker(if forward {
                    finish - 1
                } else {
                    begin
                })?))
            } else {
                None
            };
            let retained = super::persistent::marker_retained_bytes(
                count as usize,
                preceding,
                following,
                continuation,
            );
            if retained <= demand.retained_byte_ceiling() {
                if available != 0 && count == 0 {
                    return Err(DetachedDraftReadErrorV1::Limits);
                }
                let mut markers = Vec::with_capacity(count as usize);
                for ordinal in begin..finish {
                    markers.push(self.marker(ordinal)?);
                }
                return Ok(DraftPieceMarkerDemandResultV1::new(
                    self.root(),
                    &demand,
                    markers,
                    preceding,
                    following,
                    continuation.is_none(),
                    continuation,
                    retained,
                    0,
                ));
            }
            if count == 0 {
                return Err(DetachedDraftReadErrorV1::Limits);
            }
            count -= 1;
        }
    }
}
