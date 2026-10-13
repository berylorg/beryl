#![cfg(feature = "test-faults")]
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod ordinary;
mod support;
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
use sha2::{Digest, Sha256};
use std::io::Write;
use syndic_storage::test_faults::draft_canonical_encoding_for_test;
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceSettlementKeyV1, SyndicStorage,
};

#[test]
fn ordinary_inflight_complete_and_terminal_record_bytes_are_frozen() {
    let home = support::TestHome::new("ordinary-encoding-vectors");
    if let Some(ledger) = std::env::var_os("BERYL_REPLACEMENT_FIXTURE_LEDGER") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger)
            .unwrap();
        writeln!(file, "{}", home.path().canonicalize().unwrap().display()).unwrap();
    }
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = ordinary::create_thread(&storage, &store, 37, 65_536);
    let current = ordinary::current(&storage, &store, thread);
    let opened = ordinary::open_session(&storage, &store, &current, 39, 40);
    let edit = ordinary::transaction(&storage, &store, &opened, 41, "α\r\n🙂", ordinary::point(8));
    let key = DraftPieceSettlementKeyV1::new(
        opened.draft_id(),
        opened.session_id(),
        edit.prepared.header().operation_id(),
    );
    ordinary::committed(ordinary::execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let inflight = head(&storage, &store, &opened);
    let first = draft_canonical_encoding_for_test(&store, &storage, key, &inflight);
    for fragment in &edit.fragments {
        ordinary::committed(ordinary::execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    while let Some(step) = storage
        .prepare_draft_piece_build_advance(
            &store,
            opened.draft_id(),
            opened.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
    {
        ordinary::committed(ordinary::execute(
            &store,
            storage.advance_draft_piece_edit(step),
        ));
    }
    let complete = head(&storage, &store, &opened);
    let second = draft_canonical_encoding_for_test(&store, &storage, key, &complete);
    ordinary::committed(ordinary::execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let terminal = head(&storage, &store, &opened);
    let third = draft_canonical_encoding_for_test(&store, &storage, key, &terminal);
    let hashes = [first, second, third].map(|records| {
        std::array::from_fn::<_, 5, _>(|i| {
            records.payloads[i].as_ref().map(|payload| {
                let mut hash = Sha256::new();
                hash.update(&records.keys[i]);
                hash.update(payload);
                (
                    records.versions[i],
                    records.keys[i].len(),
                    payload.len(),
                    format!("{:x}", hash.finalize()),
                )
            })
        })
    });
    let expected = [
        [
            Some((
                6,
                48,
                2069,
                "f4515a6a4ac3e63936cfdfb09941a0c3839c3464a7b9e6f7184808b96fe9b19c",
            )),
            Some((
                6,
                56,
                453,
                "077ccbade636a4a7108a2b0027f6dc0318753eec66dfbbee5593bd56b1f6400e",
            )),
            None,
            Some((
                1,
                33,
                391,
                "d4288cddf7173836e45fda9c3e4a3aeb97e203b34b367a46cfde9936c0603743",
            )),
            Some((
                2,
                33,
                2688,
                "66da24018e8fc99be05f3b85d0c1167a73795d668f8d3012b646eab412086dc8",
            )),
        ],
        [
            Some((
                6,
                48,
                2566,
                "2f5ca4bf25afbb7d8bfb3263a52257820559ae66ea3e2c4d3678bf69210a141a",
            )),
            Some((
                6,
                56,
                1158,
                "0028bf3f1c9cfdbd03d1d6f92977f7a548a20a974623be98160ed48bdfee4b0a",
            )),
            None,
            Some((
                1,
                33,
                391,
                "d4288cddf7173836e45fda9c3e4a3aeb97e203b34b367a46cfde9936c0603743",
            )),
            Some((
                2,
                33,
                2688,
                "97feea36867a17c09d091ea507c04e7431642e4b40f3970f4edbbfefd238ac32",
            )),
        ],
        [
            Some((
                6,
                48,
                2566,
                "16cb9ab559bfb5d0b3c68e88b645b549868a094a3c796e4124e4e03eb8938ab0",
            )),
            Some((
                6,
                56,
                1158,
                "d8909d34a398af4c05f4aa3e1c290b114bfdd02c2ef6ad260397ae9742542880",
            )),
            Some((
                6,
                48,
                11826,
                "93d65df57a836b78078b195701c0f2368595fb91a731c6ddd0ea526c566459fc",
            )),
            Some((
                1,
                33,
                687,
                "6ed596c4330a9b5da132e98d3abb3b94192a69f7682312ce8d7984d8f3101885",
            )),
            Some((
                2,
                33,
                1997,
                "60da174bc2c397edc3bd45c5c4d8e70a718a5d124c2f90a993b797e14eb68be8",
            )),
        ],
    ]
    .map(|records| {
        records.map(|record| {
            record.map(|(version, key_len, payload_len, digest)| {
                (version, key_len, payload_len, digest.to_owned())
            })
        })
    });
    assert_eq!(hashes, expected);
}
fn head(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    opened: &syndic_storage::DraftEditorCandidateSessionV1,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = storage
        .draft_editor_candidate_session(store, opened.draft_id(), opened.session_id())
        .unwrap()
    else {
        panic!()
    };
    head
}
