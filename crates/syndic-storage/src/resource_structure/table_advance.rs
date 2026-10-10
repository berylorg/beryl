use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{
    codec::*, construction::*, ledger::Ledger, source, table_publication::*,
    transition::validate_receipt, *,
};
use crate::{
    ResourceStructure,
    codec::{Family, ResourcesFamily},
    domain::SyndicDomain,
};

pub(crate) struct AdvanceTable {
    pub(crate) before: Build,
    pub(crate) source_bytes: usize,
    pub(crate) descriptors: usize,
}

fn closed_row(
    reader: &DomainReader<'_, SyndicDomain>,
    ledger: &mut Ledger,
    build: &Build,
) -> Result<Descriptor, ConstructionError> {
    let key = CoordinateKey::TableRow(build.key, build.checkpoint.role, build.checkpoint.next_row);
    let locator = ledger
        .point::<TableRowsFamily>(reader, &key, 97)?
        .ok_or(ConstructionError::Corrupt)?;
    if locator.key != key || locator.page == 0 || locator.page >= build.next_page {
        return Err(ConstructionError::Corrupt);
    }
    let page_key = SequenceKey {
        build: build.key,
        ordinal: locator.page,
    };
    let page = ledger
        .point::<PagesFamily>(reader, &page_key, 65_536)?
        .ok_or(ConstructionError::Corrupt)?;
    if page.key != page_key
        || page.binding_digest != build.binding_digest
        || page.digest != locator.digest
    {
        return Err(ConstructionError::Corrupt);
    }
    let descriptor = page
        .descriptors
        .get(usize::try_from(locator.slot).map_err(|_| ConstructionError::Corrupt)?)
        .ok_or(ConstructionError::Corrupt)?;
    if descriptor.coordinate() != (1, build.checkpoint.role, build.checkpoint.next_row, 0) {
        return Err(ConstructionError::Corrupt);
    }
    let raw = descriptor.raw().ok_or(ConstructionError::Corrupt)?;
    if !raw.is_valid() || raw.end > build.binding.range.end - build.binding.range.start {
        return Err(ConstructionError::Corrupt);
    }
    Ok(descriptor.clone())
}

fn reserve_locator<F: Family<Key = CoordinateKey, Value = Locator>>(
    reader: &DomainReader<'_, SyndicDomain>,
    ledger: &mut Ledger,
    locator: &Locator,
) -> Result<(), ConstructionError> {
    if ledger
        .point::<F>(reader, &locator.key, F::MAX_VALUE_BYTES)?
        .is_some()
    {
        return Err(ConstructionError::Collision);
    }
    ledger.reserve_write(F::MAX_KEY_BYTES, F::encode_value(locator)?.len(), false)
}

impl DomainMutation<SyndicDomain> for AdvanceTable {
    type Error = ConstructionError;
    type Prepared = PreparedTableAdvance;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if self.source_bytes > 65_536 || !(1..=256).contains(&self.descriptors) {
            return Err(ConstructionError::Limit);
        }
        let mut ledger = Ledger::default();
        ledger.reserve_read(0, 0, 24_576)?;
        let stored = ledger
            .point::<BuildsFamily>(reader, &self.before.key, 24_576)?
            .ok_or(ConstructionError::Collision)?;
        if stored != self.before || stored.lifecycle != Lifecycle::Building {
            return Err(ConstructionError::Collision);
        }
        if let Some(selected) = stored.progress {
            let receipt = ledger
                .point::<ProgressFamily>(reader, &selected.key, 512)?
                .ok_or(ConstructionError::Corrupt)?;
            validate_receipt(&stored, &receipt)?;
        }
        let metadata = ledger
            .point::<ResourcesFamily>(reader, &stored.key.resource.id, 4_096)?
            .ok_or(ConstructionError::Unavailable)?;
        if metadata != stored.binding.metadata {
            return Err(ConstructionError::Unavailable);
        }
        let encoded_frontier = authenticate_source(reader, &mut ledger, &stored.binding)?;
        if let Some(head) = ledger.point::<HeadsFamily>(reader, &stored.key.resource, 168)? {
            authenticate_selected(reader, &mut ledger, &stored.binding, &head)?;
            return Err(ConstructionError::AlreadyCurrent);
        }
        let ResourceStructure::Table { .. } = stored.binding.metadata.structure() else {
            return Err(ConstructionError::Unavailable);
        };
        if !matches!(
            stored.checkpoint.pass,
            Pass::TableRows | Pass::TableCells | Pass::Columns
        ) || stored.checkpoint.phase == ParsePhase::Eof
        {
            return Err(ConstructionError::Unavailable);
        }
        let row = if stored.checkpoint.pass == Pass::TableCells {
            Some(closed_row(reader, &mut ledger, &stored)?)
        } else {
            None
        };
        let mut start = stored.checkpoint.next_byte;
        let mut end = stored.binding.range.end - stored.binding.range.start;
        let skip = stored.checkpoint.phase == ParsePhase::DrainingSyntax
            || matches!(
                row,
                Some(Descriptor::TableRow {
                    disposition: RowDisposition::WholeRowFallback,
                    ..
                })
            );
        if let Some(row) = &row {
            let raw = row.raw().ok_or(ConstructionError::Corrupt)?;
            // The already authenticated row index seeks over the delimiter once.
            if stored.checkpoint.role == RowRole::Body
                && stored.checkpoint.next_row == 0
                && stored.checkpoint.syntax.is_empty()
                && stored.checkpoint.utf8_carry.is_empty()
                && stored.checkpoint.phase == ParsePhase::Consuming
            {
                start = raw.start;
            }
            end = raw.end;
        }
        let length = if skip {
            0
        } else {
            self.source_bytes.min(
                usize::try_from(end.checked_sub(start).ok_or(ConstructionError::Corrupt)?)
                    .unwrap_or(usize::MAX),
            )
        };
        let bytes = if skip {
            Vec::new()
        } else {
            source::read_authenticated_window(
                reader,
                &mut ledger,
                &stored.binding,
                encoded_frontier,
                start,
                length,
            )?
        };
        // Account for cloned syntax, scalar joins, page encoding and both index streams.
        ledger.reserve_read(
            0,
            0,
            bytes.len() + 16_384 + 3 + 24_576 + 65_536 + self.descriptors * (105 + 122),
        )?;
        let prepared = prepare_table(&stored, row.as_ref(), &bytes, self.descriptors)?;
        if ledger
            .point::<ProgressFamily>(reader, &prepared.receipt.key, 512)?
            .is_some()
        {
            return Err(ConstructionError::Collision);
        }
        ledger.reserve_write(40, encode_build(&prepared.build)?.len(), false)?;
        ledger.reserve_write(48, encode_progress(&prepared.receipt)?.len(), false)?;
        if let Some(page) = &prepared.page {
            if ledger
                .point::<PagesFamily>(reader, &page.key, 65_536)?
                .is_some()
            {
                return Err(ConstructionError::Collision);
            }
            ledger.reserve_write(48, encode_page(page)?.len(), false)?;
        }
        for locator in &prepared.locators {
            match locator.key {
                CoordinateKey::TableRow(..) => {
                    reserve_locator::<TableRowsFamily>(reader, &mut ledger, locator)?
                }
                CoordinateKey::TableCell(..) => {
                    reserve_locator::<TableCellsFamily>(reader, &mut ledger, locator)?
                }
                CoordinateKey::TableColumn(..) => {
                    reserve_locator::<TableColumnsFamily>(reader, &mut ledger, locator)?
                }
                _ => return Err(ConstructionError::Corrupt),
            }
        }
        for anchor in &prepared.anchors {
            if ledger
                .point::<SourceAnchorsFamily>(reader, &anchor.key, 122)?
                .is_some()
            {
                return Err(ConstructionError::Collision);
            }
            ledger.reserve_write(66, encode_anchor(anchor)?.len(), false)?;
        }
        Ok(prepared)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if self.source_bytes > 65_536 || !(1..=256).contains(&self.descriptors) {
            return Err(ConstructionError::Limit);
        }
        reservation.reserve_records::<BuildsCodec>(1)?;
        reservation.reserve_records::<ProgressCodec>(1)?;
        reservation.reserve_records::<PagesCodec>(1)?;
        match self.before.checkpoint.pass {
            Pass::TableRows => reservation.reserve_records::<TableRowsCodec>(self.descriptors)?,
            Pass::TableCells => reservation.reserve_records::<TableCellsCodec>(self.descriptors)?,
            Pass::Columns => reservation.reserve_records::<TableColumnsCodec>(self.descriptors)?,
            _ => return Err(ConstructionError::Unavailable),
        }
        if self.before.checkpoint.pass != Pass::Columns {
            reservation.reserve_records::<SourceAnchorsCodec>(self.descriptors)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BuildsCodec>(&prepared.build.key, &prepared.build)?;
        mutations.put::<ProgressCodec>(&prepared.receipt.key, &prepared.receipt)?;
        if let Some(page) = prepared.page {
            mutations.put::<PagesCodec>(&page.key, &page)?;
        }
        for locator in prepared.locators {
            match locator.key {
                CoordinateKey::TableRow(..) => {
                    mutations.put::<TableRowsCodec>(&locator.key, &locator)?
                }
                CoordinateKey::TableCell(..) => {
                    mutations.put::<TableCellsCodec>(&locator.key, &locator)?
                }
                CoordinateKey::TableColumn(..) => {
                    mutations.put::<TableColumnsCodec>(&locator.key, &locator)?
                }
                _ => return Err(ConstructionError::Corrupt),
            }
        }
        for anchor in prepared.anchors {
            mutations.put::<SourceAnchorsCodec>(&anchor.key, &anchor)?;
        }
        Ok(())
    }
}
