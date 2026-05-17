use std::{mem::size_of, ops::Range};

use gpui::{Pixels, px};

use super::{CodePanelScrollChrome, CodePanelWrapMode, DEFAULT_CODE_PANEL_LINE_HEIGHT};

#[allow(dead_code)]
pub(crate) fn smart_wrap_for_columns(text: &str, columns: usize) -> String {
    if text.is_empty() || columns == 0 {
        return text.to_string();
    }

    split_code_panel_source_lines(text)
        .into_iter()
        .map(|line| wrap_line_segments_for_columns(line.as_str(), columns).join("\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelDisplayProjection {
    display_lines: Vec<CodePanelDisplayLine>,
    max_display_text: String,
}

impl CodePanelDisplayProjection {
    pub(crate) fn new(source: &str, wrap_mode: CodePanelWrapMode) -> Self {
        let display_lines = code_panel_display_lines(source, wrap_mode);
        let max_display_text = display_lines
            .iter()
            .max_by_key(|line| line.display_text.chars().count())
            .map(|line| line.display_text.clone())
            .unwrap_or_default();

        Self {
            display_lines,
            max_display_text,
        }
    }

    pub(crate) fn display_line_count(&self) -> usize {
        self.display_lines.len()
    }

    #[allow(dead_code)]
    pub(crate) fn display_lines(&self) -> &[CodePanelDisplayLine] {
        self.display_lines.as_slice()
    }

    pub(crate) fn display_lines_for_window(
        &self,
        range: Range<usize>,
    ) -> Vec<CodePanelDisplayLine> {
        self.display_lines
            .get(range)
            .map_or_else(Vec::new, <[_]>::to_vec)
    }

    pub(super) fn display_window(
        &self,
        viewport_height: Option<Pixels>,
        scroll_chrome: Option<&CodePanelScrollChrome>,
        overscan_lines: usize,
        row_height: Pixels,
    ) -> CodePanelDisplayWindow {
        code_panel_display_window_for_row_height(
            self.display_line_count(),
            viewport_height,
            scroll_chrome,
            overscan_lines,
            row_height,
        )
    }

    pub(crate) fn max_display_text(&self) -> &str {
        self.max_display_text.as_str()
    }

    pub(crate) fn estimated_retained_bytes(&self) -> usize {
        self.max_display_text.len().saturating_add(
            self.display_lines
                .iter()
                .map(|line| {
                    line.display_text
                        .len()
                        .saturating_add(line.raw_text.len())
                        .saturating_add(size_of::<CodePanelDisplayLine>())
                })
                .sum::<usize>(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CodePanelDisplayWindow {
    pub(crate) range: Range<usize>,
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
    pub(crate) content_height: Pixels,
    pub(crate) display_line_count: usize,
}

impl CodePanelDisplayWindow {
    pub(super) fn pending(viewport_height: Pixels) -> Self {
        Self {
            range: 0..0,
            top_spacer_height: Pixels::ZERO,
            bottom_spacer_height: viewport_height.max(Pixels::ZERO),
            content_height: viewport_height.max(Pixels::ZERO),
            display_line_count: 0,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn code_panel_display_window(
    display_line_count: usize,
    viewport_height: Option<Pixels>,
    scroll_chrome: Option<&CodePanelScrollChrome>,
    overscan_lines: usize,
) -> CodePanelDisplayWindow {
    code_panel_display_window_for_row_height(
        display_line_count,
        viewport_height,
        scroll_chrome,
        overscan_lines,
        px(DEFAULT_CODE_PANEL_LINE_HEIGHT),
    )
}

fn code_panel_display_window_for_row_height(
    display_line_count: usize,
    viewport_height: Option<Pixels>,
    scroll_chrome: Option<&CodePanelScrollChrome>,
    overscan_lines: usize,
    row_height: Pixels,
) -> CodePanelDisplayWindow {
    let row_height = row_height.max(px(1.0));
    let content_height = row_height * display_line_count.max(1) as f32;
    let Some(viewport_height) = viewport_height else {
        return CodePanelDisplayWindow {
            range: 0..display_line_count,
            top_spacer_height: Pixels::ZERO,
            bottom_spacer_height: Pixels::ZERO,
            content_height,
            display_line_count,
        };
    };
    if display_line_count == 0 || viewport_height <= Pixels::ZERO {
        return CodePanelDisplayWindow {
            range: 0..0,
            top_spacer_height: Pixels::ZERO,
            bottom_spacer_height: content_height,
            content_height,
            display_line_count,
        };
    }

    let handle_viewport_height = scroll_chrome
        .map(|chrome| chrome.handle.bounds().size.height)
        .filter(|height| *height > Pixels::ZERO)
        .unwrap_or(viewport_height);
    let scroll_offset = scroll_chrome
        .map(|chrome| -chrome.handle.offset().y)
        .unwrap_or(Pixels::ZERO);
    let max_scroll_offset = (content_height - handle_viewport_height).max(Pixels::ZERO);
    let scroll_offset = scroll_offset.clamp(Pixels::ZERO, max_scroll_offset);
    let first_visible_row = (f32::from(scroll_offset) / f32::from(row_height)).floor() as usize;
    let visible_end_row =
        (f32::from(scroll_offset + handle_viewport_height) / f32::from(row_height)).ceil() as usize;
    let start = first_visible_row
        .min(display_line_count)
        .saturating_sub(overscan_lines);
    let end = visible_end_row
        .saturating_add(overscan_lines)
        .min(display_line_count)
        .max(start);
    let rendered_height = row_height * end.saturating_sub(start) as f32;
    let top_spacer_height = row_height * start as f32;
    let bottom_spacer_height =
        (content_height - top_spacer_height - rendered_height).max(Pixels::ZERO);

    CodePanelDisplayWindow {
        range: start..end,
        top_spacer_height,
        bottom_spacer_height,
        content_height,
        display_line_count,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelDisplayLine {
    pub display_text: String,
    pub raw_text: String,
    pub break_before: usize,
    pub source_range: Range<usize>,
}

pub(crate) fn code_panel_display_lines(
    source: &str,
    wrap_mode: CodePanelWrapMode,
) -> Vec<CodePanelDisplayLine> {
    let source_lines = split_code_panel_source_lines_with_ranges(source);
    let mut display_lines = Vec::new();

    for source_line in source_lines {
        let segments = match wrap_mode {
            CodePanelWrapMode::Smart { columns } if columns > 0 => {
                wrap_line_segments_for_columns_with_ranges(source_line.text.as_str(), columns)
            }
            CodePanelWrapMode::Smart { .. } | CodePanelWrapMode::NoWrap => {
                vec![CodePanelLineSegment {
                    text: source_line.text.clone(),
                    byte_range: 0..source_line.text.len(),
                }]
            }
        };

        for (segment_index, segment) in segments.into_iter().enumerate() {
            let source_range = source_line.source_range.start + segment.byte_range.start
                ..source_line.source_range.start + segment.byte_range.end;
            display_lines.push(CodePanelDisplayLine {
                display_text: segment.text.clone(),
                raw_text: segment.text,
                break_before: usize::from(segment_index == 0),
                source_range,
            });
        }
    }

    if display_lines.is_empty() {
        display_lines.push(CodePanelDisplayLine {
            display_text: String::new(),
            raw_text: String::new(),
            break_before: 0,
            source_range: 0..0,
        });
    }

    display_lines
}

#[allow(dead_code)]
fn split_code_panel_source_lines(text: &str) -> Vec<String> {
    split_code_panel_source_lines_with_ranges(text)
        .into_iter()
        .map(|line| line.text)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodePanelSourceLine {
    text: String,
    source_range: Range<usize>,
}

fn split_code_panel_source_lines_with_ranges(source: &str) -> Vec<CodePanelSourceLine> {
    if source.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut line_start = 0usize;
    loop {
        let Some(newline_offset) = source[line_start..].find('\n') else {
            lines.push(CodePanelSourceLine {
                text: source[line_start..].to_string(),
                source_range: line_start..source.len(),
            });
            break;
        };

        let newline_index = line_start + newline_offset;
        let content_end = if newline_index > line_start
            && source.as_bytes().get(newline_index - 1) == Some(&b'\r')
        {
            newline_index - 1
        } else {
            newline_index
        };
        lines.push(CodePanelSourceLine {
            text: source[line_start..content_end].to_string(),
            source_range: line_start..content_end,
        });
        line_start = newline_index + 1;

        if line_start == source.len() {
            lines.push(CodePanelSourceLine {
                text: String::new(),
                source_range: line_start..line_start,
            });
            break;
        }
    }

    lines
}

#[allow(dead_code)]
fn wrap_line_segments_for_columns(line: &str, columns: usize) -> Vec<String> {
    wrap_line_segments_for_columns_with_ranges(line, columns)
        .into_iter()
        .map(|segment| segment.text)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodePanelLineSegment {
    text: String,
    byte_range: Range<usize>,
}

fn wrap_line_segments_for_columns_with_ranges(
    line: &str,
    columns: usize,
) -> Vec<CodePanelLineSegment> {
    if line.is_empty() {
        return vec![CodePanelLineSegment {
            text: String::new(),
            byte_range: 0..0,
        }];
    }
    if columns == 0 {
        return vec![CodePanelLineSegment {
            text: line.to_string(),
            byte_range: 0..line.len(),
        }];
    }

    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut segments = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= columns {
            let start_byte = chars[start].0;
            segments.push(CodePanelLineSegment {
                text: line[start_byte..].to_string(),
                byte_range: start_byte..line.len(),
            });
            break;
        }

        let window_end = start + columns;
        let break_index = (start..window_end)
            .rev()
            .find(|&index| matches!(chars[index].1, ' ' | ',' | ';'))
            .unwrap_or(window_end - 1);

        let start_byte = chars[start].0;
        let end_char = break_index + 1;
        let end_byte = chars
            .get(end_char)
            .map_or_else(|| line.len(), |(byte_index, _)| *byte_index);
        segments.push(CodePanelLineSegment {
            text: line[start_byte..end_byte].to_string(),
            byte_range: start_byte..end_byte,
        });
        start = break_index + 1;
    }

    segments
}
