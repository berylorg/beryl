use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{codec::*, construction::*, ledger::Ledger, source, transition::validate_receipt, *};
use crate::{
    ResourceStructure,
    codec::{Family, ResourcesFamily},
    domain::SyndicDomain,
};

pub(crate) struct AdvanceCode {
    pub(crate) before: Build,
    pub(crate) source_bytes: usize,
    pub(crate) descriptors: usize,
}

pub(crate) struct PreparedAdvance {
    pub(crate) build: Build,
    pub(crate) receipt: Progress,
    pub(crate) page: Option<Page>,
    pub(crate) lines: Vec<Locator>,
    pub(crate) anchors: Vec<AnchorLocator>,
}

pub(super) fn prepare_code(
    before: &Build,
    source: &[u8],
    descriptors: usize,
) -> Result<PreparedAdvance, ConstructionError> {
    encode_build(before)?;
    if before.lifecycle != Lifecycle::Building {
        return Err(ConstructionError::Collision);
    }
    let ResourceStructure::Code { logical_lines, .. } = before.binding.metadata.structure() else {
        return Err(ConstructionError::Unavailable);
    };
    if before.checkpoint.next_row != before.counts[0] || before.counts[1..] != [0, 0, 0] {
        return Err(ConstructionError::Corrupt);
    }
    let step = advance_code(
        &before.checkpoint,
        source,
        before.binding.range.end - before.binding.range.start,
        descriptors,
    )?;
    if step.checkpoint == before.checkpoint && step.descriptors.is_empty() {
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
    build.counts[0] = before.counts[0]
        .checked_add(step.descriptors.len() as u64)
        .ok_or(ConstructionError::Limit)?;
    if build.counts[0] > *logical_lines
        || (build.checkpoint.phase == ParsePhase::Eof && build.counts[0] != *logical_lines)
    {
        return Err(ConstructionError::Unavailable);
    }
    let mut lines = Vec::with_capacity(step.descriptors.len());
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
            descriptors: step.descriptors,
            digest: [0; 32],
        };
        page.digest = page_digest(&page)?;
        for (slot, descriptor) in page.descriptors.iter().enumerate() {
            let Descriptor::CodeLine { ordinal, raw, .. } = *descriptor else {
                return Err(ConstructionError::Corrupt);
            };
            if ordinal
                != before.counts[0]
                    .checked_add(slot as u64)
                    .ok_or(ConstructionError::Limit)?
            {
                return Err(ConstructionError::Corrupt);
            }
            lines.push(Locator {
                key: CoordinateKey::CodeLine(build.key, ordinal),
                page: page.key.ordinal,
                slot: slot as u64,
                digest: page.digest,
            });
            anchors.push(AnchorLocator {
                key: AnchorKey {
                    build: build.key,
                    start: before
                        .binding
                        .range
                        .start
                        .checked_add(raw.start)
                        .ok_or(ConstructionError::Limit)?,
                    tag: 0,
                    role: RowRole::Header,
                    row: ordinal,
                    column: 0,
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
    Ok(PreparedAdvance {
        build,
        receipt,
        page,
        lines,
        anchors,
    })
}

impl DomainMutation<SyndicDomain> for AdvanceCode {
    type Error = ConstructionError;
    type Prepared = PreparedAdvance;

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
        if stored.checkpoint.pass != Pass::CodeLines
            || stored.checkpoint.phase != ParsePhase::Consuming
        {
            return Err(ConstructionError::Unavailable);
        }
        let remaining = (stored.binding.range.end - stored.binding.range.start)
            .checked_sub(stored.checkpoint.next_byte)
            .ok_or(ConstructionError::Corrupt)?;
        let length = self
            .source_bytes
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let bytes = source::read_authenticated_window(
            reader,
            &mut ledger,
            &stored.binding,
            encoded_frontier,
            stored.checkpoint.next_byte,
            length,
        )?;
        // Derivation temporarily joins the previous scalar suffix to the offered window.
        ledger.reserve_read(
            0,
            0,
            bytes.len() + 3 + 24_576 + 65_536 + self.descriptors * (96 + 122),
        )?;
        let prepared = prepare_code(&stored, &bytes, self.descriptors)?;
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
        for line in &prepared.lines {
            if ledger
                .point::<CodeLinesFamily>(reader, &line.key, 96)?
                .is_some()
            {
                return Err(ConstructionError::Collision);
            }
            ledger.reserve_write(48, CodeLinesFamily::encode_value(line)?.len(), false)?;
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
        reservation.reserve_records::<CodeLinesCodec>(self.descriptors)?;
        reservation.reserve_records::<SourceAnchorsCodec>(self.descriptors)?;
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
        for line in prepared.lines {
            mutations.put::<CodeLinesCodec>(&line.key, &line)?;
        }
        for anchor in prepared.anchors {
            mutations.put::<SourceAnchorsCodec>(&anchor.key, &anchor)?;
        }
        Ok(())
    }
}
