use super::*;
use crate::codec::ResourcesFamily;
use crate::resource_structure::{
    advance::prepare_code, query::QueryRecords, table_publication::prepare_table,
    transition::prepare_control,
};
use std::{cell::RefCell, collections::BTreeMap};

impl crate::resource_structure::source::SourceRecords for Fixture {
    fn predecessor<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
    ) -> Result<(F::Key, F::Value), ConstructionError> {
        let first = (F::NAME, F::encode_key(&first)?);
        let last = (F::NAME, F::encode_key(&last)?);
        self.reads.borrow_mut().push(last.clone());
        let ((_, key), value) = self
            .values
            .range(first..=last)
            .next_back()
            .ok_or(ConstructionError::Corrupt)?;
        Ok((F::decode_key(key)?, F::decode_value(value)?))
    }

    fn chunk(
        &self,
        key: &crate::codec::ContentChunkKey,
    ) -> Result<crate::ContentChunkRecord, ConstructionError> {
        self.point::<crate::codec::ContentChunksFamily>(key, 65_664)?
            .ok_or(ConstructionError::Corrupt)
    }
}

#[derive(Default)]
pub(super) struct Fixture {
    pub(super) values: BTreeMap<(&'static str, Vec<u8>), Vec<u8>>,
    pub(super) reads: RefCell<Vec<(&'static str, Vec<u8>)>>,
}

impl QueryRecords for Fixture {
    fn point<F: Family>(
        &self,
        key: &F::Key,
        limit: usize,
    ) -> Result<Option<F::Value>, ConstructionError> {
        let key = (F::NAME, F::encode_key(key)?);
        self.reads.borrow_mut().push(key.clone());
        self.values
            .get(&key)
            .map(|bytes| {
                if bytes.len() > limit {
                    return Err(ConstructionError::Limit);
                }
                Ok(F::decode_value(bytes)?)
            })
            .transpose()
    }
}

impl crate::resource_structure::query_anchor::AnchorRecords for Fixture {
    fn seek_anchor(
        &self,
        first: AnchorKey,
        last: AnchorKey,
        reverse: bool,
    ) -> Result<Option<(AnchorKey, AnchorLocator)>, ConstructionError> {
        let first = (SourceAnchorsFamily::NAME, anchor_key(first)?);
        let last = (SourceAnchorsFamily::NAME, anchor_key(last)?);
        self.reads
            .borrow_mut()
            .push(if reverse { last.clone() } else { first.clone() });
        let mut range = self.values.range(first..=last);
        let record = if reverse {
            range.next_back()
        } else {
            range.next()
        };
        record
            .map(|((_, key), value)| {
                Ok((
                    SourceAnchorsFamily::decode_key(key)?,
                    SourceAnchorsFamily::decode_value(value)?,
                ))
            })
            .transpose()
    }
}

impl Fixture {
    pub(super) fn source(&mut self, binding: &Binding, source: &[u8]) {
        use crate::codec::{
            ContentByteSpanKey, ContentByteSpansFamily, ContentChunkKey, ContentChunksFamily,
            ContentTextSpanKey, ContentTextSpansFamily,
        };
        let mut bytes = vec![b'x'; binding.range.start as usize];
        bytes.extend_from_slice(source);
        for (index, bytes) in bytes.chunks(65_536).enumerate() {
            let start = index as u64 * 65_536;
            let ordinal = crate::ContentChunkOrdinal::new(index as u64 + 1).unwrap();
            let owner = binding.source.content_id();
            let chunk = crate::ContentChunkRecord::new(owner, ordinal, bytes).unwrap();
            self.put::<ContentChunksFamily>(&ContentChunkKey { owner, ordinal }, &chunk);
            self.put::<ContentByteSpansFamily>(
                &ContentByteSpanKey { owner, start },
                &crate::ContentByteSpanRecord::for_chunk(&chunk, start).unwrap(),
            );
            self.put::<ContentTextSpansFamily>(
                &ContentTextSpanKey {
                    owner,
                    logical_start: start,
                },
                &crate::ContentTextSpanRecord::new(
                    owner,
                    crate::ContentPieceOrdinal::new(index as u64 + 1).unwrap(),
                    ordinal,
                    start,
                    start,
                    start + bytes.len() as u64,
                    start,
                    start + bytes.len() as u64,
                    false,
                    [0; 32],
                )
                .unwrap(),
            );
        }
    }
    pub(super) fn put<F: Family>(&mut self, key: &F::Key, value: &F::Value) {
        self.values.insert(
            (F::NAME, F::encode_key(key).unwrap()),
            F::encode_value(value).unwrap(),
        );
    }

    pub(super) fn remove<F: Family>(&mut self, key: &F::Key) {
        self.values.remove(&(F::NAME, F::encode_key(key).unwrap()));
    }

    fn control(&mut self, build: Build, command: Command) -> Build {
        let prepared = prepare_control(&build, command).unwrap();
        self.put::<BuildsFamily>(&prepared.build.key, &prepared.build);
        self.put::<ProgressFamily>(&prepared.receipt.key, &prepared.receipt);
        if let Some((root, head)) = prepared.selection {
            self.put::<RootsFamily>(&root.key, &root);
            self.put::<HeadsFamily>(&head.key, &head);
        }
        prepared.build
    }

    pub(super) fn code(source_bytes: &[u8], lines: u64) -> (Self, Binding) {
        let original = binding();
        let length = source_bytes.len() as u64 + 5;
        let source = ProjectionTextSource::composer(ContentReference::new(
            SyndicContentId::from_bytes([0x33; 16]),
            ContentRevision::new(2).unwrap(),
            ContentEncoding::ComposerV1,
            ContentSummary::new(
                1,
                1,
                length,
                length,
                1,
                0,
                [0; 32],
                None,
                SyndicContentDigest::from_bytes([0x44; 32]),
            )
            .unwrap(),
        ));
        let metadata = ResourceMetadataRecord::new(
            key().resource.id,
            key().resource.revision,
            original.metadata.projection_id().unwrap(),
            original.metadata.item_id(),
            ResourceOrdinal::FIRST,
            ResourceKind::Code,
            "text/plain",
            ResourceBacking::TextRange {
                source,
                range: ProjectionSourceRange::new(5, length).unwrap(),
            },
            [0x77; 32],
            None,
            ResourceStructure::code(Some("rust"), lines).unwrap(),
        )
        .unwrap();
        let mut build =
            crate::resource_structure::construction::initial_build(key(), metadata).unwrap();
        let mut fixture = Self::default();
        fixture.put::<ResourcesFamily>(&build.binding.resource.id, &build.binding.metadata);
        while build.checkpoint.phase != ParsePhase::Eof {
            let start = build.checkpoint.next_byte as usize;
            let end = (start + 65_536).min(source_bytes.len());
            let prepared = prepare_code(&build, &source_bytes[start..end], 256).unwrap();
            if let Some(page) = prepared.page {
                fixture.put::<PagesFamily>(&page.key, &page);
            }
            for locator in prepared.lines {
                fixture.put::<CodeLinesFamily>(&locator.key, &locator);
            }
            for anchor in prepared.anchors {
                fixture.put::<SourceAnchorsFamily>(&anchor.key, &anchor);
            }
            fixture.put::<ProgressFamily>(&prepared.receipt.key, &prepared.receipt);
            build = prepared.build;
        }
        build = fixture.control(build, Command::PassChange);
        build = fixture.control(build, Command::Complete);
        (fixture, build.binding)
    }

    pub(super) fn table(source: &[u8], body_rows: u64, columns: u64) -> (Self, Binding) {
        let mut build = super::super::fixture_support::table_build(source, body_rows, columns);
        let mut fixture = Self::default();
        fixture.put::<ResourcesFamily>(&build.binding.resource.id, &build.binding.metadata);
        let mut rows = Vec::new();
        for pass in [Pass::TableRows, Pass::TableCells, Pass::Columns] {
            assert_eq!(build.checkpoint.pass, pass);
            while build.checkpoint.phase != ParsePhase::Eof {
                let row = if pass == Pass::TableCells {
                    rows.iter().find(|row: &&Descriptor| {
                        row.coordinate() == (1, build.checkpoint.role, build.checkpoint.next_row, 0)
                    })
                } else {
                    None
                };
                let offered = if build.checkpoint.phase == ParsePhase::DrainingSyntax
                    || row.is_some_and(|row| {
                        matches!(
                            row,
                            Descriptor::TableRow {
                                disposition: RowDisposition::WholeRowFallback,
                                ..
                            }
                        )
                    }) {
                    &[][..]
                } else if let Some(row) = row {
                    let raw = row.raw().unwrap();
                    &source[build.checkpoint.next_byte.max(raw.start) as usize..raw.end as usize]
                } else {
                    &source[build.checkpoint.next_byte as usize..]
                };
                let prepared = prepare_table(&build, row, offered, 256).unwrap();
                if let Some(page) = prepared.page {
                    if pass == Pass::TableRows {
                        rows.extend(page.descriptors.iter().cloned());
                    }
                    fixture.put::<PagesFamily>(&page.key, &page);
                }
                for locator in prepared.locators {
                    match locator.key {
                        CoordinateKey::TableRow(..) => {
                            fixture.put::<TableRowsFamily>(&locator.key, &locator)
                        }
                        CoordinateKey::TableCell(..) => {
                            fixture.put::<TableCellsFamily>(&locator.key, &locator)
                        }
                        CoordinateKey::TableColumn(..) => {
                            fixture.put::<TableColumnsFamily>(&locator.key, &locator)
                        }
                        _ => unreachable!(),
                    }
                }
                fixture.put::<ProgressFamily>(&prepared.receipt.key, &prepared.receipt);
                for anchor in prepared.anchors {
                    fixture.put::<SourceAnchorsFamily>(&anchor.key, &anchor);
                }
                build = prepared.build;
            }
            build = fixture.control(build, Command::PassChange);
        }
        build = fixture.control(build, Command::Complete);
        (fixture, build.binding)
    }
}
