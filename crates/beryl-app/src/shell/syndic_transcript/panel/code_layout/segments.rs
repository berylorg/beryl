use super::{CodeLayoutError, CodeLayoutRead, CodeLayoutReadKind};
use std::ops::Range;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

#[derive(Clone)]
pub(super) struct SegmentCheckpoint {
    pub cursor: GraphemeCursor,
    pub offset: u64,
}

pub(super) enum SegmentKind {
    Text(String),
    Tab,
    Oversize,
    Delimiter,
    End,
}

pub(super) struct Segment {
    pub source: Range<u64>,
    pub kind: SegmentKind,
    pub checkpoint: SegmentCheckpoint,
}

pub(super) struct Segmenter {
    source: Range<u64>,
    cursor: GraphemeCursor,
    cap: usize,
    page_cap: usize,
    page: Option<(u64, String)>,
    context_end: Option<u64>,
    read_offset: u64,
    grapheme_start: u64,
    grapheme: String,
    oversize: bool,
    complete_grapheme: bool,
    pending_start: u64,
    pending: String,
    pending_end: SegmentCheckpoint,
    ended: bool,
}

impl Segmenter {
    pub fn new(source: Range<u64>, cap: usize, page_cap: usize) -> Result<Self, CodeLayoutError> {
        let end =
            usize::try_from(source.end - source.start).map_err(|_| CodeLayoutError::Overflow)?;
        Ok(Self::resume(
            source.clone(),
            cap,
            page_cap,
            SegmentCheckpoint {
                cursor: GraphemeCursor::new(0, end, true),
                offset: source.start,
            },
        ))
    }

    pub fn resume(
        source: Range<u64>,
        cap: usize,
        page_cap: usize,
        checkpoint: SegmentCheckpoint,
    ) -> Self {
        Self {
            read_offset: checkpoint.offset,
            grapheme_start: checkpoint.offset,
            pending_start: checkpoint.offset,
            cursor: checkpoint.cursor.clone(),
            pending_end: checkpoint,
            source,
            cap,
            page_cap,
            page: None,
            context_end: None,
            grapheme: String::with_capacity(cap),
            pending: String::with_capacity(cap),
            oversize: false,
            complete_grapheme: false,
            ended: false,
        }
    }

    pub fn origin(&self) -> SegmentCheckpoint {
        self.pending_end.clone()
    }

    pub fn read(&self) -> Option<CodeLayoutRead> {
        if self.ended {
            return None;
        }
        if let Some(end) = self.context_end {
            return Some(CodeLayoutRead {
                range: end
                    .saturating_sub(self.page_cap as u64)
                    .max(self.source.start)..end,
                kind: CodeLayoutReadKind::Context,
            });
        }
        if self.page.is_none() && self.read_offset < self.source.end {
            return Some(CodeLayoutRead {
                range: self.read_offset
                    ..self
                        .read_offset
                        .saturating_add(self.page_cap as u64)
                        .min(self.source.end),
                kind: CodeLayoutReadKind::Forward,
            });
        }
        None
    }

    pub fn supply(&mut self, start: u64, text: &str) -> Result<(), CodeLayoutError> {
        let request = self.read().ok_or(CodeLayoutError::UnexpectedPage)?;
        let end = start
            .checked_add(text.len() as u64)
            .ok_or(CodeLayoutError::Overflow)?;
        if text.is_empty()
            || text.len() > self.page_cap
            || start < request.range.start
            || end > request.range.end
        {
            return Err(CodeLayoutError::InvalidPage);
        }
        match request.kind {
            CodeLayoutReadKind::Context => {
                if end != request.range.end {
                    return Err(CodeLayoutError::InvalidPage);
                }
                self.cursor.provide_context(
                    text,
                    usize::try_from(start - self.source.start)
                        .map_err(|_| CodeLayoutError::Overflow)?,
                );
                self.context_end = None;
            }
            CodeLayoutReadKind::Forward => {
                if start != request.range.start {
                    return Err(CodeLayoutError::InvalidPage);
                }
                self.page = Some((start, text.to_owned()));
            }
        }
        Ok(())
    }

    pub fn next(&mut self, steps: &mut usize) -> Result<Option<Segment>, CodeLayoutError> {
        loop {
            if self.ended || self.context_end.is_some() {
                return Ok(None);
            }
            if self.complete_grapheme {
                let special = self.oversize
                    || self.grapheme == "\t"
                    || self.grapheme == "\n"
                    || self.grapheme == "\r\n";
                if !self.pending.is_empty()
                    && (special || self.pending.len() + self.grapheme.len() > self.cap)
                {
                    return Ok(Some(self.take_pending()));
                }
                let end = self.source.start + self.cursor.cur_cursor() as u64;
                let checkpoint = SegmentCheckpoint {
                    cursor: self.cursor.clone(),
                    offset: end,
                };
                if special {
                    let kind = if self.oversize {
                        SegmentKind::Oversize
                    } else if self.grapheme == "\t" {
                        SegmentKind::Tab
                    } else {
                        SegmentKind::Delimiter
                    };
                    let source = self.grapheme_start..end;
                    self.finish_grapheme(end);
                    self.pending_start = end;
                    self.pending_end = checkpoint.clone();
                    return Ok(Some(Segment {
                        source,
                        kind,
                        checkpoint,
                    }));
                }
                if self.pending.is_empty() {
                    self.pending_start = self.grapheme_start;
                }
                self.pending.push_str(&self.grapheme);
                self.pending_end = checkpoint;
                self.finish_grapheme(end);
                if self.pending.len() == self.cap {
                    return Ok(Some(self.take_pending()));
                }
            }
            if self.source.start + self.cursor.cur_cursor() as u64 == self.source.end {
                if !self.pending.is_empty() {
                    return Ok(Some(self.take_pending()));
                }
                self.ended = true;
                return Ok(Some(Segment {
                    source: self.source.end..self.source.end,
                    kind: SegmentKind::End,
                    checkpoint: SegmentCheckpoint {
                        cursor: self.cursor.clone(),
                        offset: self.source.end,
                    },
                }));
            }
            if *steps == 0 {
                return Ok(None);
            }
            let Some((start, page)) = self.page.as_ref() else {
                return Ok(None);
            };
            *steps -= 1;
            let before = self.source.start + self.cursor.cur_cursor() as u64;
            let result = self
                .cursor
                .next_boundary(page, (*start - self.source.start) as usize);
            let after = self.source.start + self.cursor.cur_cursor() as u64;
            if after < before || after < *start || after > *start + page.len() as u64 {
                return Err(CodeLayoutError::InvalidPage);
            }
            let piece = &page[(before - *start) as usize..(after - *start) as usize];
            if !self.oversize {
                if self.grapheme.len() + piece.len() > self.cap {
                    self.oversize = true;
                    self.grapheme.clear();
                } else {
                    self.grapheme.push_str(piece);
                }
            }
            match result {
                Ok(Some(_)) => self.complete_grapheme = true,
                Ok(None) => {}
                Err(GraphemeIncomplete::NextChunk) => {
                    self.read_offset = *start + page.len() as u64;
                    self.page = None;
                    return Ok(None);
                }
                Err(GraphemeIncomplete::PreContext(end)) => {
                    self.context_end = Some(self.source.start + end as u64);
                    return Ok(None);
                }
                Err(_) => return Err(CodeLayoutError::InvalidPage),
            }
        }
    }

    fn finish_grapheme(&mut self, end: u64) {
        self.grapheme_start = end;
        self.grapheme.clear();
        self.oversize = false;
        self.complete_grapheme = false;
    }

    fn take_pending(&mut self) -> Segment {
        let text = std::mem::replace(&mut self.pending, String::with_capacity(self.cap));
        let source = self.pending_start..self.pending_end.offset;
        self.pending_start = self.pending_end.offset;
        Segment {
            source,
            kind: SegmentKind::Text(text),
            checkpoint: self.pending_end.clone(),
        }
    }
}
