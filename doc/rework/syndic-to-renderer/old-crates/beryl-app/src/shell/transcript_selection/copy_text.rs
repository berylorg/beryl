use std::{collections::HashMap, ops::Range};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptLineCopyText {
    line_prefix: String,
    start_prefix: String,
    runs: Vec<TranscriptLineCopyRun>,
    group: Option<TranscriptLineCopyGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptLineCopyRun {
    display_range: Range<usize>,
    display_text: String,
    copy_prefix: String,
    copy_suffix: String,
    copy_replacement: Option<String>,
    atomic_replacements: Vec<TranscriptLineAtomicReplacement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptLineAtomicReplacement {
    display_range: Range<usize>,
    copy_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptLineCopyGroup {
    id: String,
    opening: String,
    closing: String,
}

pub(crate) struct TranscriptSelectedLineCopy<'a> {
    pub(crate) copy_text: &'a TranscriptLineCopyText,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) break_before: usize,
}

impl TranscriptLineCopyText {
    pub(crate) fn plain(display_text: impl Into<String>) -> Self {
        let display_text = display_text.into();
        let mut copy_text = Self::default();
        copy_text.push_plain_run(display_text);
        copy_text
    }

    pub(crate) fn with_prefixes(mut self, line_prefix: String, start_prefix: String) -> Self {
        self.line_prefix = line_prefix;
        self.start_prefix = start_prefix;
        self
    }

    pub(crate) fn with_group(mut self, group: TranscriptLineCopyGroup) -> Self {
        self.group = Some(group);
        self
    }

    pub(crate) fn push_plain_run(&mut self, display_text: String) {
        self.push_wrapped_run(display_text, String::new(), String::new());
    }

    pub(crate) fn push_wrapped_run(
        &mut self,
        display_text: String,
        copy_prefix: String,
        copy_suffix: String,
    ) {
        self.push_run(display_text, copy_prefix, copy_suffix, None, Vec::new());
    }

    pub(crate) fn push_wrapped_run_with_atomic_replacements(
        &mut self,
        display_text: String,
        copy_prefix: String,
        copy_suffix: String,
        atomic_replacements: impl IntoIterator<Item = (Range<usize>, String)>,
    ) {
        self.push_run(
            display_text,
            copy_prefix,
            copy_suffix,
            None,
            atomic_replacements.into_iter().collect(),
        );
    }

    pub(crate) fn push_atomic_run(&mut self, display_text: String, copy_text: String) {
        self.push_run(
            display_text,
            String::new(),
            String::new(),
            Some(copy_text),
            Vec::new(),
        );
    }

    fn push_run(
        &mut self,
        display_text: String,
        copy_prefix: String,
        copy_suffix: String,
        copy_replacement: Option<String>,
        atomic_replacements: Vec<(Range<usize>, String)>,
    ) {
        if display_text.is_empty() {
            return;
        }

        let start = self.runs.last().map_or(0, |run| run.display_range.end);
        let end = start.saturating_add(display_text.len());
        let mut atomic_replacements = atomic_replacements
            .into_iter()
            .filter_map(|(range, copy_text)| {
                if range.start >= range.end
                    || range.end > display_text.len()
                    || !display_text.is_char_boundary(range.start)
                    || !display_text.is_char_boundary(range.end)
                    || copy_text.is_empty()
                {
                    return None;
                }
                Some(TranscriptLineAtomicReplacement {
                    display_range: start + range.start..start + range.end,
                    copy_text,
                })
            })
            .collect::<Vec<_>>();
        atomic_replacements.sort_by_key(|replacement| replacement.display_range.start);
        let mut previous_end = start;
        atomic_replacements.retain(|replacement| {
            if replacement.display_range.start < previous_end {
                return false;
            }
            previous_end = replacement.display_range.end;
            true
        });
        self.runs.push(TranscriptLineCopyRun {
            display_range: start..end,
            display_text,
            copy_prefix,
            copy_suffix,
            copy_replacement,
            atomic_replacements,
        });
    }

    pub(crate) fn atomic_ranges(&self) -> Vec<Range<usize>> {
        self.runs
            .iter()
            .flat_map(|run| {
                run.copy_replacement
                    .as_ref()
                    .map(|_| run.display_range.clone())
                    .into_iter()
                    .chain(
                        run.atomic_replacements
                            .iter()
                            .map(|replacement| replacement.display_range.clone()),
                    )
            })
            .collect()
    }

    fn selected_content(&self, start: usize, end: usize, include_start_prefix: bool) -> String {
        let mut selected = String::new();
        selected.push_str(&self.line_prefix);
        if include_start_prefix && start == 0 {
            selected.push_str(&self.start_prefix);
        }
        for run in &self.runs {
            if let Some(text) = run.selected_text(start, end) {
                selected.push_str(text.as_str());
            }
        }
        selected
    }

    fn group(&self) -> Option<&TranscriptLineCopyGroup> {
        self.group.as_ref()
    }

    fn group_opening(&self, start: usize) -> Option<String> {
        let group = self.group.as_ref()?;
        let mut opening = String::new();
        opening.push_str(&self.line_prefix);
        if start == 0 {
            opening.push_str(&self.start_prefix);
        }
        opening.push_str(group.opening.as_str());
        opening.push('\n');
        Some(opening)
    }

    fn group_closing(&self) -> Option<String> {
        let group = self.group.as_ref()?;
        let mut closing = String::new();
        closing.push('\n');
        closing.push_str(&self.line_prefix);
        closing.push_str(group.closing.as_str());
        Some(closing)
    }
}

impl TranscriptLineCopyRun {
    fn selected_text(&self, start: usize, end: usize) -> Option<String> {
        let selected_start = start.max(self.display_range.start);
        let selected_end = end.min(self.display_range.end);
        if selected_start >= selected_end {
            return None;
        }

        if let Some(copy_replacement) = &self.copy_replacement {
            return Some(copy_replacement.clone());
        }

        if !self.atomic_replacements.is_empty() {
            return self.selected_text_with_atomic_replacements(selected_start, selected_end);
        }

        let relative_start = selected_start.saturating_sub(self.display_range.start);
        let relative_end = selected_end.saturating_sub(self.display_range.start);
        let selected_display_text = self
            .display_text
            .get(relative_start..relative_end)?
            .to_string();
        let mut selected = String::new();
        selected.push_str(&self.copy_prefix);
        selected.push_str(selected_display_text.as_str());
        selected.push_str(&self.copy_suffix);
        Some(selected)
    }

    fn selected_text_with_atomic_replacements(
        &self,
        selected_start: usize,
        selected_end: usize,
    ) -> Option<String> {
        let mut selected_display = String::new();
        let mut cursor = selected_start;

        for replacement in &self.atomic_replacements {
            if !ranges_intersect(&(selected_start..selected_end), &replacement.display_range) {
                continue;
            }

            let plain_end = replacement.display_range.start.min(selected_end);
            if cursor < plain_end {
                let relative_start = cursor.saturating_sub(self.display_range.start);
                let relative_end = plain_end.saturating_sub(self.display_range.start);
                selected_display.push_str(self.display_text.get(relative_start..relative_end)?);
            }
            selected_display.push_str(&replacement.copy_text);
            cursor = cursor.max(replacement.display_range.end.min(selected_end));
        }

        if cursor < selected_end {
            let relative_start = cursor.saturating_sub(self.display_range.start);
            let relative_end = selected_end.saturating_sub(self.display_range.start);
            selected_display.push_str(self.display_text.get(relative_start..relative_end)?);
        }

        let mut selected = String::new();
        selected.push_str(&self.copy_prefix);
        selected.push_str(selected_display.as_str());
        selected.push_str(&self.copy_suffix);
        Some(selected)
    }
}

impl TranscriptLineCopyGroup {
    pub(crate) fn new(
        id: impl Into<String>,
        opening: impl Into<String>,
        closing: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            opening: opening.into(),
            closing: closing.into(),
        }
    }
}

pub(crate) fn selected_text_from_copy_lines(lines: &[TranscriptSelectedLineCopy<'_>]) -> String {
    let group_positions = selected_group_positions(lines);
    let mut selected = String::new();
    let mut first_line = true;

    for (index, line) in lines.iter().enumerate() {
        if first_line {
            first_line = false;
        } else if line.break_before > 0 {
            selected.push_str(&"\n".repeat(line.break_before));
        }

        let group = line.copy_text.group();
        let is_group_first = group.is_some_and(|group| {
            group_positions
                .get(group.id.as_str())
                .is_some_and(|(first, _)| *first == index)
        });
        let is_group_last = group.is_some_and(|group| {
            group_positions
                .get(group.id.as_str())
                .is_some_and(|(_, last)| *last == index)
        });

        if is_group_first && let Some(opening) = line.copy_text.group_opening(line.start) {
            selected.push_str(opening.as_str());
        }

        selected.push_str(
            line.copy_text
                .selected_content(line.start, line.end, group.is_none())
                .as_str(),
        );

        if is_group_last && let Some(closing) = line.copy_text.group_closing() {
            selected.push_str(closing.as_str());
        }
    }

    selected
}

fn selected_group_positions(
    lines: &[TranscriptSelectedLineCopy<'_>],
) -> HashMap<String, (usize, usize)> {
    let mut positions = HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(group) = line.copy_text.group() else {
            continue;
        };
        positions
            .entry(group.id.clone())
            .and_modify(|(_, last)| *last = index)
            .or_insert((index, index));
    }
    positions
}

fn ranges_intersect(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}
