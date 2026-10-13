use super::*;

pub(super) struct Sink {
    pub display: Option<String>,
    pub runs: Option<Vec<NarrativeRun>>,
    pub mappings: Option<Vec<NarrativeMapping>>,
    pub fragments: Option<Vec<NarrativeFragment>>,
    pub display_len: usize,
    pub run_count: usize,
    pub map_count: usize,
    pub fragment_count: usize,
    last_run: Option<NarrativeRun>,
    last_mapping: Option<NarrativeMapping>,
    fragment_start: usize,
}

impl Sink {
    pub fn count() -> Self {
        Self {
            display: None,
            runs: None,
            mappings: None,
            fragments: None,
            display_len: 0,
            run_count: 0,
            map_count: 0,
            fragment_count: 0,
            last_run: None,
            last_mapping: None,
            fragment_start: 0,
        }
    }

    pub fn allocate(plan: &Self, display: bool) -> Result<Self, ()> {
        let mut output = Self::count();
        if display {
            let mut text = String::new();
            text.try_reserve_exact(plan.display_len).map_err(|_| ())?;
            output.display = Some(text);
        }
        let mut runs = Vec::new();
        let mut mappings = Vec::new();
        let mut fragments = Vec::new();
        runs.try_reserve_exact(plan.run_count).map_err(|_| ())?;
        mappings.try_reserve_exact(plan.map_count).map_err(|_| ())?;
        fragments
            .try_reserve_exact(plan.fragment_count)
            .map_err(|_| ())?;
        output.runs = Some(runs);
        output.mappings = Some(mappings);
        output.fragments = Some(fragments);
        Ok(output)
    }

    pub fn display_capacity(&self) -> usize {
        self.display.as_ref().map_or(0, String::capacity)
    }
    pub fn run_capacity(&self) -> usize {
        self.runs.as_ref().map_or(0, Vec::capacity)
    }
    pub fn map_capacity(&self) -> usize {
        self.mappings.as_ref().map_or(0, Vec::capacity)
    }
    pub fn fragment_capacity(&self) -> usize {
        self.fragments.as_ref().map_or(0, Vec::capacity)
    }

    pub fn literal(&mut self, source: &str) -> Result<(), ()> {
        self.text(
            source,
            0..source.len(),
            source,
            NarrativeStyle::default(),
            false,
        )
    }

    pub fn syntax(&mut self, source: Range<usize>, display: Range<usize>) -> Result<(), ()> {
        if source.is_empty() {
            return Ok(());
        }
        self.mapping(NarrativeMapping {
            source,
            display,
            kind: NarrativeMappingKind::OmittedSyntax,
        })
    }

    fn mapping(&mut self, value: NarrativeMapping) -> Result<(), ()> {
        let merge = self.last_mapping.as_ref().is_some_and(|previous| {
            previous.kind == NarrativeMappingKind::Authored
                && value.kind == previous.kind
                && previous.source.end == value.source.start
                && previous.display.end == value.display.start
        });
        if merge {
            let previous = self.last_mapping.as_mut().ok_or(())?;
            previous.source.end = value.source.end;
            previous.display.end = value.display.end;
            if let Some(mappings) = &mut self.mappings {
                *mappings.last_mut().ok_or(())? = previous.clone();
            }
        } else {
            self.map_count += 1;
            self.last_mapping = Some(value.clone());
            if let Some(mappings) = &mut self.mappings {
                if mappings.len() == mappings.capacity() {
                    return Err(());
                }
                mappings.push(value);
            }
        }
        Ok(())
    }

    pub fn text(
        &mut self,
        source: &str,
        raw: Range<usize>,
        text: &str,
        style: NarrativeStyle,
        transformed: bool,
    ) -> Result<(), ()> {
        if source.get(raw.clone()).is_none() {
            return Err(());
        }
        let start = self.display_len;
        let mut remainder = text;
        while !remainder.is_empty() {
            let available = NARRATIVE_FRAGMENT_MAX_BYTES - (self.display_len - self.fragment_start);
            let mut count = remainder.len().min(available);
            while !remainder.is_char_boundary(count) {
                count -= 1;
            }
            if count == 0 {
                self.fragment_start = self.display_len;
                self.last_run = None;
                continue;
            }
            if self.display_len == self.fragment_start {
                self.fragment_count += 1;
                if self.fragment_count > NARRATIVE_MAX_FRAGMENTS {
                    return Err(());
                }
                if let Some(fragments) = &mut self.fragments {
                    if fragments.len() == fragments.capacity() {
                        return Err(());
                    }
                    fragments.push(NarrativeFragment {
                        display: self.display_len..self.display_len,
                        runs: self.run_count..self.run_count,
                    });
                }
            }
            let end = self.display_len + count;
            if end > NARRATIVE_MAX_BYTES {
                return Err(());
            }
            if let Some(display) = &mut self.display {
                if end > display.capacity() {
                    return Err(());
                }
                display.push_str(&remainder[..count]);
            }
            if let Some(previous) = &mut self.last_run {
                if previous.style == style {
                    previous.display.end = end;
                    if let Some(runs) = &mut self.runs {
                        *runs.last_mut().ok_or(())? = previous.clone();
                    }
                } else {
                    self.run(self.display_len..end, style)?;
                }
            } else {
                self.run(self.display_len..end, style)?;
            }
            self.display_len = end;
            if let Some(fragments) = &mut self.fragments {
                let fragment = fragments.last_mut().ok_or(())?;
                fragment.display.end = end;
                fragment.runs.end = self.run_count;
            }
            remainder = &remainder[count..];
            if self.display_len - self.fragment_start == NARRATIVE_FRAGMENT_MAX_BYTES {
                self.fragment_start = self.display_len;
                self.last_run = None;
            }
        }
        self.mapping(NarrativeMapping {
            source: raw,
            display: start..self.display_len,
            kind: if transformed {
                NarrativeMappingKind::Transformed
            } else {
                NarrativeMappingKind::Authored
            },
        })
    }

    fn run(&mut self, display: Range<usize>, style: NarrativeStyle) -> Result<(), ()> {
        self.run_count += 1;
        if self.run_count > NARRATIVE_MAX_RUNS {
            return Err(());
        }
        let run = NarrativeRun { display, style };
        self.last_run = Some(run.clone());
        if let Some(runs) = &mut self.runs {
            if runs.len() == runs.capacity() {
                return Err(());
            }
            runs.push(run);
        }
        Ok(())
    }
}
