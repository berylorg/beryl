mod support;

use beryl_home_store::{CursorReadLimits, ReadError};
use beryl_model::{JobId, SyndicThreadId, ThreadRevision};
use beryl_state::{
    ArchiveBranchDiscussion, GeneratedTitle, SetGeneratedTitle, ThreadActivitySummary,
    ThreadArchiveState, ThreadMetadataKind, ThreadMetadataMutationError, TokenUsageBreakdown,
    TokenUsageSnapshot, UnixMillis, UpdateThreadActivity, UpdateTokenUsage,
};
use tempfile::tempdir;

use support::{binding, contributor_source, create_metadata, execute, open};

#[test]
fn immutable_binding_and_automatic_metadata_survive_reopen() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let thread_id = SyndicThreadId::from_bytes([7; 16]);
    create_metadata(
        &store,
        state,
        7,
        binding(1, 2, r"C:\Project"),
        ThreadMetadataKind::Ordinary,
    );
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    let title = GeneratedTitle::new(
        "Durable title",
        ThreadRevision::new(2).unwrap(),
        UnixMillis::new(100),
    )
    .unwrap();
    execute(
        &store,
        state.thread_metadata().set_generated_title(
            state.thread_metadata().revision(&store).unwrap(),
            SetGeneratedTitle::new(thread_id, metadata.revision(), title),
        ),
    )
    .unwrap();
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    execute(
        &store,
        state.thread_metadata().update_activity(
            state.thread_metadata().revision(&store).unwrap(),
            UpdateThreadActivity::new(
                thread_id,
                metadata.revision(),
                ThreadActivitySummary::new(ThreadRevision::new(3).unwrap(), UnixMillis::new(101)),
            ),
        ),
    )
    .unwrap();
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    let usage = TokenUsageSnapshot::new(
        TokenUsageBreakdown::new(10, 20, 30, 5, 50),
        TokenUsageBreakdown::new(100, 200, 300, 50, 500),
        Some(200_000),
        ThreadRevision::new(4).unwrap(),
        UnixMillis::new(102),
    )
    .unwrap();
    execute(
        &store,
        state.thread_metadata().update_token_usage(
            state.thread_metadata().revision(&store).unwrap(),
            UpdateTokenUsage::new(thread_id, metadata.revision(), usage),
        ),
    )
    .unwrap();
    store.close().unwrap();

    let (reopened, state) = open(directory.path());
    let metadata = state
        .thread_metadata()
        .metadata(&reopened, thread_id)
        .unwrap()
        .unwrap();
    assert_eq!(metadata.generated_title().unwrap().text(), "Durable title");
    assert_eq!(
        metadata.activity().unwrap().last_activity_at(),
        UnixMillis::new(101)
    );
    assert_eq!(metadata.token_usage().unwrap(), usage);
    assert_eq!(metadata.archive_state(), ThreadArchiveState::Ordinary);
    assert_eq!(metadata.revision().get(), 4);
}

#[test]
fn duplicate_creation_cannot_rebind_a_thread() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let thread_id = SyndicThreadId::from_bytes([7; 16]);
    create_metadata(
        &store,
        state,
        7,
        binding(1, 2, r"C:\One"),
        ThreadMetadataKind::Ordinary,
    );
    let error = execute(
        &store,
        state.thread_metadata().create(
            state.thread_metadata().revision(&store).unwrap(),
            beryl_state::CreateThreadMetadata::new(
                thread_id,
                binding(3, 4, r"C:\Two"),
                ThreadMetadataKind::Ordinary,
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<ThreadMetadataMutationError>(&error),
        Some(ThreadMetadataMutationError::ImmutableBindingMismatch { .. })
    ));
    assert_eq!(
        state
            .thread_metadata()
            .metadata(&store, thread_id)
            .unwrap()
            .unwrap()
            .binding(),
        &binding(1, 2, r"C:\One")
    );
}

#[test]
fn generated_title_is_one_way_and_stale_summary_updates_reject() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let thread_id = SyndicThreadId::from_bytes([8; 16]);
    create_metadata(
        &store,
        state,
        8,
        binding(1, 2, r"C:\Project"),
        ThreadMetadataKind::Ordinary,
    );
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    execute(
        &store,
        state.thread_metadata().set_generated_title(
            state.thread_metadata().revision(&store).unwrap(),
            SetGeneratedTitle::new(
                thread_id,
                metadata.revision(),
                GeneratedTitle::new(
                    "First title",
                    ThreadRevision::new(2).unwrap(),
                    UnixMillis::new(10),
                )
                .unwrap(),
            ),
        ),
    )
    .unwrap();
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    let second_title = execute(
        &store,
        state.thread_metadata().set_generated_title(
            state.thread_metadata().revision(&store).unwrap(),
            SetGeneratedTitle::new(
                thread_id,
                metadata.revision(),
                GeneratedTitle::new(
                    "Second title",
                    ThreadRevision::new(3).unwrap(),
                    UnixMillis::new(11),
                )
                .unwrap(),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<ThreadMetadataMutationError>(&second_title),
        Some(ThreadMetadataMutationError::GeneratedTitleAlreadySet)
    ));

    execute(
        &store,
        state.thread_metadata().update_activity(
            state.thread_metadata().revision(&store).unwrap(),
            UpdateThreadActivity::new(
                thread_id,
                metadata.revision(),
                ThreadActivitySummary::new(ThreadRevision::new(4).unwrap(), UnixMillis::new(20)),
            ),
        ),
    )
    .unwrap();
    let metadata = state
        .thread_metadata()
        .metadata(&store, thread_id)
        .unwrap()
        .unwrap();
    let stale = execute(
        &store,
        state.thread_metadata().update_activity(
            state.thread_metadata().revision(&store).unwrap(),
            UpdateThreadActivity::new(
                thread_id,
                metadata.revision(),
                ThreadActivitySummary::new(ThreadRevision::new(4).unwrap(), UnixMillis::new(21)),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<ThreadMetadataMutationError>(&stale),
        Some(ThreadMetadataMutationError::SourceRevisionNotLater { .. })
    ));
}

#[test]
fn only_open_branch_discussion_archives_and_never_unarchives() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let ordinary_id = SyndicThreadId::from_bytes([1; 16]);
    let branch_id = SyndicThreadId::from_bytes([2; 16]);
    create_metadata(
        &store,
        state,
        1,
        binding(1, 2, r"C:\Project"),
        ThreadMetadataKind::Ordinary,
    );
    create_metadata(
        &store,
        state,
        2,
        binding(1, 2, r"C:\Project"),
        ThreadMetadataKind::BranchDiscussion,
    );

    let ordinary = state
        .thread_metadata()
        .metadata(&store, ordinary_id)
        .unwrap()
        .unwrap();
    let rejected = execute(
        &store,
        state.thread_metadata().archive_branch_discussion(
            state.thread_metadata().revision(&store).unwrap(),
            ArchiveBranchDiscussion::new(
                ordinary_id,
                ordinary.revision(),
                JobId::from_bytes([9; 16]),
                UnixMillis::new(30),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<ThreadMetadataMutationError>(&rejected),
        Some(ThreadMetadataMutationError::NotOpenBranchDiscussion)
    ));

    let branch = state
        .thread_metadata()
        .metadata(&store, branch_id)
        .unwrap()
        .unwrap();
    execute(
        &store,
        state.thread_metadata().archive_branch_discussion(
            state.thread_metadata().revision(&store).unwrap(),
            ArchiveBranchDiscussion::new(
                branch_id,
                branch.revision(),
                JobId::from_bytes([9; 16]),
                UnixMillis::new(31),
            ),
        ),
    )
    .unwrap();
    let archived = state
        .thread_metadata()
        .metadata(&store, branch_id)
        .unwrap()
        .unwrap();
    assert!(archived.archive_state().is_archived());

    let again = execute(
        &store,
        state.thread_metadata().archive_branch_discussion(
            state.thread_metadata().revision(&store).unwrap(),
            ArchiveBranchDiscussion::new(
                branch_id,
                archived.revision(),
                JobId::from_bytes([10; 16]),
                UnixMillis::new(32),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<ThreadMetadataMutationError>(&again),
        Some(ThreadMetadataMutationError::NotOpenBranchDiscussion)
    ));
}

#[test]
fn metadata_listing_is_complete_only_within_explicit_bounds() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    for byte in 1..=3 {
        create_metadata(
            &store,
            state,
            byte,
            binding(1, 2, r"C:\Project"),
            ThreadMetadataKind::Ordinary,
        );
    }

    let page = state
        .thread_metadata()
        .list(&store, None, CursorReadLimits::new(1, 1_000_000).unwrap())
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(page.has_more());
    assert!(page.stored_bytes() > 0);

    let error = state
        .thread_metadata()
        .list(&store, None, CursorReadLimits::new(8, 1).unwrap())
        .unwrap_err();
    assert!(matches!(error, ReadError::BoundExceeded { .. }));
}
