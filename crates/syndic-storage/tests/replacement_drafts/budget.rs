use super::*;

#[test]
fn immutable_baseline_and_session_fork_both_fit_the_actual_head_budget() {
    let payload = ComposerPayload::new(vec![ComposerAtom::text("baseline").unwrap()]).unwrap();
    let measure = Fixture::new("baseline-charge", payload.clone(), vec![], 65_536);
    let p = measure.prepare();
    begin(&measure, &p);
    advance_all(&measure, &p);
    let start = measure
        .storage
        .prepare_replacement_draft_start(&measure.store, &p, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let DraftPieceSettlementClosureV1::SealedImport(closure) = start.settlement().closure() else {
        panic!()
    };
    let baseline = closure.baseline().retained_encoded_bytes();
    let fork = closure.session_fork().retained_encoded_bytes();
    assert!(baseline > 0 && fork > 0);
    assert_ne!(baseline, fork);
    let exact = baseline.max(fork);
    for budget in [exact - 1, exact] {
        let f = Fixture::new("baseline-budget", payload.clone(), vec![], budget);
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let before = f.head();
        let result = f.storage.prepare_replacement_draft_start(
            &f.store,
            &p,
            SyndicTimestamp::from_unix_millis(3),
        );
        if budget < exact {
            assert!(matches!(
                result,
                Err(DraftReplacementErrorV1::CapacityUnavailable)
            ));
            assert_eq!(f.head(), before);
            assert_eq!(
                ordinary::current(&f.storage, &f.store, f.thread)
                    .draft()
                    .submission_intent(),
                DraftSubmissionIntent::Ordinary
            );
        } else {
            let prepared = result.unwrap();
            committed(f.store.execute(start_command(&f, &prepared)));
            assert_eq!(f.head().newest_root(), prepared.selector().root());
        }
    }
}
