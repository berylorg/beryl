#![cfg(feature = "test-faults")]

#[path = "phase13_provider_observation/depth.rs"]
mod depth;
#[path = "phase13_provider_observation/identity.rs"]
mod identity;
#[path = "phase13_provider_observation/identity_exclusions.rs"]
mod identity_exclusions;
#[path = "phase13_provider_observation/lifecycle.rs"]
mod lifecycle;
#[path = "phase13_provider_observation/matrix.rs"]
mod matrix;
#[path = "phase13_provider_observation/matrix_restart.rs"]
mod matrix_restart;
#[path = "phase13_provider_observation/nested.rs"]
mod nested;
#[path = "phase13_provider_observation/reasoning_privacy.rs"]
mod reasoning_privacy;
#[path = "phase13_provider_observation/restart_validation.rs"]
mod restart_validation;
#[path = "phase13_provider_observation/schemas.rs"]
mod schemas;
mod support;
#[path = "phase13_provider_observation/validation.rs"]
mod validation;

use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::{CasThreadId, CasTurnId, ProviderObservationId};
use syndic_storage::test_faults::{PhysicalFamily, ProviderObservationCorruption};
use syndic_storage::*;

use support::{TestHome, open};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn commit_callback(
    store: &HomeStore,
    storage: SyndicStorage,
) -> impl FnMut(&ProviderObservationStageBatch) -> CommandOutcome + '_ {
    move |batch| store.execute_current(storage.current_stage_provider_observation_batch(batch.clone()))
}

fn scalar<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderScalar,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    stager.control(
        ProviderObservationControl::Scalar {
            context: ProviderValueContext::Field(field),
            value,
        },
        callback,
    )
}

fn text<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    pieces: &[&[u8]],
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    let context = ProviderValueContext::Field(field);
    stager.control(ProviderObservationControl::BeginField(context), callback)?;
    for piece in pieces {
        stager.fragment(
            ProviderObservationStagingBytes::new(context, piece)?,
            callback,
        )?;
    }
    stager.control(ProviderObservationControl::EndField(context), callback)
}

fn common_item<C: ProviderObservationStageCallback>(
    stager: &mut ProviderObservationStager,
    callback: &mut C,
) -> Result<(), ProviderObservationStagingError> {
    scalar(
        stager,
        ProviderField::LifecycleObservedAt,
        ProviderScalar::Unsigned(42),
        callback,
    )?;
    text(stager, ProviderField::ItemId, &[b"provider-item"], callback)
}

fn begin_agent<C: ProviderObservationStageCallback>(
    identity: ProviderObservationId,
    callback: &mut C,
) -> Result<ProviderObservationStager, ProviderObservationStagingError> {
    let mut stager = ProviderObservationStager::begin(
        identity,
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Started,
            kind: ProviderObservationItemKind::AgentMessage,
        },
        callback,
    )?;
    common_item(&mut stager, callback)?;
    Ok(stager)
}

fn route() -> ProviderObservationRoute {
    ProviderObservationRoute::new(
        CasThreadId::new("provider-observation-thread").unwrap(),
        CasTurnId::new("provider-observation-turn").unwrap(),
    )
}

#[test]
fn arbitrary_utf8_fragmentation_is_canonical_and_cursor_has_exact_eof() {
    let home = TestHome::new("provider-observation-canonical");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let bytes = "héllo \u{1f980}".as_bytes();

    let first = {
        let mut callback = commit_callback(&store, storage);
        let mut stager =
            begin_agent(ProviderObservationId::from_bytes([1; 16]), &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            &[&bytes[..2], &bytes[2..5], &bytes[5..]],
            &mut callback,
        )
        .unwrap();
        stager.seal(&mut callback).unwrap()
    };
    let second = {
        let mut callback = commit_callback(&store, storage);
        let mut stager =
            begin_agent(ProviderObservationId::from_bytes([2; 16]), &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            &[bytes],
            &mut callback,
        )
        .unwrap();
        stager.seal(&mut callback).unwrap()
    };
    assert!(first.canonical_eq(&second));

    let build = storage
        .provider_observation_build(&store, first.identity(), limit())
        .unwrap()
        .unwrap()
        .clone();
    let mut cursor = storage
        .open_provider_observation_cursor(&store, first.bind(route(), route()).unwrap(), limit())
        .unwrap();
    let mut pages = 0_u64;
    while let Some(page) = storage
        .read_provider_observation_cursor_page(&store, &mut cursor, limit())
        .unwrap()
    {
        pages += 1;
        assert_eq!(page.ordinal(), pages);
        assert!(page.stored_bytes() < 1_000_000);
    }
    assert_eq!(pages, build.chunk_count());
    assert!(matches!(
        storage.read_provider_observation_cursor_page(&store, &mut cursor, limit()),
        Err(ProviderObservationCursorError::CursorTerminal)
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn partial_build_reopens_resumes_and_exact_batches_reconcile() {
    let home = TestHome::new("provider-observation-resume");
    let identity = ProviderObservationId::from_bytes([3; 16]);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    {
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_agent(identity, &mut callback).unwrap();
        let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
        stager
            .control(
                ProviderObservationControl::BeginField(context),
                &mut callback,
            )
            .unwrap();
        stager
            .fragment(
                ProviderObservationStagingBytes::new(context, b"restart ").unwrap(),
                &mut callback,
            )
            .unwrap();
        stager.abandon();
    }
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let mut stager = storage
        .resume_provider_observation(&reopened, identity, limit())
        .unwrap()
        .unwrap();
    let mut callback = |batch: &ProviderObservationStageBatch| -> CommandOutcome {
        let outcome = reopened
            .execute_current(storage.current_stage_provider_observation_batch(batch.clone()));
        if !matches!(
            &outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ) {
            return outcome;
        }
        let current = storage
            .provider_observation_build(&reopened, identity, limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            batch.classify_current(Some(&current)),
            ProviderObservationStageBatchState::Next
        );
        outcome
    };
    let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
    stager
        .fragment(
            ProviderObservationStagingBytes::new(context, b"complete").unwrap(),
            &mut callback,
        )
        .unwrap();
    stager
        .control(ProviderObservationControl::EndField(context), &mut callback)
        .unwrap();
    let sealed = stager.seal(&mut callback).unwrap();
    assert_eq!(sealed.identity(), identity);
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn identity_collision_route_mismatch_and_abandonment_are_explicit() {
    let home = TestHome::new("provider-observation-authority");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let identity = ProviderObservationId::from_bytes([70; 16]);
    let sealed = {
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_agent(identity, &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            &[b"authority"],
            &mut callback,
        )
        .unwrap();
        stager.seal(&mut callback).unwrap()
    };
    {
        let mut callback = commit_callback(&store, storage);
        assert!(
            ProviderObservationStager::begin(
                identity,
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Started,
                    kind: ProviderObservationItemKind::AgentMessage,
                },
                &mut callback,
            )
            .is_err()
        );
    }
    let trailing = ProviderObservationRoute::new(
        CasThreadId::new("provider-observation-thread").unwrap(),
        CasTurnId::new("other-turn").unwrap(),
    );
    let error = match sealed.bind(route(), trailing.clone()) {
        Ok(_) => panic!("route mismatch unexpectedly bound"),
        Err(error) => error,
    };
    assert_eq!(error.admitted(), &route());
    assert_eq!(error.trailing(), &trailing);
    let persisted = storage
        .reopen_provider_observation(&store, identity, limit())
        .unwrap()
        .unwrap();
    persisted.abandon();
    assert_eq!(
        storage
            .provider_observation_build(&store, identity, limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ProviderObservationBuildLifecycle::Sealed
    );
    store.close().unwrap();
}

#[test]
fn large_observation_stays_bounded_and_missing_chunk_is_rejected() {
    let home = TestHome::new("provider-observation-large-fault");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let identity = ProviderObservationId::from_bytes([71; 16]);
    let sealed = {
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_agent(identity, &mut callback).unwrap();
        let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
        stager
            .control(
                ProviderObservationControl::BeginField(context),
                &mut callback,
            )
            .unwrap();
        let chunk = vec![b'x'; PROVIDER_OBSERVATION_CHUNK_MAX_BYTES];
        for _ in 0..20 {
            stager
                .fragment(
                    ProviderObservationStagingBytes::new(context, &chunk).unwrap(),
                    &mut callback,
                )
                .unwrap();
        }
        stager
            .control(ProviderObservationControl::EndField(context), &mut callback)
            .unwrap();
        stager.seal(&mut callback).unwrap()
    };
    let build = storage
        .provider_observation_build(&store, sealed.identity(), limit())
        .unwrap()
        .unwrap()
        .clone();
    assert!(build.canonical_bytes() > 1_000_000);
    store.validate_registered_domains().unwrap();
    sealed.abandon();
    match store.execute_current(
        storage
            .current_corrupt_provider_observation(
                &build,
                ProviderObservationCorruption::MissingChunk { ordinal: 1 },
            )
            .unwrap(),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed provider-observation corruption, got {outcome:?}"),
    }
    let error = store.validate_registered_domains().unwrap_err();
    assert!(error.to_string().contains("missing chunk"));
    store.close().unwrap();
}

#[test]
fn corrupted_build_digest_is_rejected_and_new_families_are_registered() {
    let home = TestHome::new("provider-observation-digest-fault");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let identity = ProviderObservationId::from_bytes([72; 16]);
    {
        let mut callback = commit_callback(&store, storage);
        let mut stager = begin_agent(identity, &mut callback).unwrap();
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            &[b"digest"],
            &mut callback,
        )
        .unwrap();
        stager.seal(&mut callback).unwrap().abandon();
    }
    let build = storage
        .provider_observation_build(&store, identity, limit())
        .unwrap()
        .unwrap()
        .clone();
    match store.execute_current(
        storage
            .current_corrupt_provider_observation(&build, ProviderObservationCorruption::BuildDigest)
                .unwrap(),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed provider-observation corruption, got {outcome:?}"),
    }
    let error = store.validate_registered_domains().unwrap_err();
    assert!(error.to_string().contains("disagrees with chunk replay"));
    assert_eq!(PhysicalFamily::ALL.len(), 61);
    assert!(PhysicalFamily::ALL.contains(&PhysicalFamily::ProviderObservationBuilds));
    assert!(PhysicalFamily::ALL.contains(&PhysicalFamily::ProviderObservationChunks));
    store.close().unwrap();
}
