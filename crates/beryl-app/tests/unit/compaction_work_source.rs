use super::*;
use crate::cas_projection::context_compaction::coordinator::custody::CompactionCustodyPool;
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    CasThreadId, RuntimeId, SyndicExecutionSnapshotId,
};
use syndic_storage::{
    CompactionAttemptNonce, CompactionOperationId, CompactionOperationNonce,
    CompactionOperationTarget,
};

fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
fn turn(seed: u8) -> SyndicTurnId {
    SyndicTurnId::from_bytes([seed; 16])
}
fn limits(count: usize) -> CompactionWorkPageLimits {
    CompactionWorkPageLimits::new(count, 65_536).unwrap()
}

fn prepare_operation(handle: &CompactionWorkHandle, seed: u8) {
    let operation =
        CompactionOperationId::new(thread(7), CompactionOperationNonce::from_bytes([seed; 16]));
    handle.operation(
        operation,
        CompactionAttemptNonce::from_bytes([seed; 16]),
        &CompactionOperationTarget::new(
            thread(7),
            operation.provider_turn_id(),
            SyndicExecutionSnapshotId::from_bytes([seed; 16]),
            BindingRevision::new(1).unwrap(),
            RuntimeId::from_bytes([7; 16]),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            CasThreadId::new("membership-target").unwrap(),
        ),
    );
}

#[test]
fn replacement_and_token_teardown_never_publish_two_memberships_for_one_thread() {
    let source = CompactionWorkSource::new(80);
    let mut current = source.begin(thread(7), None);
    prepare_operation(&current, 10);
    let mut command = current.command_observation();
    let mut membership = current.register_local(None);
    for seed in 11..75 {
        current.complete(crate::cas_projection::ContextCompactionOutcome::Succeeded);
        let next = source.begin(thread(7), None);
        prepare_operation(&next, seed);
        let next_command = next.command_observation();
        let before = source.revision().unwrap();
        let next_membership = next.register_local(Some(&current));
        let replaced = source.revision().unwrap();
        assert_eq!(replaced, before + 1);
        let page = source.page(replaced, None, limits(80)).unwrap();
        assert_eq!(page.records.len(), 2);
        assert_eq!(
            page.records
                .iter()
                .filter(|record| record.compaction.as_ref().unwrap().local_registered)
                .count(),
            1
        );
        assert_eq!(
            page.records
                .iter()
                .find(|record| record.compaction.as_ref().unwrap().local_registered)
                .unwrap()
                .serial,
            next.serial
        );
        drop(membership);
        assert_eq!(source.revision().unwrap(), replaced);
        drop(command);
        current = next;
        command = next_command;
        membership = next_membership;
    }
    drop(command);
    let retained = source
        .page(source.revision().unwrap(), None, limits(80))
        .unwrap();
    assert_eq!(retained.records.len(), 1);
    assert!(
        retained.records[0]
            .compaction
            .as_ref()
            .unwrap()
            .local_registered
    );
    assert!(
        retained.records[0]
            .compaction
            .as_ref()
            .unwrap()
            .command
            .is_none()
    );
    drop(membership);
    assert!(
        source
            .page(source.revision().unwrap(), None, limits(80))
            .unwrap()
            .records
            .is_empty()
    );
    current.complete(crate::cas_projection::ContextCompactionOutcome::Failed);
    assert!(
        source
            .page(source.revision().unwrap(), None, limits(80))
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn paged_compaction_facts_are_exact_stable_and_reject_changed_or_small_reads() {
    let source = CompactionWorkSource::new(80);
    let first = source.begin(thread(1), Some(turn(1)));
    let second = source.begin(thread(2), Some(turn(2)));
    let revision = source.revision().unwrap();
    let page = source.page(revision, None, limits(1)).unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].thread_id, thread(1));
    assert_eq!(page.after, Some(page.records[0].serial));
    let next = source.page(revision, page.after, limits(1)).unwrap();
    assert_eq!(next.records.len(), 1);
    assert_eq!(next.records[0].thread_id, thread(2));
    assert!(next.after.is_none());
    assert!(matches!(
        source.page(revision, None, CompactionWorkPageLimits::new(1, 1).unwrap()),
        Err(CompactionWorkError::ByteLimit)
    ));
    assert_eq!(source.revision().unwrap(), revision);
    let exact = source
        .page(
            revision,
            None,
            CompactionWorkPageLimits::new(10, page.bytes).unwrap(),
        )
        .unwrap();
    assert_eq!(exact.records, page.records);
    let observer = first.continuation_observation();
    observer.cancel();
    assert!(matches!(
        source.page(revision, page.after, limits(1)),
        Err(CompactionWorkError::StaleRevision)
    ));
    let cancelled = source.revision().unwrap();
    observer.cancel();
    assert_eq!(source.revision().unwrap(), cancelled);
    drop(observer);
    let final_page = source
        .page(source.revision().unwrap(), None, limits(10))
        .unwrap();
    assert_eq!(final_page.records.len(), 1);
    assert_eq!(page.records[0].continuation.as_ref().unwrap().pending, true);
    second.retire_slot();
    assert!(
        source
            .page(source.revision().unwrap(), None, limits(10))
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn observer_exhaustion_never_changes_custody_admission_or_release() {
    for serial in [false, true] {
        let pool = CompactionCustodyPool::new();
        if serial {
            pool.source.state.lock().unwrap().next_serial = Some(u64::MAX);
        } else {
            pool.source.state.lock().unwrap().revision = Some(u64::MAX);
        }
        let first = pool.reserve_continuation(thread(3), turn(3)).unwrap();
        let second = pool.reserve_continuation(thread(4), turn(4)).unwrap();
        assert_eq!(
            pool.source.revision(),
            Err(CompactionWorkError::RevisionUnavailable)
        );
        assert!(pool.source.state.lock().unwrap().records.is_empty());
        assert_eq!(pool.in_use(), 2);
        drop(first);
        drop(second);
        assert_eq!(pool.in_use(), 0);
        let later = pool.reserve_continuation(thread(5), turn(5)).unwrap();
        assert_eq!(pool.in_use(), 1);
        assert_eq!(
            pool.source.revision(),
            Err(CompactionWorkError::RevisionUnavailable)
        );
        drop(later);
        assert_eq!(pool.in_use(), 0);
    }
}

#[test]
fn poison_and_broken_owner_bound_never_publish_a_partial_inventory() {
    let source = CompactionWorkSource::new(80);
    for seed in 0..80 {
        source.begin(thread(seed), Some(turn(seed)));
    }
    assert_eq!(
        source
            .page(source.revision().unwrap(), None, limits(256))
            .unwrap()
            .records
            .len(),
        80
    );
    source.begin(thread(80), Some(turn(80)));
    assert_eq!(
        source.revision(),
        Err(CompactionWorkError::RevisionUnavailable)
    );
    assert!(source.state.lock().unwrap().records.is_empty());
    let pool = CompactionCustodyPool::new();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lock = pool.source.state.lock().unwrap();
            panic!("work source poison");
        }))
        .is_err()
    );
    let live = pool.reserve_continuation(thread(6), turn(6)).unwrap();
    assert_eq!(pool.source.revision(), Err(CompactionWorkError::Poisoned));
    assert_eq!(pool.in_use(), 1);
    drop(live);
    assert_eq!(pool.in_use(), 0);
}
