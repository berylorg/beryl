use super::*;

impl SyndicStorage {
    pub(super) fn prepare_plan_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let source = build.key().source();
        let cursor = build.encoder().cursor();
        let page = read_materialization_page(
            self,
            store,
            source,
            cursor.piece_index(),
            DRAFT_COMPOSER_INPUT_MAX_RECORDS,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let mut work = EncoderWork::from_state(build.encoder());
        let flushed = advance_encoder(
            &mut work,
            page.pieces(),
            source.summary().piece_count(),
            SyndicContentId::from_bytes([0; 16]),
        )?;
        let at_eof = work.cursor.piece_index() == source.summary().piece_count()
            && work.cursor.atom_encoded_offset() == 0;
        let mut next_manifest = None;
        let mut expected_manifest = None;
        let lifecycle;
        let output;
        let output_revision;
        if at_eof {
            if flushed.is_none() {
                let _ = work.flush(SyndicContentId::from_bytes([0; 16]))?;
            }
            let summary = work.summary(source.summary().piece_count())?;
            if summary.logical_utf8_bytes() != source.summary().logical_utf8_bytes()
                || summary.image_marker_count() != source.summary().marker_count()
            {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let content_id = content_id_for_summary(summary);
            let revision = ContentRevision::new(1)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let mut reference =
                ContentReference::new(content_id, revision, ContentEncoding::ComposerV1, summary);
            let existing = self.point::<ContentManifestsFamily>(
                store,
                content_id,
                storage_point_limit::<ContentManifestsFamily>(),
            )?;
            if let Some(existing) = existing {
                if existing.encoding() != ContentEncoding::ComposerV1
                    || existing.expected() != summary
                    || existing.owner().is_some()
                {
                    return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
                }
                reference = ContentReference::new(
                    content_id,
                    existing.revision(),
                    ContentEncoding::ComposerV1,
                    summary,
                );
                expected_manifest = Some(existing);
            } else {
                next_manifest = Some(ContentManifestRecord::new(
                    content_id,
                    revision,
                    ContentEncoding::ComposerV1,
                    ContentLifecycle::Building,
                    0,
                    0,
                    content_chain_seed(ContentEncoding::ComposerV1),
                    summary,
                ));
            }
            work = EncoderWork::initial(source.summary().piece_count());
            lifecycle = DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing);
            output = Some(reference);
            output_revision = Some(reference.revision());
        } else {
            lifecycle = DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Planning);
            output = None;
            output_revision = None;
        }
        let next = DraftComposerBuildRecordV1::new(
            build.key(),
            work.state(),
            build.records(),
            output,
            output_revision,
            0,
            0,
            content_chain_seed(ContentEncoding::ComposerV1),
            lifecycle,
        );
        prepared(
            build,
            next,
            expected_manifest,
            next_manifest,
            None,
            None,
            None,
            None,
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    pub(super) fn prepare_write_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let reference = build
            .output()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let manifest = self
            .point::<ContentManifestsFamily>(
                store,
                reference.id(),
                storage_point_limit::<ContentManifestsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        validate_build_manifest(&build, &manifest)?;
        let cursor = build.encoder().cursor();
        let page = read_materialization_page(
            self,
            store,
            build.key().source(),
            cursor.piece_index(),
            DRAFT_COMPOSER_INPUT_MAX_RECORDS,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let mut work = EncoderWork::from_state(build.encoder());
        let mut emitted = advance_encoder(
            &mut work,
            page.pieces(),
            build.key().source().summary().piece_count(),
            reference.id(),
        )?;
        let at_eof = work.cursor.piece_index() == build.key().source().summary().piece_count()
            && work.cursor.atom_encoded_offset() == 0;
        let final_chunk = at_eof;
        if emitted.is_none() && at_eof {
            emitted = work.flush(reference.id())?;
        }
        let Some((chunk, byte_span)) = emitted else {
            let next = copy_build(
                &build,
                work.state(),
                build.records(),
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Writing),
            );
            return prepared(
                build,
                next,
                Some(manifest),
                None,
                None,
                None,
                None,
                None,
                None,
                page.records_read(),
                page.payload_bytes(),
            );
        };
        let chunk_end = byte_span.end();
        let next_manifest = if chunk.ordinal().get() <= manifest.chunk_count() {
            if chunk.ordinal().get() == manifest.chunk_count()
                && (chunk_end != manifest.encoded_bytes()
                    || work.chain_digest != manifest.chain_digest())
            {
                return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
            }
            None
        } else {
            if manifest.lifecycle() != ContentLifecycle::Building
                || chunk.ordinal().get() != checked_next(manifest.chunk_count())?
                || build.output_chunk_count() != manifest.chunk_count()
                || build.output_encoded_bytes() != manifest.encoded_bytes()
                || build.output_chain_digest() != manifest.chain_digest()
            {
                return Err(DraftComposerMaterializationErrorV1::InvalidOutput);
            }
            Some(ContentManifestRecord::new(
                manifest.id(),
                manifest
                    .revision()
                    .checked_next()
                    .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?,
                manifest.encoding(),
                ContentLifecycle::Building,
                work.chunk_count,
                chunk_end,
                work.chain_digest,
                manifest.expected(),
            ))
        };
        let next_revision = next_manifest
            .as_ref()
            .map_or(manifest.revision(), |next| next.revision());
        let next_reference = ContentReference::new(
            reference.id(),
            next_revision,
            ContentEncoding::ComposerV1,
            reference.summary(),
        );
        let next_chain = work.chain_digest;
        let next = copy_build(
            &build,
            work.state(),
            build.records(),
            Some(next_reference),
            Some(next_revision),
            chunk.ordinal().get(),
            chunk_end,
            next_chain,
            DraftComposerBuildLifecycleV1::Open(DraftComposerBuildPhaseV1::Draining {
                final_chunk,
            }),
        );
        prepared(
            build,
            next,
            Some(manifest),
            next_manifest,
            Some(chunk),
            Some(byte_span),
            None,
            None,
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    pub(super) fn prepare_drain_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
        final_chunk: bool,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let source = build.key().source();
        let frontier = build.records();
        let manifest = self
            .point::<ContentManifestsFamily>(
                store,
                build
                    .output()
                    .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?
                    .id(),
                storage_point_limit::<ContentManifestsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        validate_build_manifest(&build, &manifest)?;
        if frontier.cursor().piece_index() == source.summary().piece_count() {
            if frontier.encoded_bytes() != build.output_encoded_bytes() {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let phase = if final_chunk {
                DraftComposerBuildPhaseV1::ReadyToSeal
            } else {
                DraftComposerBuildPhaseV1::Writing
            };
            let next_frontier = if final_chunk {
                frontier
            } else {
                DraftComposerRecordFrontierV1::new(
                    frontier.cursor(),
                    frontier.encoded_bytes(),
                    frontier.logical_utf8_bytes(),
                    frontier.piece_count(),
                    frontier.marker_count(),
                    frontier.marker_digest(),
                    frontier.maximum_image_label(),
                    build.output_encoded_bytes(),
                    checked_next(frontier.chunk_ordinal())?,
                    frontier.break_before(),
                )
            };
            let next = copy_build(
                &build,
                build.encoder().clone(),
                next_frontier,
                build.output(),
                build.output_revision(),
                build.output_chunk_count(),
                build.output_encoded_bytes(),
                build.output_chain_digest(),
                DraftComposerBuildLifecycleV1::Open(phase),
            );
            return prepared(
                build,
                next,
                Some(manifest),
                None,
                None,
                None,
                None,
                None,
                None,
                0,
                0,
            );
        }
        let page = read_materialization_page(
            self,
            store,
            source,
            frontier.cursor().piece_index(),
            1,
            DRAFT_COMPOSER_INPUT_MAX_BYTES,
        )?;
        let piece = page
            .pieces()
            .first()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let (next_frontier, text_span, output_piece) = drain_record(&build, piece)?;
        let drained_all = next_frontier.encoded_bytes() == build.output_encoded_bytes();
        let phase = if drained_all {
            if final_chunk && next_frontier.cursor().piece_index() == source.summary().piece_count()
            {
                DraftComposerBuildPhaseV1::ReadyToSeal
            } else {
                DraftComposerBuildPhaseV1::Writing
            }
        } else {
            DraftComposerBuildPhaseV1::Draining { final_chunk }
        };
        let next_frontier = if drained_all && !final_chunk {
            DraftComposerRecordFrontierV1::new(
                next_frontier.cursor(),
                next_frontier.encoded_bytes(),
                next_frontier.logical_utf8_bytes(),
                next_frontier.piece_count(),
                next_frontier.marker_count(),
                next_frontier.marker_digest(),
                next_frontier.maximum_image_label(),
                build.output_encoded_bytes(),
                checked_next(next_frontier.chunk_ordinal())?,
                next_frontier.break_before(),
            )
        } else {
            next_frontier
        };
        let next = copy_build(
            &build,
            build.encoder().clone(),
            next_frontier,
            build.output(),
            build.output_revision(),
            build.output_chunk_count(),
            build.output_encoded_bytes(),
            build.output_chain_digest(),
            DraftComposerBuildLifecycleV1::Open(phase),
        );
        prepared(
            build,
            next,
            Some(manifest),
            None,
            None,
            None,
            text_span,
            output_piece,
            None,
            page.records_read(),
            page.payload_bytes(),
        )
    }

    pub(super) fn prepare_seal_step(
        &self,
        store: &HomeStore,
        build: DraftComposerBuildRecordV1,
    ) -> Result<PreparedDraftComposerStepV1, DraftComposerMaterializationErrorV1> {
        let output = build
            .output()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
        let manifest = self
            .point::<ContentManifestsFamily>(
                store,
                output.id(),
                storage_point_limit::<ContentManifestsFamily>(),
            )?
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        validate_build_manifest(&build, &manifest)?;
        let eof = read_materialization_page(
            self,
            store,
            build.key().source(),
            build.key().source().summary().piece_count(),
            1,
            1,
        )?;
        if !eof.pieces().is_empty()
            || build.encoder().cursor().piece_index()
                != build.key().source().summary().piece_count()
            || build.encoder().cursor().atom_encoded_offset() != 0
            || !build.encoder().carry().is_empty()
            || build.records().cursor().piece_index()
                != build.key().source().summary().piece_count()
            || build.records().encoded_bytes() != output.summary().encoded_bytes()
            || build.records().logical_utf8_bytes() != output.summary().logical_utf8_bytes()
            || build.records().piece_count() != output.summary().piece_count()
            || build.records().marker_count() != output.summary().image_marker_count()
            || build.records().marker_digest() != output.summary().marker_digest()
            || build.records().maximum_image_label() != output.summary().maximum_image_label()
            || manifest.chunk_count() != output.summary().chunk_count()
            || manifest.encoded_bytes() != output.summary().encoded_bytes()
            || manifest.chain_digest() != output.summary().digest()
        {
            return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
        }
        let sealed_manifest = if manifest.lifecycle() == ContentLifecycle::Sealed {
            None
        } else {
            Some(ContentManifestRecord::new(
                manifest.id(),
                manifest
                    .revision()
                    .checked_next()
                    .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?,
                ContentEncoding::ComposerV1,
                ContentLifecycle::Sealed,
                manifest.chunk_count(),
                manifest.encoded_bytes(),
                manifest.chain_digest(),
                manifest.expected(),
            ))
        };
        let reference = sealed_manifest
            .as_ref()
            .unwrap_or(&manifest)
            .sealed_reference()
            .ok_or(DraftComposerMaterializationErrorV1::InvalidOutput)?;
        let mapping = DraftComposerMaterializationRecordV1::new(
            DraftComposerMaterializationKeyV1::new(build.key().source(), build.key().format()),
            build.key().operation(),
            reference,
        );
        let next = copy_build(
            &build,
            build.encoder().clone(),
            build.records(),
            Some(reference),
            Some(reference.revision()),
            build.output_chunk_count(),
            build.output_encoded_bytes(),
            build.output_chain_digest(),
            DraftComposerBuildLifecycleV1::Sealed(reference),
        );
        prepared(
            build,
            next,
            Some(manifest),
            sealed_manifest,
            None,
            None,
            None,
            None,
            Some(mapping),
            eof.records_read(),
            0,
        )
    }
}
