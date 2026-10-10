use super::{codec::*, construction::ConstructionError, *};
use crate::ResourceStructure;

pub(crate) struct PreparedTableAdvance {
    pub(crate) build: Build,
    pub(crate) receipt: Progress,
    pub(crate) page: Option<Page>,
    pub(crate) locators: Vec<Locator>,
    pub(crate) anchors: Vec<AnchorLocator>,
}

pub(super) fn prepare_table(
    before: &Build,
    row: Option<&Descriptor>,
    source: &[u8],
    maximum_descriptors: usize,
) -> Result<PreparedTableAdvance, ConstructionError> {
    encode_build(before)?;
    if before.lifecycle != Lifecycle::Building {
        return Err(ConstructionError::Collision);
    }
    if source.len() > 65_536 || !(1..=256).contains(&maximum_descriptors) {
        return Err(ConstructionError::Limit);
    }
    let ResourceStructure::Table {
        body_rows,
        columns,
        header_range: header,
    } = before.binding.metadata.structure()
    else {
        return Err(ConstructionError::Unavailable);
    };
    let length = before.binding.range.end - before.binding.range.start;
    let cp = &before.checkpoint;
    let mut step = match cp.pass {
        Pass::TableRows if row.is_none() => advance_rows(
            cp,
            source,
            length,
            ByteRange {
                start: header.start(),
                end: header.end(),
            },
            *columns,
        )?,
        Pass::TableCells => {
            let row = row.ok_or(ConstructionError::Corrupt)?;
            let raw = row.raw().ok_or(ConstructionError::Corrupt)?;
            let mut input = cp.clone();
            if cp.role == RowRole::Body
                && cp.next_row == 0
                && cp.next_byte == header.end()
                && cp.phase == ParsePhase::Consuming
                && cp.syntax.is_empty()
                && cp.utf8_carry.is_empty()
            {
                if raw.start <= header.end() {
                    return Err(ConstructionError::Corrupt);
                }
                input.next_byte = raw.start;
            }
            let mut step = advance_cells(&input, row, source, length, maximum_descriptors)?;
            if step.checkpoint.role == RowRole::Body
                && step.checkpoint.phase == ParsePhase::Consuming
                && step.checkpoint.next_row == *body_rows
            {
                if !step.checkpoint.syntax.is_empty()
                    || !step.checkpoint.utf8_carry.is_empty()
                    || step.checkpoint.next_column != 0
                {
                    return Err(ConstructionError::Corrupt);
                }
                // The delimiter has no cells. The zero-body case ends at the header.
                if *body_rows != 0 && step.checkpoint.next_byte != length {
                    return Err(ConstructionError::Unavailable);
                }
                step.checkpoint.next_byte = length;
                step.checkpoint.phase = ParsePhase::Eof;
            }
            step
        }
        Pass::Columns if row.is_none() => {
            advance_columns(cp, source, *columns, length, maximum_descriptors)?
        }
        _ => return Err(ConstructionError::Unavailable),
    };
    if step.checkpoint == *cp && step.descriptors.is_empty() {
        return Err(ConstructionError::Unavailable);
    }
    let mut build = before.clone();
    build.revision = build
        .revision
        .checked_add(1)
        .ok_or(ConstructionError::Limit)?;
    build.transition = build
        .transition
        .checked_add(1)
        .ok_or(ConstructionError::Limit)?;
    build.checkpoint = step.checkpoint;
    let tag = match cp.pass {
        Pass::TableRows => 1,
        Pass::TableCells => 2,
        Pass::Columns => 3,
        _ => unreachable!(),
    };
    for (slot, descriptor) in step.descriptors.iter().enumerate() {
        let (actual_tag, role, ordinal, column) = descriptor.coordinate();
        if actual_tag != tag {
            return Err(ConstructionError::Corrupt);
        }
        let contiguous = match cp.pass {
            Pass::TableRows => {
                let expected = before.counts[1]
                    .checked_add(slot as u64)
                    .ok_or(ConstructionError::Limit)?;
                (role == RowRole::Header && expected == 0 && ordinal == 0)
                    || (role == RowRole::Body && expected != 0 && ordinal == expected - 1)
            }
            Pass::TableCells => {
                role == cp.role
                    && ordinal == cp.next_row
                    && column
                        == cp
                            .next_column
                            .checked_add(slot as u64)
                            .ok_or(ConstructionError::Limit)?
            }
            Pass::Columns => {
                ordinal
                    == before.counts[3]
                        .checked_add(slot as u64)
                        .ok_or(ConstructionError::Limit)?
            }
            _ => false,
        };
        if !contiguous {
            return Err(ConstructionError::Corrupt);
        }
        if let Some(raw) = descriptor.raw() {
            if !raw.is_valid() || raw.end > length {
                return Err(ConstructionError::Corrupt);
            }
        }
    }
    build.counts[tag as usize] = build.counts[tag as usize]
        .checked_add(step.descriptors.len() as u64)
        .ok_or(ConstructionError::Limit)?;
    let rows = body_rows.checked_add(1).ok_or(ConstructionError::Limit)?;
    if build.counts[0] != 0
        || build.counts[1] > rows
        || build.counts[3] > *columns
        || (cp.pass == Pass::TableRows
            && build.checkpoint.phase == ParsePhase::Eof
            && build.counts[1] != rows)
        || (cp.pass == Pass::Columns
            && build.checkpoint.phase == ParsePhase::Eof
            && build.counts[3] != *columns)
    {
        return Err(ConstructionError::Unavailable);
    }
    let mut locators = Vec::with_capacity(step.descriptors.len());
    let mut anchors = Vec::with_capacity(step.descriptors.len());
    let page = if step.descriptors.is_empty() {
        None
    } else {
        let mut page = Page {
            key: SequenceKey {
                build: build.key,
                ordinal: build.next_page,
            },
            binding_digest: build.binding_digest,
            prior_chain: before.chain,
            descriptors: std::mem::take(&mut step.descriptors),
            digest: [0; 32],
        };
        page.digest = page_digest(&page)?;
        for (slot, descriptor) in page.descriptors.iter().enumerate() {
            let (tag, role, row, column) = descriptor.coordinate();
            let key = match tag {
                1 => CoordinateKey::TableRow(build.key, role, row),
                2 => CoordinateKey::TableCell(build.key, role, row, column),
                3 => CoordinateKey::TableColumn(build.key, row),
                _ => return Err(ConstructionError::Corrupt),
            };
            locators.push(Locator {
                key,
                page: page.key.ordinal,
                slot: slot as u64,
                digest: page.digest,
            });
            if matches!(
                descriptor,
                Descriptor::TableCell { .. }
                    | Descriptor::TableRow {
                        disposition: RowDisposition::WholeRowFallback,
                        ..
                    }
            ) {
                let raw = descriptor.raw().ok_or(ConstructionError::Corrupt)?;
                anchors.push(AnchorLocator {
                    key: AnchorKey {
                        build: build.key,
                        start: before
                            .binding
                            .range
                            .start
                            .checked_add(raw.start)
                            .ok_or(ConstructionError::Limit)?,
                        tag,
                        role,
                        row,
                        column,
                    },
                    end: before
                        .binding
                        .range
                        .start
                        .checked_add(raw.end)
                        .ok_or(ConstructionError::Limit)?,
                    page: page.key.ordinal,
                    slot: slot as u64,
                    digest: page.digest,
                });
            }
        }
        build.next_page = build
            .next_page
            .checked_add(1)
            .ok_or(ConstructionError::Limit)?;
        build.chain = chain_link(&before.chain, &page.digest);
        Some(page)
    };
    let key = SequenceKey {
        build: build.key,
        ordinal: build.transition,
    };
    build.progress = Some(Reference {
        key,
        digest: [0; 32],
    });
    let mut receipt = Progress {
        key,
        command: Command::Advance,
        prior: before.progress,
        before_revision: before.revision,
        after_revision: build.revision,
        before_digest: build_digest(before)?,
        after_digest: build_digest(&build)?,
        page: page.as_ref().map(|page| Reference {
            key: page.key,
            digest: page.digest,
        }),
        root: None,
        digest: [0; 32],
    };
    receipt.digest = progress_digest(&receipt)?;
    build.progress = Some(Reference {
        key,
        digest: receipt.digest,
    });
    encode_build(&build)?;
    encode_progress(&receipt)?;
    Ok(PreparedTableAdvance {
        build,
        receipt,
        page,
        locators,
        anchors,
    })
}
