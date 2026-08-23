#[cfg(feature = "test-faults")]
mod support;

#[cfg(feature = "test-faults")]
#[path = "phase74_thread_properties/archive.rs"]
mod archive;
#[cfg(feature = "test-faults")]
#[path = "phase74_thread_properties/catalog_fences.rs"]
mod catalog_fences;
#[cfg(feature = "test-faults")]
#[path = "phase74_thread_properties/core.rs"]
mod core;
#[path = "phase74_thread_properties/title_usage.rs"]
mod title_usage;

#[cfg(not(feature = "test-faults"))]
mod production {
    use std::sync::atomic::{AtomicU64, Ordering};

    use beryl_home_store::{
        CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    };
    use beryl_model::{SyndicDraftId, SyndicThreadId};
    use syndic_storage::{
        CreateThread, DraftEditHistoryPolicyV1, SyndicPointReadLimit, SyndicStorage,
        ThreadAttributesRevision, ThreadCreationStatus, ThreadUsageRecord,
    };

    static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

    fn point_limit() -> SyndicPointReadLimit {
        SyndicPointReadLimit::new(1_000_000).unwrap()
    }

    #[test]
    fn ordinary_creation_publishes_intrinsic_properties_exactly() {
        let path = std::env::temp_dir().join(format!(
            "beryl-phase74-creation-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let result = (|| {
            let mut store =
                HomeStore::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT)).unwrap();
            let storage = SyndicStorage::register(&mut store).unwrap();
            let thread = SyndicThreadId::from_bytes([74; 16]);
            let draft = SyndicDraftId::from_bytes([75; 16]);
            let creation = CreateThread::ordinary(
                thread,
                draft,
                crate::production_execution(),
                syndic_storage::SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            );
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.create_thread(storage.revision(&store).unwrap(), creation.clone()))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            assert_eq!(
                storage
                    .thread_creation_status(&store, &creation, point_limit())
                    .unwrap(),
                ThreadCreationStatus::Exact
            );
            assert_eq!(
                storage
                    .thread_attributes(&store, thread, point_limit())
                    .unwrap()
                    .unwrap()
                    .revision(),
                ThreadAttributesRevision::FIRST
            );
            assert_eq!(
                storage
                    .thread_usage(&store, thread, point_limit())
                    .unwrap()
                    .unwrap(),
                ThreadUsageRecord::empty(thread)
            );
            assert!(
                storage
                    .thread_catalog_summary(&store, thread, point_limit())
                    .unwrap()
                    .is_some()
            );
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .unwrap();
            store.close().unwrap();
        })();
        let _ = std::fs::remove_dir_all(&path);
        result
    }
}

#[cfg(not(feature = "test-faults"))]
fn production_execution() -> beryl_model::ExecutionBinding {
    beryl_model::ExecutionBinding::new(
        beryl_model::RuntimeId::from_bytes([76; 16]),
        beryl_model::RootId::from_bytes([77; 16]),
        beryl_model::RuntimeNativePath::from_admitted(
            beryl_model::RuntimeMode::host(),
            beryl_model::PathFlavor::Windows,
            r"C:\\Work\\Beryl",
        )
        .unwrap(),
    )
}
