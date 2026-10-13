use std::{mem::size_of, ops::Range, sync::Arc};

use syndic_storage::narrative::{
    NARRATIVE_FRAGMENT_MAX_BYTES, NARRATIVE_MAX_BYTES, NARRATIVE_MAX_FRAGMENTS, NARRATIVE_MAX_RUNS,
    NarrativeFragment, NarrativeMapping, NarrativeMappingKind, NarrativeRun, NarrativeStyle,
};

use super::narrative_layout::NarrativeDisplay;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LiteralPreparationError {
    SourceCapacity,
    FragmentCapacity,
    RunCapacity,
    RetainedCapacity,
    ChargeOverflow,
}

#[derive(Debug)]
pub(super) struct PreparedLiteralNarrative {
    source: Arc<str>,
    runs: Vec<NarrativeRun>,
    mappings: Vec<NarrativeMapping>,
    fragments: Vec<NarrativeFragment>,
    retained_bytes: usize,
}

impl PreparedLiteralNarrative {
    pub(super) fn required_charge(source: &str) -> Result<usize, LiteralPreparationError> {
        Self::charge_for_capacities(
            Self::fragment_count(source)?,
            usize::from(!source.is_empty()),
        )
    }

    pub(super) fn prepare(
        source: Arc<str>,
        allowance: usize,
    ) -> Result<Self, LiteralPreparationError> {
        let count = Self::fragment_count(&source)?;
        let mapping_count = usize::from(!source.is_empty());
        let charge = Self::charge_for_capacities(count, mapping_count)?;
        if charge > allowance {
            return Err(LiteralPreparationError::RetainedCapacity);
        }
        let mut runs = Vec::with_capacity(count);
        let mut fragments = Vec::with_capacity(count);
        let mut mappings = Vec::with_capacity(mapping_count);
        let actual_charge = size_of::<Self>()
            .checked_add(
                runs.capacity()
                    .checked_mul(size_of::<NarrativeRun>())
                    .ok_or(LiteralPreparationError::ChargeOverflow)?,
            )
            .and_then(|charge| {
                charge.checked_add(
                    fragments
                        .capacity()
                        .checked_mul(size_of::<NarrativeFragment>())?,
                )
            })
            .and_then(|charge| {
                charge.checked_add(
                    mappings
                        .capacity()
                        .checked_mul(size_of::<NarrativeMapping>())?,
                )
            })
            .and_then(|charge| charge.checked_add(6 * size_of::<usize>()))
            .ok_or(LiteralPreparationError::ChargeOverflow)?;
        if actual_charge > allowance {
            return Err(LiteralPreparationError::RetainedCapacity);
        }
        let mut start = 0;
        while start < source.len() {
            let end = Self::fragment_end(&source, start);
            let ordinal = runs.len();
            runs.push(NarrativeRun {
                display: start..end,
                style: NarrativeStyle::default(),
            });
            fragments.push(NarrativeFragment {
                display: start..end,
                runs: ordinal..ordinal + 1,
            });
            start = end;
        }
        if !source.is_empty() {
            mappings.push(NarrativeMapping {
                source: 0..source.len(),
                display: 0..source.len(),
                kind: NarrativeMappingKind::Authored,
            });
        }
        Ok(Self {
            source,
            runs,
            mappings,
            fragments,
            retained_bytes: actual_charge,
        })
    }

    fn fragment_count(source: &str) -> Result<usize, LiteralPreparationError> {
        if source.len() > NARRATIVE_MAX_BYTES {
            return Err(LiteralPreparationError::SourceCapacity);
        }
        let mut start = 0;
        let mut count = 0;
        while start < source.len() {
            count += 1;
            if count > NARRATIVE_MAX_FRAGMENTS {
                return Err(LiteralPreparationError::FragmentCapacity);
            }
            if count > NARRATIVE_MAX_RUNS {
                return Err(LiteralPreparationError::RunCapacity);
            }
            start = Self::fragment_end(source, start);
        }
        Ok(count)
    }

    fn fragment_end(source: &str, start: usize) -> usize {
        let mut end = (start + NARRATIVE_FRAGMENT_MAX_BYTES).min(source.len());
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        end
    }

    fn charge_for_capacities(
        fragments: usize,
        mappings: usize,
    ) -> Result<usize, LiteralPreparationError> {
        fragments
            .checked_mul(size_of::<NarrativeRun>() + size_of::<NarrativeFragment>())
            .and_then(|charge| {
                charge.checked_add(mappings.checked_mul(size_of::<NarrativeMapping>())?)
            })
            .and_then(|charge| charge.checked_add(size_of::<Self>() + 6 * size_of::<usize>()))
            .ok_or(LiteralPreparationError::ChargeOverflow)
    }

    pub(super) fn source(&self) -> &str {
        &self.source
    }
    pub(super) fn display(&self) -> &str {
        &self.source
    }
    pub(super) fn runs(&self) -> &[NarrativeRun] {
        &self.runs
    }
    pub(super) fn mappings(&self) -> &[NarrativeMapping] {
        &self.mappings
    }
    pub(super) fn fragments(&self) -> &[NarrativeFragment] {
        &self.fragments
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(super) fn snap_display_endpoint(&self, offset: usize, forward: bool) -> Option<usize> {
        if offset > self.source.len() {
            return None;
        }
        let mut endpoint = offset;
        while !self.source.is_char_boundary(endpoint) {
            if forward {
                endpoint += 1;
            } else {
                endpoint -= 1;
            }
        }
        Some(endpoint)
    }

    pub(super) fn source_range_for_display(&self, display: Range<usize>) -> Option<Range<usize>> {
        self.source.get(display.clone())?;
        Some(display)
    }

    pub(super) fn selected_display(&self, display: Range<usize>) -> Option<&str> {
        self.source.get(self.source_range_for_display(display)?)
    }
}

impl NarrativeDisplay for PreparedLiteralNarrative {
    fn display(&self) -> &str {
        self.display()
    }
    fn runs(&self) -> &[NarrativeRun] {
        self.runs()
    }
    fn fragments(&self) -> &[NarrativeFragment] {
        self.fragments()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/syndic_transcript_panel/literal_narrative.rs"
    ));
}
