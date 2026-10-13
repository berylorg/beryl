use super::*;

fn marker_fixture(name: &str) -> Fixture {
    let id = SyndicDraftMarkerId::from_bytes([240; 16]);
    let label = ImageLabelOrdinal::new(7).unwrap();
    Fixture::new(
        name,
        ComposerPayload::new(vec![ComposerAtom::image_marker(id, label)]).unwrap(),
        vec![(id, label)],
        65_536,
    )
}
fn image(f: &Fixture, p: &PreparedDraftReplacementImportV1) -> DraftReplacementImageSourceV1 {
    loop {
        match f
            .storage
            .prepare_replacement_import_step(&f.store, p)
            .unwrap()
        {
            DraftReplacementAdvancePreparationV1::Ready(s) => committed(execute(
                &f.store,
                f.storage
                    .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), s),
            )),
            DraftReplacementAdvancePreparationV1::Image(image) => return image,
            DraftReplacementAdvancePreparationV1::Complete(_) => panic!("marker frame absent"),
        }
    }
}
fn attempt(
    f: &Fixture,
    image: DraftReplacementImageSourceV1,
) -> DraftReplacementImageProofAttemptV1 {
    let asset = f.asset.unwrap();
    let page = image.descriptive_page(asset).unwrap();
    let witness = f
        .state
        .assets()
        .historical_input_marker_witness(&f.store, page)
        .unwrap();
    f.storage
        .prepare_replacement_image_proof(&f.store, image, asset, witness)
        .unwrap()
}

#[test]
fn genuine_image_proof_and_qualified_advance_cannot_transfer_to_identical_foreign_records() {
    let a = marker_fixture("image-proof-a");
    let b = marker_fixture("image-proof-b");
    let pa = a.prepare();
    let pb = b.prepare();
    assert_eq!(pa.canonical_header(), pb.canonical_header());
    begin(&a, &pa);
    begin(&b, &pb);
    let ia = image(&a, &pa);
    let _ib = image(&b, &pb);
    let before = b.head();
    let asset = a.asset.unwrap();
    let page = ia.descriptive_page(asset).unwrap();
    let witness = b
        .state
        .assets()
        .historical_input_marker_witness(&b.store, page)
        .unwrap();
    assert!(matches!(
        b.storage
            .prepare_replacement_image_proof(&b.store, ia, asset, witness),
        Err(DraftReplacementErrorV1::Proof)
    ));
    let mut proof = attempt(&a, image(&a, &pa));
    let receipt = a
        .store
        .compose_proof(proof.take_command().unwrap())
        .unwrap();
    let step = proof.consume(&a.storage, &a.store, receipt).unwrap();
    let unavailable = b
        .state
        .assets()
        .verify_sidecar(&b.store, b.asset.unwrap())
        .unwrap();
    std::fs::remove_file(unavailable.path()).unwrap();
    assert_eq!(
        b.store.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    let outcome = execute(
        &b.store,
        b.storage
            .advance_replacement_draft_import(b.storage.revision(&b.store).unwrap(), step),
    );
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    assert_eq!(b.head(), before);
    let mut proof = attempt(&a, image(&a, &pa));
    assert!(
        b.store
            .compose_proof(proof.take_command().unwrap())
            .is_err()
    );
    assert_eq!(b.head(), before);
    assert!(
        b.state
            .assets()
            .verify_sidecar(&b.store, b.asset.unwrap())
            .is_err()
    );
}

#[test]
fn original_consumer_rejects_another_genuine_receipt_for_the_same_marker_window() {
    let f = marker_fixture("original-proof-consumer");
    let p = f.prepare();
    begin(&f, &p);
    let mut first = attempt(&f, image(&f, &p));
    let mut second = attempt(&f, image(&f, &p));
    let before = f.head();
    let a = f
        .store
        .compose_proof(first.take_command().unwrap())
        .unwrap();
    let b = f
        .store
        .compose_proof(second.take_command().unwrap())
        .unwrap();
    assert!(matches!(
        first.consume(&f.storage, &f.store, b),
        Err(DraftReplacementErrorV1::Proof)
    ));
    assert!(matches!(
        second.consume(&f.storage, &f.store, a),
        Err(DraftReplacementErrorV1::Proof)
    ));
    assert_eq!(f.head(), before);
    let mut proof = attempt(&f, image(&f, &p));
    let receipt = f
        .store
        .compose_proof(proof.take_command().unwrap())
        .unwrap();
    let step = proof.consume(&f.storage, &f.store, receipt).unwrap();
    committed(execute(
        &f.store,
        f.storage
            .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), step),
    ));
    advance_all(&f, &p);
    start(&f, &p);
}

#[test]
fn a_retired_image_consumer_does_not_qualify_a_new_open_generation() {
    let f = marker_fixture("retired-image-consumer");
    let p = f.prepare();
    begin(&f, &p);
    let mut proof = attempt(&f, image(&f, &p));
    let receipt = f
        .store
        .compose_proof(proof.take_command().unwrap())
        .unwrap();
    let before = f.head();
    let f = f.reopen();
    assert!(matches!(
        proof.consume(&f.storage, &f.store, receipt),
        Err(DraftReplacementErrorV1::Proof)
    ));
    assert_eq!(f.head(), before);
}
