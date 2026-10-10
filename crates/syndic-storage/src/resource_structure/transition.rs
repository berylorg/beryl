use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use super::{codec::*, construction::*, ledger::Ledger, *};
use crate::{ResourceStructure, codec::ResourcesFamily, domain::SyndicDomain};

pub(crate) struct ControlTransition {
    pub(crate) before: Build,
    pub(crate) command: Command,
}

pub(crate) struct PreparedTransition {
    pub(crate) build: Build,
    pub(crate) receipt: Progress,
    pub(crate) selection: Option<(Root, Head)>,
}

pub(super) fn validate_receipt(build: &Build, receipt: &Progress) -> Result<(), ConstructionError> {
    let selected = build.progress.ok_or(ConstructionError::Corrupt)?;
    if receipt.key != selected.key
        || receipt.digest != selected.digest
        || receipt.after_revision != build.revision
        || receipt.after_digest != build_digest(build)?
        || receipt.command == Command::Complete
        || matches!(
            receipt.command,
            Command::Cancel | Command::Supersede | Command::Refuse
        )
    {
        return Err(ConstructionError::Corrupt);
    }
    encode_progress(receipt)?;
    Ok(())
}

fn reset(pass: Pass, next_byte: u64) -> Checkpoint {
    Checkpoint {
        pass,
        next_byte,
        line_start: 0,
        role: RowRole::Header,
        next_row: 0,
        next_column: 0,
        phase: if pass == Pass::Sealed {
            ParsePhase::Eof
        } else {
            ParsePhase::Consuming
        },
        pending_cr: false,
        utf8_carry: Vec::new(),
        syntax_start: 0,
        syntax: Vec::new(),
        output_position: 0,
        escaped: false,
        in_code: false,
    }
}

fn change_pass(build: &Build) -> Result<Checkpoint, ConstructionError> {
    let cp = &build.checkpoint;
    let length = build.binding.range.end - build.binding.range.start;
    if cp.phase != ParsePhase::Eof
        || !cp.utf8_carry.is_empty()
        || cp.pending_cr
        || !cp.syntax.is_empty()
        || cp.syntax_start != 0
        || cp.output_position != 0
        || cp.escaped
        || cp.in_code
        || cp.line_start != 0
        || cp.next_column != 0
    {
        return Err(ConstructionError::Unavailable);
    }
    match (cp.pass, build.binding.metadata.structure()) {
        (Pass::CodeLines, ResourceStructure::Code { logical_lines, .. })
            if cp.next_byte == length && build.counts == [*logical_lines, 0, 0, 0] =>
        {
            Ok(reset(Pass::Sealed, length))
        }
        (Pass::TableRows, ResourceStructure::Table { body_rows, .. })
            if cp.next_byte == length
                && cp.role == RowRole::Body
                && cp.next_row == *body_rows
                && body_rows.checked_add(1) == Some(build.counts[1])
                && build.counts[2..] == [0, 0] =>
        {
            Ok(reset(Pass::TableCells, 0))
        }
        (
            Pass::TableCells,
            ResourceStructure::Table {
                body_rows,
                header_range,
                ..
            },
        ) if cp.next_byte == length
            && cp.role == RowRole::Body
            && cp.next_row == *body_rows
            && body_rows.checked_add(1) == Some(build.counts[1])
            && build.counts[3] == 0 =>
        {
            Ok(reset(Pass::Columns, header_range.end()))
        }
        (Pass::Columns, ResourceStructure::Table { columns, .. })
            if build.counts[3] == *columns =>
        {
            Ok(reset(Pass::Sealed, length))
        }
        _ => Err(ConstructionError::Unavailable),
    }
}

fn completed_root(build: &Build, progress: SequenceKey) -> Result<Root, ConstructionError> {
    if build.checkpoint.pass != Pass::Sealed {
        return Err(ConstructionError::Unavailable);
    }
    let (code_lines, body_rows, columns, header, language) =
        match build.binding.metadata.structure() {
            ResourceStructure::Code {
                logical_lines,
                language,
            } if build.counts == [*logical_lines, 0, 0, 0] => {
                (*logical_lines, 0, 0, None, language.clone())
            }
            ResourceStructure::Table {
                body_rows,
                columns,
                header_range,
            } if build.counts[0] == 0
                && body_rows.checked_add(1) == Some(build.counts[1])
                && build.counts[3] == *columns =>
            {
                (
                    0,
                    *body_rows,
                    *columns,
                    Some(ByteRange {
                        start: header_range.start(),
                        end: header_range.end(),
                    }),
                    None,
                )
            }
            _ => return Err(ConstructionError::Unavailable),
        };
    let mut root = Root {
        key: build.key,
        binding_digest: build.binding_digest,
        pages: build
            .next_page
            .checked_sub(1)
            .ok_or(ConstructionError::Corrupt)?,
        chain: build.chain,
        counts: build.counts,
        code_lines,
        body_rows,
        columns,
        header,
        language,
        progress,
        digest: [0; 32],
    };
    root.digest = root_digest(&root)?;
    Ok(root)
}

pub(super) fn prepare_control(
    before: &Build,
    command: Command,
) -> Result<PreparedTransition, ConstructionError> {
    encode_build(before)?;
    if before.lifecycle != Lifecycle::Building {
        return Err(ConstructionError::Collision);
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
    let key = SequenceKey {
        build: build.key,
        ordinal: build.transition,
    };
    build.progress = Some(Reference {
        key,
        digest: [0; 32],
    });
    let selection = match command {
        Command::PassChange => {
            build.checkpoint = change_pass(before)?;
            None
        }
        Command::Cancel | Command::Supersede | Command::Refuse => {
            build.lifecycle = match command {
                Command::Cancel => Lifecycle::Cancelled,
                Command::Supersede => Lifecycle::Superseded,
                _ => Lifecycle::Refused,
            };
            None
        }
        Command::Complete => {
            let root = completed_root(&build, key)?;
            build.lifecycle = Lifecycle::Completed;
            let mut head = Head {
                key: build.key.resource,
                binding_digest: build.binding_digest,
                build: build.key,
                root: root.digest,
                digest: [0; 32],
            };
            head.digest = head_digest(&head)?;
            Some((root, head))
        }
        Command::Advance => return Err(ConstructionError::Unavailable),
    };
    let mut receipt = Progress {
        key,
        command,
        prior: before.progress,
        before_revision: before.revision,
        after_revision: build.revision,
        before_digest: build_digest(before)?,
        after_digest: build_digest(&build)?,
        page: None,
        root: selection.as_ref().map(|(root, _)| root.digest),
        digest: [0; 32],
    };
    receipt.digest = progress_digest(&receipt)?;
    build.progress = Some(Reference {
        key,
        digest: receipt.digest,
    });
    encode_build(&build)?;
    encode_progress(&receipt)?;
    Ok(PreparedTransition {
        build,
        receipt,
        selection,
    })
}

impl DomainMutation<SyndicDomain> for ControlTransition {
    type Error = ConstructionError;
    type Prepared = PreparedTransition;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut ledger = Ledger::default();
        ledger.reserve_read(0, 0, 24_576)?;
        let stored = ledger
            .point::<BuildsFamily>(reader, &self.before.key, 24_576)?
            .ok_or(ConstructionError::Collision)?;
        if stored != self.before {
            return Err(ConstructionError::Collision);
        }
        if stored.lifecycle != Lifecycle::Building {
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
        authenticate_source(reader, &mut ledger, &stored.binding)?;
        if let Some(head) = ledger.point::<HeadsFamily>(reader, &stored.key.resource, 168)? {
            authenticate_selected(reader, &mut ledger, &stored.binding, &head)?;
            return Err(ConstructionError::AlreadyCurrent);
        }
        let prepared = prepare_control(&stored, self.command)?;
        if ledger
            .point::<ProgressFamily>(reader, &prepared.receipt.key, 512)?
            .is_some()
        {
            return Err(ConstructionError::Collision);
        }
        let completion = prepared.selection.is_some();
        ledger.reserve_write(40, encode_build(&prepared.build)?.len(), completion)?;
        ledger.reserve_write(48, encode_progress(&prepared.receipt)?.len(), completion)?;
        if let Some((root, head)) = &prepared.selection {
            if ledger
                .point::<RootsFamily>(reader, &root.key, 4_608)?
                .is_some()
            {
                return Err(ConstructionError::Collision);
            }
            ledger.reserve_write(40, encode_root(root)?.len(), true)?;
            ledger.reserve_write(24, encode_head(head)?.len(), true)?;
        }
        Ok(prepared)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BuildsCodec>(1)?;
        reservation.reserve_records::<ProgressCodec>(1)?;
        if self.command == Command::Complete {
            reservation.reserve_records::<RootsCodec>(1)?;
            reservation.reserve_records::<HeadsCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BuildsCodec>(&prepared.build.key, &prepared.build)?;
        mutations.put::<ProgressCodec>(&prepared.receipt.key, &prepared.receipt)?;
        if let Some((root, head)) = prepared.selection {
            mutations.put::<RootsCodec>(&root.key, &root)?;
            mutations.put::<HeadsCodec>(&head.key, &head)?;
        }
        Ok(())
    }
}
