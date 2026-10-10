use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use super::{
    codec::*,
    construction::ConstructionError,
    query::{IndexedDescriptor, IndexedQuery, QueryRecords},
    *,
};
use crate::domain::SyndicDomain;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAnchor {
    pub source: ProjectionTextSource,
    pub block_start: u64,
    pub kind: crate::ResourceKind,
    pub position: u64,
    pub visual_offset: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAnchor {
    pub descriptor: IndexedDescriptor,
    pub anchor: SourceAnchor,
}

pub(super) trait AnchorRecords: QueryRecords {
    fn seek_anchor(
        &self,
        first: AnchorKey,
        last: AnchorKey,
        reverse: bool,
    ) -> Result<Option<(AnchorKey, AnchorLocator)>, ConstructionError>;
}

impl AnchorRecords for DomainReader<'_, SyndicDomain> {
    fn seek_anchor(
        &self,
        first: AnchorKey,
        last: AnchorKey,
        reverse: bool,
    ) -> Result<Option<(AnchorKey, AnchorLocator)>, ConstructionError> {
        let direction = if reverse {
            CursorDirection::Reverse
        } else {
            CursorDirection::Forward
        };
        match self.cursor::<SourceAnchorsCodec>(
            &CursorRange::closed(first, last),
            direction,
            CursorReadLimits::new(1, 512).map_err(|_| ConstructionError::Limit)?,
        ) {
            Err(beryl_home_store::ReadError::BoundExceeded { .. }) => Err(ConstructionError::Limit),
            result => Ok(result?
                .into_records()
                .into_iter()
                .next()
                .map(|record| record.into_parts())),
        }
    }
}

impl IndexedQuery {
    pub(super) fn resolve_anchor(
        &mut self,
        records: &impl AnchorRecords,
        old: SourceAnchor,
    ) -> Result<Option<ResolvedAnchor>, ConstructionError> {
        if !old.source.can_extend(self.binding.source)
            || old.block_start != self.binding.range.start
            || old.kind != self.binding.metadata.kind()
        {
            return Ok(None);
        }
        if old.position < old.block_start || old.position > old.source.logical_utf8_bytes() {
            return Err(ConstructionError::Unavailable);
        }
        self.ledger
            .reserve_read(0, 0, std::mem::size_of::<ResolvedAnchor>())?;
        let position = old
            .position
            .clamp(self.binding.range.start, self.binding.range.end);
        let low = |start| AnchorKey {
            build: self.root.key,
            start,
            tag: 0,
            role: RowRole::Header,
            row: 0,
            column: 0,
        };
        let high = |start| AnchorKey {
            build: self.root.key,
            start,
            tag: 2,
            role: RowRole::Body,
            row: u64::MAX,
            column: u64::MAX,
        };
        let mut best: Option<(u64, IndexedDescriptor, u64)> = None;
        for (first, last, reverse) in [
            (low(self.binding.range.start), high(position), true),
            (low(position), high(self.binding.range.end), false),
        ] {
            self.ledger.reserve_read(1, 512, 512)?;
            let Some((key, locator)) = records.seek_anchor(first, last, reverse)? else {
                continue;
            };
            let encoded = anchor_key(key)?;
            if encoded < anchor_key(first)? || encoded > anchor_key(last)? || locator.key != key {
                return Err(ConstructionError::Corrupt);
            }
            let descriptor = self.anchor_descriptor(records, locator)?;
            let raw = descriptor.absolute_raw.ok_or(ConstructionError::Corrupt)?;
            let resolved = old.position.clamp(raw.start, raw.end);
            let distance = old.position.abs_diff(resolved);
            if best.as_ref().is_none_or(|(previous, item, _)| {
                distance < *previous
                    || (distance == *previous && raw.start < item.absolute_raw.unwrap().start)
            }) {
                best = Some((distance, descriptor, resolved));
            }
        }
        if best.is_none()
            && (self.root.code_lines != 0
                || self.binding.metadata.kind() == crate::ResourceKind::Table)
        {
            return Err(ConstructionError::Corrupt);
        }
        Ok(best.map(|(_, descriptor, position)| ResolvedAnchor {
            descriptor,
            anchor: SourceAnchor {
                source: self.binding.source,
                position,
                ..old
            },
        }))
    }

    fn anchor_descriptor(
        &mut self,
        records: &impl QueryRecords,
        locator: AnchorLocator,
    ) -> Result<IndexedDescriptor, ConstructionError> {
        let key = locator.key;
        let coordinate = (key.tag, key.role, key.row, key.column);
        let index = match key.tag {
            0 if self.binding.metadata.kind() == crate::ResourceKind::Code
                && key.row < self.root.code_lines =>
            {
                CoordinateKey::CodeLine(self.root.key, key.row)
            }
            1 | 2
                if self.binding.metadata.kind() == crate::ResourceKind::Table
                    && match key.role {
                        RowRole::Header => key.row == 0,
                        RowRole::Body => key.row < self.root.body_rows,
                    } =>
            {
                if key.tag == 1 {
                    CoordinateKey::TableRow(self.root.key, key.role, key.row)
                } else {
                    if key.column >= self.root.columns {
                        return Err(ConstructionError::Corrupt);
                    }
                    CoordinateKey::TableCell(self.root.key, key.role, key.row, key.column)
                }
            }
            _ => return Err(ConstructionError::Corrupt),
        };
        let descriptor = self.located_descriptor(
            records,
            Locator {
                key: index,
                page: locator.page,
                slot: locator.slot,
                digest: locator.digest,
            },
            coordinate,
        )?;
        if key.tag == 1
            && !matches!(
                descriptor.descriptor,
                Descriptor::TableRow {
                    disposition: RowDisposition::WholeRowFallback,
                    ..
                }
            )
            || descriptor.absolute_raw
                != Some(ByteRange {
                    start: key.start,
                    end: locator.end,
                })
        {
            return Err(ConstructionError::Corrupt);
        }
        if key.tag == 2 {
            let row = self.row(records, key.role, key.row)?;
            let Descriptor::TableRow {
                raw,
                authored_cells,
                disposition,
                ..
            } = row.descriptor
            else {
                return Err(ConstructionError::Corrupt);
            };
            let cell = descriptor
                .descriptor
                .raw()
                .ok_or(ConstructionError::Corrupt)?;
            if disposition == RowDisposition::WholeRowFallback
                || key.column >= authored_cells
                || cell.start < raw.start
                || cell.end > raw.end
            {
                return Err(ConstructionError::Corrupt);
            }
        }
        Ok(descriptor)
    }
}
