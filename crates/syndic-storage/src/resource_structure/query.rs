use beryl_home_store::{DomainReader, PointReadLimit, RECORD_VERSION_BYTES};

use super::{
    codec::*,
    construction::{
        AuthenticationRecords, ConstructionError, authenticate_source, validate_selected,
    },
    ledger::Ledger,
    *,
};
use crate::{
    codec::{ExactCodec, Family, ResourcesFamily},
    domain::SyndicDomain,
};

pub(super) trait QueryRecords {
    fn point<F: Family>(
        &self,
        key: &F::Key,
        payload_limit: usize,
    ) -> Result<Option<F::Value>, ConstructionError>;
}

impl QueryRecords for DomainReader<'_, SyndicDomain> {
    fn point<F: Family>(
        &self,
        key: &F::Key,
        payload_limit: usize,
    ) -> Result<Option<F::Value>, ConstructionError> {
        let limit = payload_limit
            .checked_add(RECORD_VERSION_BYTES)
            .and_then(|bytes| PointReadLimit::new(bytes).ok())
            .ok_or(ConstructionError::Limit)?;
        match self.point::<ExactCodec<F>>(key, limit) {
            Err(beryl_home_store::ReadError::BoundExceeded { .. }) => Err(ConstructionError::Limit),
            result => Ok(result?),
        }
    }
}

fn point<F: Family>(
    records: &impl QueryRecords,
    ledger: &mut Ledger,
    key: &F::Key,
    payload: usize,
) -> Result<Option<F::Value>, ConstructionError> {
    ledger.reserve_point::<F>(payload)?;
    records.point::<F>(key, payload)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedDescriptor {
    pub descriptor: Descriptor,
    pub absolute_raw: Option<ByteRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CellLookup {
    Authored(IndexedDescriptor),
    Missing {
        role: RowRole,
        row: u64,
        column: u64,
    },
    RowFallback(IndexedDescriptor),
}

pub(super) struct IndexedQuery {
    pub(super) binding: Binding,
    pub(super) root: Root,
    pub(super) ledger: Ledger,
    page: Option<Page>,
    row: Option<IndexedDescriptor>,
}

impl IndexedQuery {
    pub(super) fn open(
        reader: &impl AuthenticationRecords,
        binding: &Binding,
        expected_root: Option<[u8; 32]>,
    ) -> Result<Option<Self>, ConstructionError> {
        let mut ledger = Ledger::default();
        ledger.reserve_read(0, 0, binding_bytes(binding)?.len())?;
        authenticate_source(reader, &mut ledger, binding)?;
        Self::authenticate(reader, binding, expected_root, ledger)
    }

    pub(super) fn authenticate(
        records: &impl QueryRecords,
        binding: &Binding,
        expected_root: Option<[u8; 32]>,
        mut ledger: Ledger,
    ) -> Result<Option<Self>, ConstructionError> {
        let metadata = point::<ResourcesFamily>(records, &mut ledger, &binding.resource.id, 4_096)?
            .ok_or(ConstructionError::Unavailable)?;
        if metadata != binding.metadata {
            return Err(ConstructionError::Collision);
        }
        let Some(head) = point::<HeadsFamily>(records, &mut ledger, &binding.resource, 168)? else {
            return if expected_root.is_some() {
                Err(ConstructionError::Unavailable)
            } else {
                Ok(None)
            };
        };
        if expected_root.is_some_and(|digest| digest != head.root) {
            return Err(ConstructionError::Collision);
        }
        if head.key != binding.resource || head.binding_digest != binding_digest(binding)? {
            return Err(ConstructionError::Collision);
        }
        let root = point::<RootsFamily>(records, &mut ledger, &head.build, 4_608)?
            .ok_or(ConstructionError::Corrupt)?;
        let build = point::<BuildsFamily>(records, &mut ledger, &head.build, 24_576)?
            .ok_or(ConstructionError::Corrupt)?;
        let progress = build.progress.ok_or(ConstructionError::Corrupt)?;
        let receipt = point::<ProgressFamily>(records, &mut ledger, &progress.key, 512)?
            .ok_or(ConstructionError::Corrupt)?;
        validate_selected(binding, &head, &root, &build, &receipt)?;
        ledger.reserve_read(0, 0, binding_bytes(binding)?.len())?;
        Ok(Some(Self {
            binding: binding.clone(),
            root,
            ledger,
            page: None,
            row: None,
        }))
    }

    fn descriptor<F: Family<Key = CoordinateKey, Value = Locator>>(
        &mut self,
        records: &impl QueryRecords,
        key: CoordinateKey,
        coordinate: (u8, RowRole, u64, u64),
    ) -> Result<IndexedDescriptor, ConstructionError> {
        let locator = point::<F>(records, &mut self.ledger, &key, F::MAX_VALUE_BYTES)?
            .ok_or(ConstructionError::Corrupt)?;
        if locator.key != key || locator.page == 0 || locator.page > self.root.pages {
            return Err(ConstructionError::Corrupt);
        }
        self.located_descriptor(records, locator, coordinate)
    }

    pub(super) fn located_descriptor(
        &mut self,
        records: &impl QueryRecords,
        locator: Locator,
        coordinate: (u8, RowRole, u64, u64),
    ) -> Result<IndexedDescriptor, ConstructionError> {
        if locator.page == 0 || locator.page > self.root.pages {
            return Err(ConstructionError::Corrupt);
        }
        let page_key = SequenceKey {
            build: self.root.key,
            ordinal: locator.page,
        };
        if self.page.as_ref().is_none_or(|page| page.key != page_key) {
            self.page = Some(
                point::<PagesFamily>(records, &mut self.ledger, &page_key, 65_536)?
                    .ok_or(ConstructionError::Corrupt)?,
            );
        }
        let page = self.page.as_ref().ok_or(ConstructionError::Corrupt)?;
        if page.key != page_key
            || page.binding_digest != self.root.binding_digest
            || page.digest != locator.digest
        {
            return Err(ConstructionError::Corrupt);
        }
        let slot = usize::try_from(locator.slot).map_err(|_| ConstructionError::Corrupt)?;
        let descriptor = page
            .descriptors
            .get(slot)
            .ok_or(ConstructionError::Corrupt)?;
        if descriptor.coordinate() != coordinate {
            return Err(ConstructionError::Corrupt);
        }
        let absolute_raw = descriptor
            .raw()
            .map(|raw| {
                let length = self
                    .binding
                    .range
                    .end
                    .checked_sub(self.binding.range.start)
                    .ok_or(ConstructionError::Corrupt)?;
                if !raw.is_valid() || raw.end > length {
                    return Err(ConstructionError::Corrupt);
                }
                Ok(ByteRange {
                    start: self
                        .binding
                        .range
                        .start
                        .checked_add(raw.start)
                        .ok_or(ConstructionError::Corrupt)?,
                    end: self
                        .binding
                        .range
                        .start
                        .checked_add(raw.end)
                        .ok_or(ConstructionError::Corrupt)?,
                })
            })
            .transpose()?;
        self.ledger
            .reserve_read(0, 0, std::mem::size_of::<IndexedDescriptor>())?;
        Ok(IndexedDescriptor {
            descriptor: descriptor.clone(),
            absolute_raw,
        })
    }

    pub(super) fn line(
        &mut self,
        records: &impl QueryRecords,
        ordinal: u64,
    ) -> Result<IndexedDescriptor, ConstructionError> {
        if !matches!(
            self.binding.metadata.structure(),
            crate::ResourceStructure::Code { .. }
        ) || ordinal >= self.root.code_lines
        {
            return Err(ConstructionError::Unavailable);
        }
        self.descriptor::<CodeLinesFamily>(
            records,
            CoordinateKey::CodeLine(self.root.key, ordinal),
            (0, RowRole::Header, ordinal, 0),
        )
    }

    pub(super) fn row(
        &mut self,
        records: &impl QueryRecords,
        role: RowRole,
        ordinal: u64,
    ) -> Result<IndexedDescriptor, ConstructionError> {
        if !matches!(
            self.binding.metadata.structure(),
            crate::ResourceStructure::Table { .. }
        ) || match role {
            RowRole::Header => ordinal != 0,
            RowRole::Body => ordinal >= self.root.body_rows,
        } {
            return Err(ConstructionError::Unavailable);
        }
        if let Some(row) = &self.row
            && row.descriptor.coordinate() == (1, role, ordinal, 0)
        {
            self.ledger
                .reserve_read(0, 0, std::mem::size_of::<IndexedDescriptor>())?;
            return Ok(row.clone());
        }
        let row = self.descriptor::<TableRowsFamily>(
            records,
            CoordinateKey::TableRow(self.root.key, role, ordinal),
            (1, role, ordinal, 0),
        )?;
        self.ledger
            .reserve_read(0, 0, std::mem::size_of::<IndexedDescriptor>())?;
        self.row = Some(row.clone());
        Ok(row)
    }

    pub(super) fn cell(
        &mut self,
        records: &impl QueryRecords,
        role: RowRole,
        row: u64,
        column: u64,
    ) -> Result<CellLookup, ConstructionError> {
        if column >= self.root.columns {
            return Err(ConstructionError::Unavailable);
        }
        let closed_row = self.row(records, role, row)?;
        let Descriptor::TableRow {
            raw,
            authored_cells,
            disposition,
            ..
        } = closed_row.descriptor
        else {
            return Err(ConstructionError::Corrupt);
        };
        if disposition == RowDisposition::WholeRowFallback {
            return Ok(CellLookup::RowFallback(closed_row));
        }
        if authored_cells > self.root.columns {
            return Err(ConstructionError::Corrupt);
        }
        if column >= authored_cells {
            return Ok(CellLookup::Missing { role, row, column });
        }
        let cell = self.descriptor::<TableCellsFamily>(
            records,
            CoordinateKey::TableCell(self.root.key, role, row, column),
            (2, role, row, column),
        )?;
        let cell_raw = cell.descriptor.raw().ok_or(ConstructionError::Corrupt)?;
        if cell_raw.start < raw.start || cell_raw.end > raw.end {
            return Err(ConstructionError::Corrupt);
        }
        Ok(CellLookup::Authored(cell))
    }

    pub(super) fn column(
        &mut self,
        records: &impl QueryRecords,
        ordinal: u64,
    ) -> Result<IndexedDescriptor, ConstructionError> {
        if ordinal >= self.root.columns {
            return Err(ConstructionError::Unavailable);
        }
        self.descriptor::<TableColumnsFamily>(
            records,
            CoordinateKey::TableColumn(self.root.key, ordinal),
            (3, RowRole::Header, ordinal, 0),
        )
    }
}
