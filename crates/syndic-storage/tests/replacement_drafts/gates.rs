use super::*;

#[test]
fn occupied_import_identity_and_busy_editor_do_not_reset_the_prefix() {
    let f = Fixture::new(
        "busy-import",
        ComposerPayload::new(vec![ComposerAtom::text("source").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    let first = f.head();
    assert!(
        f.storage
            .prepare_replacement_draft_import(&f.store, f.request())
            .is_err()
    );
    let replay = execute(
        &f.store,
        f.storage
            .begin_replacement_draft_import(f.storage.revision(&f.store).unwrap(), p.clone()),
    );
    assert!(matches!(
        replay,
        CommandOutcome::NotCommitted { .. } | CommandOutcome::Committed { .. }
    ));
    assert_eq!(f.head(), first);
    let step = match f
        .storage
        .prepare_replacement_import_step(&f.store, &p)
        .unwrap()
    {
        DraftReplacementAdvancePreparationV1::Ready(s) => s,
        _ => panic!(),
    };
    committed(execute(
        &f.store,
        f.storage
            .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), step),
    ));
    let advanced = f.head();
    assert!(advanced.session_generation() > first.session_generation());
    assert!(matches!(
        execute(
            &f.store,
            f.storage
                .begin_replacement_draft_import(f.storage.revision(&f.store).unwrap(), p)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(f.head(), advanced);
}

#[test]
fn exact_entry_and_canonical_item_revision_are_required_before_begin() {
    let f = Fixture::new(
        "source-entry-gates",
        ComposerPayload::new(vec![ComposerAtom::text("source").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let r = f.request();
    let selector = ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread));
    let stale = ReplacementEditIntent::new(
        f.intent.target_turn_id(),
        f.intent.selected_path(),
        CurrentTranscriptEntryProof::new(
            TranscriptGeneration::new(2).unwrap(),
            TranscriptPosition::FIRST,
        ),
    );
    for (intent, revision, item) in [
        (
            stale,
            beryl_model::ProjectionRevision::new(1).unwrap(),
            f.item,
        ),
        (
            f.intent,
            beryl_model::ProjectionRevision::new(2).unwrap(),
            f.item,
        ),
        (
            f.intent,
            beryl_model::ProjectionRevision::new(1).unwrap(),
            SyndicItemId::from_bytes([221; 16]),
        ),
    ] {
        let request = DraftReplacementImportRequestV1::new(
            selector,
            DraftEditorCandidateActivationBindingV1::from_head(&f.session),
            operation(207),
            intent,
            item,
            revision,
            f.content,
            f.storage
                .input_gate(
                    &f.store,
                    f.thread,
                    SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap()
                .revision(),
            f.proof,
            point(0),
            point(0),
            point(0),
            point(0),
        );
        assert!(
            f.storage
                .prepare_replacement_draft_import(&f.store, request)
                .is_err()
        );
    }
    assert!(
        f.storage
            .prepare_replacement_draft_import(&f.store, r)
            .is_ok()
    );
    assert_eq!(f.head(), f.session);
}

fn source_request(
    f: &Fixture,
    content: ContentReference,
    gate: beryl_model::InputGateRevision,
    assets: Option<SealedAssetReferenceSetProof>,
) -> DraftReplacementImportRequestV1 {
    DraftReplacementImportRequestV1::new(
        ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread)),
        DraftEditorCandidateActivationBindingV1::from_head(&f.session),
        operation(207),
        f.intent,
        f.item,
        beryl_model::ProjectionRevision::new(1).unwrap(),
        content,
        gate,
        assets,
        point(0),
        point(0),
        point(0),
        point(0),
    )
}

#[test]
fn caller_source_expectations_reject_stale_content_and_idle_gate_revisions() {
    let f = Fixture::new(
        "source-expectations",
        ComposerPayload::default(),
        vec![],
        65_536,
    );
    let prior = f.request();
    let foreign = ContentReference::new(
        beryl_model::SyndicContentId::from_bytes([231; 16]),
        f.content.revision(),
        f.content.encoding(),
        f.content.summary(),
    );
    assert!(matches!(
        f.storage.prepare_replacement_draft_import(
            &f.store,
            source_request(
                &f,
                foreign,
                beryl_model::InputGateRevision::new(1).unwrap(),
                None
            )
        ),
        Err(DraftReplacementErrorV1::Conflict)
    ));
    let gate = InputGateRecord::new(
        f.thread,
        beryl_model::InputGateRevision::new(2).unwrap(),
        InputGateState::Idle,
        0,
        None,
        None,
        0,
        0,
        0,
    )
    .unwrap();
    support::commit(
        &f.store,
        f.storage.clone(),
        support::batch(vec![FixtureRecord::InputGate(gate)]),
    );
    assert!(matches!(
        f.storage.prepare_replacement_draft_import(&f.store, prior),
        Err(DraftReplacementErrorV1::Conflict)
    ));
    assert!(
        f.storage
            .prepare_replacement_draft_import(
                &f.store,
                source_request(
                    &f,
                    f.content,
                    beryl_model::InputGateRevision::new(2).unwrap(),
                    None
                )
            )
            .is_ok()
    );
    assert_eq!(f.head(), f.session);
}

#[test]
fn caller_must_supply_the_exact_historical_asset_reference_proof() {
    let marker = SyndicDraftMarkerId::from_bytes([232; 16]);
    let label = ImageLabelOrdinal::new(9).unwrap();
    let f = Fixture::new(
        "source-asset-expectation",
        ComposerPayload::new(vec![ComposerAtom::image_marker(marker, label)]).unwrap(),
        vec![(marker, label)],
        65_536,
    );
    assert!(matches!(
        f.storage.prepare_replacement_draft_import(
            &f.store,
            source_request(
                &f,
                f.content,
                beryl_model::InputGateRevision::new(1).unwrap(),
                None
            )
        ),
        Err(DraftReplacementErrorV1::Conflict)
    ));
    assert!(
        f.storage
            .prepare_replacement_draft_import(&f.store, f.request())
            .is_ok()
    );
    assert_eq!(f.head(), f.session);
}
