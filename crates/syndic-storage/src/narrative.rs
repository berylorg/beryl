mod parser;
mod sink;

use std::{mem::size_of, ops::Range};

use crate::{MarkdownBlockId, MarkdownBlockKind, ProjectionPayload, ProjectionSourceRange};

pub const NARRATIVE_MAX_BYTES: usize = 65_536;
pub const NARRATIVE_MAX_RUNS: usize = 4_096;
pub const NARRATIVE_MAX_DEPTH: usize = 16;
pub const NARRATIVE_FRAGMENT_MAX_BYTES: usize = 4_096;
pub const NARRATIVE_MAX_FRAGMENTS: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NarrativeStyle {
    pub emphasis: bool,
    pub strong: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub link_label: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NarrativeRun {
    pub display: Range<usize>,
    pub style: NarrativeStyle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NarrativeMappingKind {
    Authored,
    Transformed,
    OmittedSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NarrativeMapping {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub kind: NarrativeMappingKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NarrativeFragment {
    pub display: Range<usize>,
    pub runs: Range<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NarrativeDisposition {
    Styled,
    LiteralFallback,
    LiteralContinuation,
    LiteralSyntaxLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NarrativeTreatment {
    Paragraph,
    Heading(u8),
    Quote,
    List,
    ThematicBreak,
    Literal,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PreparedNarrative {
    block_id: MarkdownBlockId,
    block_kind: MarkdownBlockKind,
    span_ordinal: u64,
    source_range: ProjectionSourceRange,
    source: Box<str>,
    display: Option<String>,
    runs: Vec<NarrativeRun>,
    mappings: Vec<NarrativeMapping>,
    fragments: Vec<NarrativeFragment>,
    disposition: NarrativeDisposition,
    treatment: NarrativeTreatment,
    list_marker: Option<Range<usize>>,
    retained_bytes: usize,
}

impl PreparedNarrative {
    pub fn block_id(&self) -> MarkdownBlockId {
        self.block_id
    }
    pub fn block_kind(&self) -> MarkdownBlockKind {
        self.block_kind
    }
    pub fn span_ordinal(&self) -> u64 {
        self.span_ordinal
    }
    pub fn source_range(&self) -> ProjectionSourceRange {
        self.source_range
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn display(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.source)
    }
    pub fn runs(&self) -> &[NarrativeRun] {
        &self.runs
    }
    pub fn mappings(&self) -> &[NarrativeMapping] {
        &self.mappings
    }
    pub fn fragments(&self) -> &[NarrativeFragment] {
        &self.fragments
    }
    pub fn disposition(&self) -> NarrativeDisposition {
        self.disposition
    }
    pub fn treatment(&self) -> NarrativeTreatment {
        self.treatment
    }

    pub fn list_marker(&self) -> Option<&str> {
        self.source.get(self.list_marker.clone()?)
    }
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub fn snap_display_endpoint(&self, offset: usize, forward: bool) -> Option<usize> {
        let display = self.display();
        if offset > display.len() {
            return None;
        }
        let mut endpoint = offset;
        while !display.is_char_boundary(endpoint) {
            if forward {
                endpoint += 1;
            } else {
                endpoint -= 1;
            }
        }
        for mapping in &self.mappings {
            if mapping.kind == NarrativeMappingKind::Transformed
                && mapping.display.start < endpoint
                && endpoint < mapping.display.end
            {
                return Some(if forward {
                    mapping.display.end
                } else {
                    mapping.display.start
                });
            }
        }
        Some(endpoint)
    }

    pub fn selected_display(&self, display: Range<usize>) -> Option<&str> {
        self.source_range_for_display(display.clone())?;
        self.display().get(display)
    }

    pub fn source_range_for_display(&self, display: Range<usize>) -> Option<Range<usize>> {
        if display.start > display.end
            || self.snap_display_endpoint(display.start, false)? != display.start
            || self.snap_display_endpoint(display.end, true)? != display.end
        {
            return None;
        }
        let mut source: Option<Range<usize>> = None;
        for mapping in &self.mappings {
            let overlap =
                display.start.max(mapping.display.start)..display.end.min(mapping.display.end);
            let selected = match mapping.kind {
                NarrativeMappingKind::OmittedSyntax => {
                    if display.start < display.end
                        && display.start <= mapping.display.start
                        && display.end >= mapping.display.end
                    {
                        Some(mapping.source.clone())
                    } else {
                        None
                    }
                }
                NarrativeMappingKind::Transformed => {
                    if overlap.start < overlap.end {
                        Some(mapping.source.clone())
                    } else {
                        None
                    }
                }
                NarrativeMappingKind::Authored => {
                    if overlap.start < overlap.end {
                        let start = mapping.source.start + overlap.start - mapping.display.start;
                        Some(start..start + overlap.len())
                    } else {
                        None
                    }
                }
            };
            if let Some(selected) = selected {
                source = Some(match source {
                    Some(previous) => {
                        previous.start.min(selected.start)..previous.end.max(selected.end)
                    }
                    None => selected,
                });
            }
        }
        if let Some(source) = source {
            self.source.get(source.clone())?;
            return Some(source);
        }
        if display.start != display.end {
            return None;
        }
        for mapping in &self.mappings {
            if mapping.kind == NarrativeMappingKind::Authored
                && mapping.display.start <= display.start
                && display.start <= mapping.display.end
            {
                let offset = mapping.source.start + display.start - mapping.display.start;
                return Some(offset..offset);
            }
            if mapping.kind == NarrativeMappingKind::Transformed
                && mapping.display.start == display.start
            {
                return Some(mapping.source.start..mapping.source.start);
            }
        }
        Some(self.source.len()..self.source.len())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NarrativePreparationErrorKind {
    #[error("invalid narrative source binding")]
    InvalidBinding,
    #[error("insufficient narrative preparation capacity")]
    Capacity,
}

#[derive(Debug, thiserror::Error)]
#[error("{kind}")]
pub struct NarrativePreparationError {
    kind: NarrativePreparationErrorKind,
    payload: ProjectionPayload,
}

impl NarrativePreparationError {
    pub fn kind(&self) -> NarrativePreparationErrorKind {
        self.kind
    }
    pub fn payload(&self) -> &ProjectionPayload {
        &self.payload
    }
    pub fn into_payload(self) -> ProjectionPayload {
        self.payload
    }
}

pub fn prepare_narrative(
    payload: ProjectionPayload,
    allowance: usize,
) -> Result<PreparedNarrative, NarrativePreparationError> {
    match prepare(&payload, allowance) {
        Ok(mut prepared) => {
            let ProjectionPayload::InlineMarkdown { source, .. } = payload else {
                unreachable!("validated narrative payload")
            };
            prepared.source = source;
            Ok(prepared)
        }
        Err(kind) => Err(NarrativePreparationError { kind, payload }),
    }
}

fn prepare(
    payload: &ProjectionPayload,
    allowance: usize,
) -> Result<PreparedNarrative, NarrativePreparationErrorKind> {
    use NarrativePreparationErrorKind::{Capacity, InvalidBinding};
    let ProjectionPayload::InlineMarkdown {
        block_id,
        block_kind,
        span_ordinal,
        source_range,
        source,
    } = payload
    else {
        return Err(InvalidBinding);
    };
    if source.len() > NARRATIVE_MAX_BYTES
        || source.is_empty()
        || *span_ordinal == 0
        || source_range.len() != source.len() as u64
        || matches!(
            block_kind,
            MarkdownBlockKind::FencedCode | MarkdownBlockKind::Table
        )
        || matches!(block_kind, MarkdownBlockKind::Heading(level) if !(1..=6).contains(level))
    {
        return Err(InvalidBinding);
    }
    let mut disposition = if *span_ordinal > 1 {
        NarrativeDisposition::LiteralContinuation
    } else if *block_kind == MarkdownBlockKind::Fallback {
        NarrativeDisposition::LiteralFallback
    } else {
        NarrativeDisposition::Styled
    };
    let mut plan = sink::Sink::count();
    if disposition == NarrativeDisposition::Styled
        && parser::parse(source, *block_kind, &mut plan).is_err()
    {
        disposition = NarrativeDisposition::LiteralSyntaxLimit;
    }
    if disposition != NarrativeDisposition::Styled {
        plan = sink::Sink::count();
        plan.literal(source).map_err(|_| Capacity)?;
    }
    let derived_display = disposition == NarrativeDisposition::Styled;
    let minimum = charge(
        source.len(),
        if derived_display { plan.display_len } else { 0 },
        plan.run_count,
        plan.map_count,
        plan.fragment_count,
    )
    .ok_or(Capacity)?;
    if minimum > allowance {
        return Err(Capacity);
    }
    let mut output = sink::Sink::allocate(&plan, derived_display).map_err(|_| Capacity)?;
    let retained_bytes = charge(
        source.len(),
        output.display_capacity(),
        output.run_capacity(),
        output.map_capacity(),
        output.fragment_capacity(),
    )
    .ok_or(Capacity)?;
    if retained_bytes > allowance {
        return Err(Capacity);
    }
    if derived_display {
        parser::parse(source, *block_kind, &mut output).map_err(|_| InvalidBinding)?;
    } else {
        output.literal(source).map_err(|_| Capacity)?;
    }
    let treatment = if disposition != NarrativeDisposition::Styled {
        NarrativeTreatment::Literal
    } else {
        match block_kind {
            MarkdownBlockKind::Heading(level) => NarrativeTreatment::Heading(*level),
            MarkdownBlockKind::BlockQuote => NarrativeTreatment::Quote,
            MarkdownBlockKind::List => NarrativeTreatment::List,
            MarkdownBlockKind::ThematicBreak => NarrativeTreatment::ThematicBreak,
            _ => NarrativeTreatment::Paragraph,
        }
    };
    Ok(PreparedNarrative {
        block_id: *block_id,
        block_kind: *block_kind,
        span_ordinal: *span_ordinal,
        source_range: *source_range,
        source: Box::default(),
        display: output.display,
        runs: output.runs.unwrap_or_default(),
        mappings: output.mappings.unwrap_or_default(),
        fragments: output.fragments.unwrap_or_default(),
        disposition,
        treatment,
        list_marker: if treatment == NarrativeTreatment::List {
            let prefix =
                parser::block_prefix(source.split('\n').next().unwrap_or(source), *block_kind);
            let marker = source.get(..prefix).ok_or(InvalidBinding)?;
            let start = marker.len() - marker.trim_start().len();
            let end = marker.trim_end().len();
            (start < end).then_some(start..end)
        } else {
            None
        },
        retained_bytes,
    })
}

fn charge(
    source: usize,
    display: usize,
    runs: usize,
    maps: usize,
    fragments: usize,
) -> Option<usize> {
    size_of::<PreparedNarrative>()
        .checked_add(source)?
        .checked_add(display)?
        .checked_add(runs.checked_mul(size_of::<NarrativeRun>())?)?
        .checked_add(maps.checked_mul(size_of::<NarrativeMapping>())?)?
        .checked_add(fragments.checked_mul(size_of::<NarrativeFragment>())?)
}
