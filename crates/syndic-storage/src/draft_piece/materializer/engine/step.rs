use super::*;

impl DomainMutation<SyndicDomain> for StepMutation {
    type Error = SyndicMutationError;
    type Prepared = PreparedDraftComposerStepV1;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut prepared = self.prepared;
        if point::<DraftComposerBuildsFamily>(reader, &prepared.expected.key())?
            != Some(prepared.expected.clone())
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        match (&prepared.expected_manifest, &prepared.next_manifest) {
            (Some(expected), _) => {
                if point::<ContentManifestsFamily>(reader, &expected.id())?
                    != Some(expected.clone())
                {
                    return Err(SyndicMutationError::ContentManifestConflict);
                }
            }
            (None, Some(next)) => {
                if point::<ContentManifestsFamily>(reader, &next.id())?.is_some() {
                    return Err(SyndicMutationError::ContentIdentityCollision);
                }
            }
            (None, None) => {}
        }
        validate_exact_or_absent_records(reader, &mut prepared)?;
        if let Some(mapping) = prepared.mapping {
            if let Some(existing) =
                point::<DraftComposerMaterializationsFamily>(reader, &mapping.key())?
                && existing != mapping
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        Ok(prepared)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftComposerBuildsCodec>(1)?;
        if self.prepared.next_manifest.is_some() {
            reservation.reserve_records::<ContentManifestsCodec>(1)?;
        }
        if self.prepared.chunk.is_some() {
            reservation.reserve_records::<ContentChunksCodec>(1)?;
        }
        if self.prepared.byte_span.is_some() {
            reservation.reserve_records::<ContentByteSpansCodec>(1)?;
        }
        if self.prepared.text_span.is_some() {
            reservation.reserve_records::<ContentTextSpansCodec>(1)?;
        }
        if self.prepared.piece.is_some() {
            reservation.reserve_records::<ContentPiecesCodec>(1)?;
        }
        if self.prepared.mapping.is_some() {
            reservation.reserve_records::<DraftComposerMaterializationsCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let p = &prepared;
        if let Some(manifest) = &p.next_manifest {
            mutations.put::<ContentManifestsCodec>(&manifest.id(), manifest)?;
        }
        if let Some(chunk) = &p.chunk {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        if let Some(span) = &p.byte_span {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        if let Some(span) = &p.text_span {
            mutations.put::<ContentTextSpansCodec>(
                &ContentTextSpanKey {
                    owner: span.content_id(),
                    logical_start: span.logical_start(),
                },
                span,
            )?;
        }
        if let Some(piece) = &p.piece {
            mutations.put::<ContentPiecesCodec>(
                &ContentPieceKey {
                    owner: piece.content_id(),
                    ordinal: piece.ordinal(),
                },
                piece,
            )?;
        }
        if let Some(mapping) = &p.mapping {
            mutations.put::<DraftComposerMaterializationsCodec>(&mapping.key(), mapping)?;
        }
        mutations.put::<DraftComposerBuildsCodec>(&p.next.key(), &p.next)?;
        Ok(())
    }
}

fn validate_exact_or_absent_records(
    reader: &DomainReader<'_, SyndicDomain>,
    p: &mut PreparedDraftComposerStepV1,
) -> Result<(), SyndicMutationError> {
    let sealed = p
        .expected_manifest
        .as_ref()
        .is_some_and(|manifest| manifest.lifecycle() == ContentLifecycle::Sealed);
    let replay_chunk = sealed
        || p.expected_manifest.as_ref().is_some_and(|manifest| {
            p.chunk
                .as_ref()
                .is_some_and(|chunk| chunk.ordinal().get() <= manifest.chunk_count())
        });
    if let Some(chunk) = &p.chunk {
        let existing = point::<ContentChunksFamily>(
            reader,
            &ContentChunkKey {
                owner: chunk.content_id(),
                ordinal: chunk.ordinal(),
            },
        )?;
        match existing {
            Some(existing) if &existing != chunk => {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
            Some(_) => p.chunk = None,
            None if replay_chunk => {
                return Err(SyndicMutationError::RequiredRecordMissing {
                    family: "content-chunks",
                });
            }
            None => {}
        }
    }
    if let Some(span) = &p.byte_span {
        let existing = point::<ContentByteSpansFamily>(
            reader,
            &ContentByteSpanKey {
                owner: span.content_id(),
                start: span.start(),
            },
        )?;
        match existing {
            Some(existing) if &existing != span => {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
            Some(_) => p.byte_span = None,
            None if replay_chunk => {
                return Err(SyndicMutationError::RequiredRecordMissing {
                    family: "content-byte-spans",
                });
            }
            None => {}
        }
    }
    if let Some(span) = &p.text_span {
        let existing = point::<ContentTextSpansFamily>(
            reader,
            &ContentTextSpanKey {
                owner: span.content_id(),
                logical_start: span.logical_start(),
            },
        )?;
        match existing {
            Some(existing) if &existing != span => {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
            Some(_) => p.text_span = None,
            None if sealed => {
                return Err(SyndicMutationError::RequiredRecordMissing {
                    family: "content-text-spans",
                });
            }
            None => {}
        }
    }
    if let Some(piece) = &p.piece {
        let existing = point::<ContentPiecesFamily>(
            reader,
            &ContentPieceKey {
                owner: piece.content_id(),
                ordinal: piece.ordinal(),
            },
        )?;
        match existing {
            Some(existing) if &existing != piece => {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
            Some(_) => p.piece = None,
            None if sealed => {
                return Err(SyndicMutationError::RequiredRecordMissing {
                    family: "content-pieces",
                });
            }
            None => {}
        }
    }
    Ok(())
}
