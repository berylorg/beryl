use super::*;

impl SyndicStorage {
    pub fn prepare_draft_marker_seal_begin(
        &self,
        store: &HomeStore,
        request: DraftMarkerSealRequestV1,
    ) -> Result<PreparedDraftMarkerSealBeginV1, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_begin_with_access(ReadAccess::Ordinary(store), request)
    }

    pub fn prepare_draft_marker_seal_begin_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        request: DraftMarkerSealRequestV1,
    ) -> Result<PreparedDraftMarkerSealBeginV1, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.prepare_draft_marker_seal_begin_with_access(ReadAccess::Candidate(store), request)
        })
    }

    pub fn prepare_draft_marker_seal_advance_with_limit(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        marker_limit: usize,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_advance_with_limit_with_access(
            ReadAccess::Ordinary(store),
            key,
            marker_limit,
        )
    }

    pub fn prepare_draft_marker_seal_advance_with_limit_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
        marker_limit: usize,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.prepare_draft_marker_seal_advance_with_limit_with_access(
                ReadAccess::Candidate(store),
                key,
                marker_limit,
            )
        })
    }

    pub fn prepare_draft_marker_seal_cancel(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<PreparedDraftMarkerSealCancelV1, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_cancel_with_access(ReadAccess::Ordinary(store), key)
    }

    pub fn prepare_draft_marker_seal_cancel_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
    ) -> Result<PreparedDraftMarkerSealCancelV1, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.prepare_draft_marker_seal_cancel_with_access(ReadAccess::Candidate(store), key)
        })
    }

    pub fn prepare_draft_marker_seal_fail(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        reason: DraftMarkerSealFailureReasonV1,
    ) -> Result<PreparedDraftMarkerSealFailV1, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_fail_with_access(ReadAccess::Ordinary(store), key, reason)
    }

    pub fn prepare_draft_marker_seal_fail_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
        reason: DraftMarkerSealFailureReasonV1,
    ) -> Result<PreparedDraftMarkerSealFailV1, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.prepare_draft_marker_seal_fail_with_access(
                ReadAccess::Candidate(store),
                key,
                reason,
            )
        })
    }

    pub fn prepare_draft_marker_seal_supersede(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
        successor: DraftMarkerSealOperationIdV1,
    ) -> Result<PreparedDraftMarkerSealSupersedeV1, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_supersede_with_access(
            ReadAccess::Ordinary(store),
            key,
            successor,
        )
    }

    pub fn prepare_draft_marker_seal_supersede_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
        successor: DraftMarkerSealOperationIdV1,
    ) -> Result<PreparedDraftMarkerSealSupersedeV1, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.prepare_draft_marker_seal_supersede_with_access(
                ReadAccess::Candidate(store),
                key,
                successor,
            )
        })
    }

    pub fn draft_marker_seal_status(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<DraftMarkerSealStatusV1, DraftMarkerSealErrorV1> {
        self.draft_marker_seal_status_with_access(ReadAccess::Ordinary(store), key)
    }

    pub fn draft_marker_seal_status_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
    ) -> Result<DraftMarkerSealStatusV1, DraftMarkerSealErrorV1> {
        self.with_candidate_revision(store, || {
            self.draft_marker_seal_status_with_access(ReadAccess::Candidate(store), key)
        })
    }

    pub fn prepare_draft_marker_seal_advance(
        &self,
        store: &HomeStore,
        key: DraftMarkerSealKeyV1,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_advance_with_limit(
            store,
            key,
            DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS,
        )
    }

    pub fn prepare_draft_marker_seal_advance_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        key: DraftMarkerSealKeyV1,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        self.prepare_draft_marker_seal_advance_with_limit_candidate(
            store,
            key,
            DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS,
        )
    }

    fn prepare_draft_marker_seal_begin_with_access(
        &self,
        store: ReadAccess<'_>,
        request: DraftMarkerSealRequestV1,
    ) -> Result<PreparedDraftMarkerSealBeginV1, DraftMarkerSealErrorV1> {
        validate_source(self, store, request.source())?;
        let mut initial = DraftMarkerSealRecordV1 {
            key: request.key(),
            cursor: DraftMarkerSealCursorV1::BeforeRoot,
            frontier: None,
            sequential_digest: sequential_marker_digest_seed(),
            ordered_asset_digest: ordered_marker_asset_digest_seed(),
            completed_marker_count: 0,
            maximum_image_label: None,
            lifecycle: DraftMarkerSealLifecycleV1::Open,
            record_digest: [0; 32],
        };
        initial.record_digest = seal_record_digest(&initial);
        if let Some(existing) = self.point_with_access::<DraftMarkerSealsFamily>(
            store,
            request.key(),
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )? {
            validate_record(&existing)?;
            if existing.key != request.key() {
                return Err(DraftMarkerSealErrorV1::IdentityCollision);
            }
        }
        Ok(PreparedDraftMarkerSealBeginV1 {
            initial,
            source: request.source(),
        })
    }

    fn prepare_draft_marker_seal_advance_with_limit_with_access(
        &self,
        store: ReadAccess<'_>,
        key: DraftMarkerSealKeyV1,
        marker_limit: usize,
    ) -> Result<Option<PreparedDraftMarkerSealAdvanceV1>, DraftMarkerSealErrorV1> {
        if marker_limit == 0 || marker_limit > DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS {
            return Err(DraftMarkerSealErrorV1::InvalidPageLimit);
        }
        let Some(expected) = self.point_with_access::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Err(DraftMarkerSealErrorV1::MissingSeal);
        };
        validate_record(&expected)?;
        if expected.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let source = validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &expected)?;
        if !matches!(expected.lifecycle, DraftMarkerSealLifecycleV1::Open) {
            return Ok(None);
        }

        let source_frontier = expected.completed_marker_count;
        let mut next = expected.clone();
        let mut markers = Vec::with_capacity(marker_limit);
        while markers.len() < marker_limit {
            let Some(frontier) = next_marker(self, store, &mut next)? else {
                break;
            };
            next.sequential_digest = advance_sequential_marker_digest(
                next.sequential_digest,
                frontier.marker_id,
                frontier.label,
            );
            next.ordered_asset_digest = advance_ordered_marker_asset_digest(
                next.ordered_asset_digest,
                frontier.marker_id,
                frontier.label,
                frontier.asset_id,
            );
            next.completed_marker_count = next
                .completed_marker_count
                .checked_add(1)
                .ok_or(DraftMarkerSealErrorV1::MarkerCountOverflow)?;
            next.maximum_image_label = Some(match next.maximum_image_label {
                Some(current) => current.max(frontier.label),
                None => frontier.label,
            });
            next.frontier = Some(frontier);
            markers.push(DraftMarkerSealOrderedMarkerV1 {
                marker_id: frontier.marker_id,
                label: frontier.label,
                asset_id: frontier.asset_id,
            });
        }

        close_exhausted_cursor(self, store, &mut next)?;
        let exact_eof = matches!(next.cursor, DraftMarkerSealCursorV1::Eof(_));
        if exact_eof {
            let commitment = key.commitment;
            if next.completed_marker_count != commitment.marker_count()
                || next.maximum_image_label != commitment.maximum_image_label()
                || source.marker_order_root() != key.marker_order_root
                || source.marker_commitment() != commitment
            {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
            let summary = SequentialMarkerSummaryV1::new(
                next.sequential_digest,
                next.completed_marker_count,
                next.maximum_image_label,
            )
            .map_err(|_| DraftMarkerSealErrorV1::Corruption)?;
            let ordered_assets = OrderedMarkerAssetSummaryV1::new(
                next.ordered_asset_digest,
                next.completed_marker_count,
            );
            next.lifecycle = DraftMarkerSealLifecycleV1::Sealed {
                sequential: summary,
                ordered_assets,
            };
        } else if markers.is_empty() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        next.record_digest = seal_record_digest(&next);
        let page = DraftMarkerSealPageV1 {
            markers,
            release: DraftMarkerSealPageReleaseV1 {
                key,
                source_frontier,
                target_frontier: next.completed_marker_count,
            },
            exact_eof,
        };
        Ok(Some(PreparedDraftMarkerSealAdvanceV1 {
            expected,
            next,
            page,
        }))
    }

    fn prepare_draft_marker_seal_cancel_with_access(
        &self,
        store: ReadAccess<'_>,
        key: DraftMarkerSealKeyV1,
    ) -> Result<PreparedDraftMarkerSealCancelV1, DraftMarkerSealErrorV1> {
        let Some(expected) = self.point_with_access::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Err(DraftMarkerSealErrorV1::MissingSeal);
        };
        validate_record(&expected)?;
        if expected.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &expected)?;
        let mut next = expected.clone();
        match expected.lifecycle {
            DraftMarkerSealLifecycleV1::Open => {
                next.lifecycle = DraftMarkerSealLifecycleV1::Cancelled;
                next.record_digest = seal_record_digest(&next);
            }
            DraftMarkerSealLifecycleV1::Cancelled => {}
            _ => return Err(DraftMarkerSealErrorV1::IdentityCollision),
        }
        Ok(PreparedDraftMarkerSealCancelV1 { expected, next })
    }

    fn prepare_draft_marker_seal_fail_with_access(
        &self,
        store: ReadAccess<'_>,
        key: DraftMarkerSealKeyV1,
        reason: DraftMarkerSealFailureReasonV1,
    ) -> Result<PreparedDraftMarkerSealFailV1, DraftMarkerSealErrorV1> {
        let (expected, next) = prepare_terminal_transition(
            self,
            store,
            key,
            DraftMarkerSealLifecycleV1::Failed(reason),
        )?;
        Ok(PreparedDraftMarkerSealFailV1 { expected, next })
    }

    fn prepare_draft_marker_seal_supersede_with_access(
        &self,
        store: ReadAccess<'_>,
        key: DraftMarkerSealKeyV1,
        successor: DraftMarkerSealOperationIdV1,
    ) -> Result<PreparedDraftMarkerSealSupersedeV1, DraftMarkerSealErrorV1> {
        if successor == key.operation_id {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let (expected, next) = prepare_terminal_transition(
            self,
            store,
            key,
            DraftMarkerSealLifecycleV1::Superseded(successor),
        )?;
        Ok(PreparedDraftMarkerSealSupersedeV1 { expected, next })
    }

    fn draft_marker_seal_status_with_access(
        &self,
        store: ReadAccess<'_>,
        key: DraftMarkerSealKeyV1,
    ) -> Result<DraftMarkerSealStatusV1, DraftMarkerSealErrorV1> {
        let Some(record) = self.point_with_access::<DraftMarkerSealsFamily>(
            store,
            key,
            storage_point_limit::<DraftMarkerSealsFamily>(),
        )?
        else {
            return Ok(DraftMarkerSealStatusV1::Absent);
        };
        validate_record(&record)?;
        if record.key != key {
            return Err(DraftMarkerSealErrorV1::IdentityCollision);
        }
        let source = validate_key_source(self, store, key)?;
        validate_cursor_closure(self, store, &record)?;
        let release = release_for(&record);
        Ok(match record.lifecycle {
            DraftMarkerSealLifecycleV1::Open => DraftMarkerSealStatusV1::Open {
                completed_marker_count: record.completed_marker_count,
            },
            DraftMarkerSealLifecycleV1::Cancelled => DraftMarkerSealStatusV1::Cancelled(release),
            DraftMarkerSealLifecycleV1::Failed(reason) => {
                DraftMarkerSealStatusV1::Failed { reason, release }
            }
            DraftMarkerSealLifecycleV1::Superseded(successor) => {
                DraftMarkerSealStatusV1::Superseded { successor, release }
            }
            DraftMarkerSealLifecycleV1::Sealed {
                sequential,
                ordered_assets,
            } => {
                if !matches!(record.cursor, DraftMarkerSealCursorV1::Eof(_))
                    || sequential.marker_digest() != record.sequential_digest
                    || sequential.marker_count() != record.completed_marker_count
                    || sequential.maximum_image_label() != record.maximum_image_label
                    || ordered_assets.marker_asset_digest() != record.ordered_asset_digest
                    || ordered_assets.marker_count() != record.completed_marker_count
                    || sequential.marker_count() != key.commitment.marker_count()
                    || sequential.maximum_image_label() != key.commitment.maximum_image_label()
                {
                    return Err(DraftMarkerSealErrorV1::Corruption);
                }
                DraftMarkerSealStatusV1::Sealed(
                    DraftMarkerSealProofV1::new_authenticated(
                        source,
                        key.commitment,
                        sequential,
                        ordered_assets,
                    ),
                    release,
                )
            }
        })
    }

    fn with_candidate_revision<T>(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        read: impl FnOnce() -> Result<T, DraftMarkerSealErrorV1>,
    ) -> Result<T, DraftMarkerSealErrorV1> {
        let revision = self
            .revision_candidate(store)
            .map_err(SyndicReadError::Read)?;
        let value = read()?;
        if self
            .revision_candidate(store)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "candidate marker seal preparation",
            }
            .into());
        }
        Ok(value)
    }
}

fn prepare_terminal_transition(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    key: DraftMarkerSealKeyV1,
    lifecycle: DraftMarkerSealLifecycleV1,
) -> Result<(DraftMarkerSealRecordV1, DraftMarkerSealRecordV1), DraftMarkerSealErrorV1> {
    let Some(expected) = storage.point_with_access::<DraftMarkerSealsFamily>(
        store,
        key,
        storage_point_limit::<DraftMarkerSealsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSeal);
    };
    validate_record(&expected)?;
    if expected.key != key {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    validate_key_source(storage, store, key)?;
    validate_cursor_closure(storage, store, &expected)?;
    let mut next = expected.clone();
    if expected.lifecycle == lifecycle {
        return Ok((expected, next));
    }
    if !matches!(expected.lifecycle, DraftMarkerSealLifecycleV1::Open) {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    next.lifecycle = lifecycle;
    next.record_digest = seal_record_digest(&next);
    Ok((expected, next))
}
