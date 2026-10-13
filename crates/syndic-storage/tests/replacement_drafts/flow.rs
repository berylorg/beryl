use super::*;
pub(super) fn begin(f: &Fixture, p: &PreparedDraftReplacementImportV1) {
    committed(execute(
        &f.store,
        f.storage
            .begin_replacement_draft_import(f.storage.revision(&f.store).unwrap(), p.clone()),
    ));
}
pub(super) fn advance_all(
    f: &Fixture,
    p: &PreparedDraftReplacementImportV1,
) -> DraftPieceRootReferenceV1 {
    let mut steps = 0;
    loop {
        steps += 1;
        assert!(steps < 10_000, "bounded fixture must finish");
        if let Some(root) = advance_one(f, p) {
            return root;
        }
    }
}
pub(super) fn advance_one(
    f: &Fixture,
    p: &PreparedDraftReplacementImportV1,
) -> Option<DraftPieceRootReferenceV1> {
    match f
        .storage
        .prepare_replacement_import_step(&f.store, p)
        .unwrap()
    {
        DraftReplacementAdvancePreparationV1::Ready(step) => committed(execute(
            &f.store,
            f.storage
                .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), step),
        )),
        DraftReplacementAdvancePreparationV1::Image(image) => {
            let asset = f.asset.unwrap();
            let page = image.descriptive_page(asset).unwrap();
            let witness = f
                .state
                .assets()
                .historical_input_marker_witness(&f.store, page)
                .unwrap();
            let mut attempt = f
                .storage
                .prepare_replacement_image_proof(&f.store, image, asset, witness)
                .unwrap();
            let receipt = f
                .store
                .compose_proof(attempt.take_command().unwrap())
                .unwrap();
            let step = attempt.consume(&f.storage, &f.store, receipt).unwrap();
            committed(execute(
                &f.store,
                f.storage
                    .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), step),
            ));
        }
        DraftReplacementAdvancePreparationV1::Complete(root) => return Some(root),
    }
    None
}
pub(super) fn start_command(f: &Fixture, p: &PreparedDraftReplacementStartV1) -> HomeCommand {
    let mut command = HomeCommand::new(f.store.home_revision().unwrap());
    command
        .add(
            f.storage
                .start_replacement_draft(f.storage.revision(&f.store).unwrap(), p.clone()),
        )
        .unwrap();
    command
        .add(
            f.state
                .assets()
                .historical_input_owner_selection(
                    &f.store,
                    p.basis().selector().draft_id(),
                    p.basis().item_id(),
                    p.basis().asset_reference_set(),
                )
                .unwrap(),
        )
        .unwrap();
    command
}
pub(super) fn start(
    f: &Fixture,
    p: &PreparedDraftReplacementImportV1,
) -> PreparedDraftReplacementStartV1 {
    let p = f
        .storage
        .prepare_replacement_draft_start(&f.store, p, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let outcome = f.store.execute(start_command(f, &p));
    assert_eq!(
        f.storage
            .reconcile_replacement_draft_start(&f.store, &p, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(p.selector())
    );
    p
}
pub(super) fn cancel(
    f: &Fixture,
    head: &DraftEditorCandidateSessionV1,
    op: u8,
) -> PreparedDraftReplacementCancellationV1 {
    let current = ordinary::current(&f.storage, &f.store, f.thread);
    let request = DraftReplacementCancellationRequestV1::new(
        ordinary::selector(&current),
        head.session_id(),
        head.session_generation(),
        operation(op),
        f.intent,
        SyndicTimestamp::from_unix_millis(4),
    );
    let prepared = f
        .storage
        .prepare_replacement_intent_cancellation(&f.store, request)
        .unwrap();
    let outcome = execute(
        &f.store,
        f.storage
            .cancel_replacement_intent(f.storage.revision(&f.store).unwrap(), prepared.clone()),
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_intent_cancellation(&f.store, &prepared, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::IntentCancelled(prepared.receipt().selector())
    );
    prepared
}
pub(super) fn text(f: &Fixture, root: DraftPieceRootReferenceV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    let length = root.summary().logical_utf8_bytes();
    let mut cursor = 0;
    while cursor < length {
        let window = f
            .storage
            .draft_piece_text_demand(
                &f.store,
                root,
                DraftPieceTextDemandV1::Forward(cursor),
                4096,
            )
            .unwrap();
        assert_eq!(window.start(), cursor);
        assert!(!window.bytes().is_empty());
        assert!(window.bytes().len() <= 4096);
        cursor += window.bytes().len() as u64;
        bytes.extend_from_slice(window.bytes());
    }
    bytes
}
