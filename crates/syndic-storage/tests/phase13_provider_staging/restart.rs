use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};

use super::*;

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub(super) struct TestHome(PathBuf);

impl TestHome {
    pub(super) fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-provider-staging-{name}-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn commit_first_batch(
    store: &HomeStore,
    storage: SyndicStorage,
    prepared: &PreparedProviderFrame,
) -> ProviderFrameStageBatch {
    let mut committed_batch = None;
    let stale_home_revision = store.home_revision().unwrap();
    let interrupted = stage_provider_frame(
        prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            if committed_batch.is_none() {
                committed_batch = Some(batch.clone());
                store.execute_current(storage.current_stage_provider_frame_batch(batch.clone()))
            } else {
                let mut command = HomeCommand::new(stale_home_revision);
                command
                    .add(storage.stage_provider_frame_batch(
                        storage.revision(store).unwrap(),
                        batch.clone(),
                    ))
                    .unwrap();
                store.execute(command)
            }
        },
    )
    .unwrap();
    match interrupted {
        ProviderFrameStageOutcome::NotCommitted { .. } => {}
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected restart interruption to be definitive, got indeterminate {failure:?}")
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure,
            ..
        } => panic!(
            "expected restart interruption, got committed outcome with receipt {receipt:?} and later failure {later_failure:?}"
        ),
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected restart interruption, got unchanged build {value:?}")
        }
    }
    committed_batch.unwrap()
}

pub(super) fn narrative_ahead_prepared() -> PreparedProviderFrame {
    let item = "narrative-ahead";
    let huge_note = "m".repeat(CONTENT_CHUNK_MAX_BYTES * (CONTENT_APPEND_MAX_CHUNKS + 3));
    let value = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline("short narrative"),
        phase: Some(ProviderMessagePhaseV1::Commentary),
        memory_citation: Some(ProviderMemoryCitationV1 {
            entries: vec![ProviderMemoryCitationEntryV1 {
                path: ProviderTextV1::inline("memory.md"),
                line_start: 1,
                line_end: 1,
                note: ProviderTextV1::inline(huge_note),
            }],
            thread_ids: Vec::new(),
        }),
    });
    prepare_first(
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            beryl_model::CasItemId::new(item).unwrap(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(10),
                item: value,
            },
        ),
        11,
    )
}

#[test]
fn content_ahead_partial_build_reopens_and_resumes() {
    let home = TestHome::new("content-ahead");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let text = "r".repeat(CONTENT_CHUNK_MAX_BYTES * (CONTENT_APPEND_MAX_CHUNKS + 3));
    let prepared = prepare_first(agent_start("content-ahead", text), 9);
    let narrative_seed = prepared.initial_build().staged_narrative().unwrap();
    match store.execute_current(storage.current_begin_provider_frame_build(&prepared)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(failure),
        } => panic!(
            "expected clean provider-frame build begin, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        CommandOutcome::NotCommitted { evidence } => panic!(
            "expected clean provider-frame build begin, got definitive non-commit {evidence:?}"
        ),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!(
                "expected clean provider-frame build begin, got indeterminate outcome {failure:?}"
            )
        }
    }

    let committed_batch = commit_first_batch(&store, storage, &prepared);
    let committed = committed_batch.next_build();
    assert_eq!(committed_batch.chunks().len(), CONTENT_APPEND_MAX_CHUNKS);
    assert!(committed_batch.narrative_spans().is_empty());
    assert!(committed.staged_chunk_count() > prepared.initial_build().staged_chunk_count());
    assert_eq!(committed.staged_narrative(), Some(narrative_seed));
    assert_eq!(committed.lifecycle(), ProviderItemBuildLifecycle::Staging);

    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let durable = reopened_storage
        .provider_item_build(
            &reopened,
            prepared.initial_build().item_id(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(&durable, committed);

    let final_build = match stage_provider_frame(
        &prepared,
        durable,
        &mut |batch: &ProviderFrameStageBatch| {
            reopened
                .execute_current(reopened_storage.current_stage_provider_frame_batch(batch.clone()))
        },
    )
    .unwrap()
    {
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
            value
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure: Some(failure),
            ..
        } => panic!(
            "expected clean resumed staging, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean resumed staging, got definitive non-commit {evidence:?}")
        }
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean resumed staging, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected resumed staging work, got unchanged build {value:?}")
        }
    };
    assert_eq!(final_build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    reopened.close().unwrap();

    let mut verified = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let verified_storage = SyndicStorage::register(&mut verified).unwrap();
    let reopened_build = verified_storage
        .provider_item_build(
            &verified,
            final_build.item_id(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(reopened_build, final_build);
    verified.close().unwrap();
}

#[test]
fn narrative_ahead_content_incomplete_partial_build_reopens_and_resumes() {
    let home = TestHome::new("narrative-ahead");
    let mut store = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let prepared = narrative_ahead_prepared();
    match store.execute_current(storage.current_begin_provider_frame_build(&prepared)) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(failure),
        } => panic!(
            "expected clean provider-frame build begin, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        CommandOutcome::NotCommitted { evidence } => panic!(
            "expected clean provider-frame build begin, got definitive non-commit {evidence:?}"
        ),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!(
                "expected clean provider-frame build begin, got indeterminate outcome {failure:?}"
            )
        }
    }

    let committed_batch = commit_first_batch(&store, storage, &prepared);
    let committed = committed_batch.next_build();
    assert_eq!(committed_batch.chunks().len(), CONTENT_APPEND_MAX_CHUNKS);
    assert_eq!(committed_batch.narrative_spans().len(), 1);
    let span = committed_batch.narrative_spans()[0];
    assert!(span.source_end() <= committed.staged_encoded_bytes());
    assert!(
        committed.staged_encoded_bytes() < prepared.target().content().summary().encoded_bytes()
    );
    assert_eq!(committed.staged_narrative(), prepared.target().narrative());
    assert_eq!(committed.lifecycle(), ProviderItemBuildLifecycle::Staging);

    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let durable = reopened_storage
        .provider_item_build(
            &reopened,
            committed.item_id(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(&durable, committed);

    let sealed = match stage_provider_frame(
        &prepared,
        durable,
        &mut |batch: &ProviderFrameStageBatch| {
            reopened
                .execute_current(reopened_storage.current_stage_provider_frame_batch(batch.clone()))
        },
    )
    .unwrap()
    {
        ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            let _receipt = receipt;
            value
        }
        ProviderFrameStageOutcome::Committed {
            receipt,
            later_failure: Some(failure),
            ..
        } => panic!(
            "expected clean resumed staging, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        ProviderFrameStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean resumed staging, got definitive non-commit {evidence:?}")
        }
        ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean resumed staging, got indeterminate outcome {failure:?}")
        }
        ProviderFrameStageOutcome::Unchanged { value } => {
            panic!("expected resumed staging work, got unchanged build {value:?}")
        }
    };
    assert_eq!(sealed.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    reopened.close().unwrap();
}
