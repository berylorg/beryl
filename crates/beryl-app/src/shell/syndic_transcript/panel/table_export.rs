use super::super::provider::resource_index as index;
use super::super::{IndexedResourceResponse, IndexedResourceValue};
use super::copy_keyboard::CopyMethod;
use super::table_viewport::TableCellId;
use std::ops::Range;
use std::sync::Arc;

pub(super) const COPY_BYTES: usize = 65_536;
pub(super) const COPY_CHARGE: usize = COPY_BYTES * 5 + 65_536 + 32_768;

#[derive(Clone)]
struct ExportCell {
    id: TableCellId,
    bytes: Range<usize>,
    alignment: index::Alignment,
}

pub(super) struct TableExport {
    pub ranges: Vec<index::ByteRange>,
    pub source_bytes: usize,
    cells: Vec<ExportCell>,
    whole: bool,
}

impl TableExport {
    pub fn selected(
        cells: Vec<(TableCellId, index::Item)>,
        pages: &[Arc<IndexedResourceResponse>],
        binding: &index::Binding,
        whole: bool,
    ) -> Result<Self, &'static str> {
        if cells.is_empty() || cells.len() > 128 {
            return Err("Copy unavailable: unresolved selection");
        }
        let mut result = Self {
            ranges: Vec::with_capacity(cells.len()),
            cells: Vec::with_capacity(cells.len()),
            source_bytes: 0,
            whole,
        };
        if whole {
            result.add_range(binding.range)?;
        }
        let mut previous = None;
        for (id, item) in cells {
            if previous.is_some_and(|old| old >= id) {
                return Err("Copy unavailable: incoherent selection");
            }
            previous = Some(id);
            let range = match item {
                index::Item::Descriptor(index::IndexedDescriptor {
                    descriptor:
                        index::Descriptor::TableCell {
                            role, row, column, ..
                        },
                    absolute_raw: Some(range),
                }) if role == id.role && row == id.row && column == id.column => Some(range),
                index::Item::Missing { role, row, column }
                    if role == id.role && row == id.row && column == id.column =>
                {
                    None
                }
                _ => return Err("Copy unavailable: non-cell selection"),
            };
            let bytes = if let Some(range) = range {
                if range.start < binding.range.start || range.end > binding.range.end {
                    return Err("Copy unavailable: source binding changed");
                }
                if whole {
                    usize::try_from(range.start - binding.range.start)
                        .map_err(|_| "Copy unavailable: capacity")?
                        ..usize::try_from(range.end - binding.range.start)
                            .map_err(|_| "Copy unavailable: capacity")?
                } else {
                    let start = result.source_bytes;
                    result.add_range(range)?;
                    start..result.source_bytes
                }
            } else {
                0..0
            };
            let alignment = pages
                .iter()
                .find_map(|page| {
                    let IndexedResourceValue::Range(value) = &page.value else {
                        return None;
                    };
                    value.items.iter().enumerate().find_map(|(position, item)| {
                        let index::Item::Descriptor(descriptor) = item else {
                            return None;
                        };
                        match descriptor.descriptor {
                            index::Descriptor::TableColumn { ordinal, alignment }
                                if ordinal == id.column =>
                            {
                                Some(alignment)
                            }
                            index::Descriptor::TableCell {
                                role, row, column, ..
                            } if role == id.role && row == id.row && column == id.column => value
                                .presentations
                                .as_ref()
                                .and_then(|values| values.get(position))
                                .and_then(|value| value.alignment),
                            _ => None,
                        }
                    })
                })
                .unwrap_or(index::Alignment::Unspecified);
            result.cells.push(ExportCell {
                id,
                bytes,
                alignment,
            });
        }
        Ok(result)
    }

    pub fn raw(range: index::ByteRange) -> Result<Self, &'static str> {
        let mut result = Self {
            ranges: Vec::new(),
            source_bytes: 0,
            cells: Vec::new(),
            whole: true,
        };
        result.add_range(range)?;
        Ok(result)
    }

    fn add_range(&mut self, range: index::ByteRange) -> Result<(), &'static str> {
        let bytes = usize::try_from(
            range
                .end
                .checked_sub(range.start)
                .ok_or("Copy unavailable: source range")?,
        )
        .map_err(|_| "Copy unavailable: capacity")?;
        self.source_bytes = self
            .source_bytes
            .checked_add(bytes)
            .filter(|count| *count <= COPY_BYTES)
            .ok_or("Copy unavailable: source exceeds 64 KiB")?;
        if bytes != 0 {
            self.ranges.push(range);
        }
        Ok(())
    }

    pub fn render(self, source: Vec<u8>, method: CopyMethod) -> Result<String, &'static str> {
        if source.len() != self.source_bytes || source.contains(&0) {
            return Err("Copy unavailable: invalid source bytes");
        }
        if self.whole && method == CopyMethod::Markdown {
            return String::from_utf8(source).map_err(|_| "Copy unavailable: invalid UTF-8");
        }
        if self.cells.is_empty() {
            let source =
                std::str::from_utf8(&source).map_err(|_| "Copy unavailable: invalid UTF-8")?;
            return crate::transcript_provider::stripped_copy_source(source)
                .map_err(|_| "Copy unavailable: representation refused");
        }
        let mut output = String::new();
        output
            .try_reserve_exact(COPY_BYTES)
            .map_err(|_| "Copy unavailable: allocation")?;
        let mut start = 0;
        while start < self.cells.len() {
            let first = &self.cells[start];
            let mut end = start + 1;
            while end < self.cells.len()
                && self.cells[end].id.role == first.id.role
                && self.cells[end].id.row == first.id.row
            {
                end += 1;
            }
            let row = &self.cells[start..end];
            match method {
                CopyMethod::Markdown => {
                    if !output.is_empty() {
                        append(&mut output, "\n")?;
                    }
                    append(&mut output, "|")?;
                    for cell in row {
                        let raw = std::str::from_utf8(
                            source
                                .get(cell.bytes.clone())
                                .ok_or("Copy unavailable: source range")?,
                        )
                        .map_err(|_| "Copy unavailable: invalid UTF-8")?;
                        append(&mut output, raw)?;
                        append(&mut output, "|")?;
                    }
                    if first.id.role == index::RowRole::Header {
                        append(&mut output, "\n|")?;
                        for cell in row {
                            append(
                                &mut output,
                                match cell.alignment {
                                    index::Alignment::Unspecified => "---|",
                                    index::Alignment::Left => ":---|",
                                    index::Alignment::Center => ":---:|",
                                    index::Alignment::Right => "---:|",
                                },
                            )?;
                        }
                    }
                }
                CopyMethod::Stripped => {
                    let mut values = Vec::with_capacity(row.len());
                    for cell in row {
                        let raw = std::str::from_utf8(
                            source
                                .get(cell.bytes.clone())
                                .ok_or("Copy unavailable: source range")?,
                        )
                        .map_err(|_| "Copy unavailable: invalid UTF-8")?;
                        values.push(
                            crate::transcript_provider::stripped_copy_source(raw.trim())
                                .map_err(|_| "Copy unavailable: representation refused")?,
                        );
                    }
                    if let Some(last) = values.iter().rposition(|value| !value.trim().is_empty()) {
                        if !output.is_empty() {
                            append(&mut output, "\n")?;
                        }
                        for (position, value) in values[..=last].iter().enumerate() {
                            if position != 0 {
                                append(&mut output, "\t")?;
                            }
                            append(&mut output, value)?;
                        }
                    }
                }
            }
            start = end;
        }
        Ok(output)
    }
}

fn append(output: &mut String, value: &str) -> Result<(), &'static str> {
    if output
        .len()
        .checked_add(value.len())
        .is_none_or(|bytes| bytes > COPY_BYTES)
    {
        return Err("Copy unavailable: representation exceeds 64 KiB");
    }
    output.push_str(value);
    Ok(())
}
