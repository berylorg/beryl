use super::runtime::{fixture, source, submit, support, thread_id};
use beryl_app::{
    cas_projection::{ProcessScheduledExecutionProvider, RuntimeInterestKind},
    generated_media_qualification::{self as qualification, Coordinator},
};
use beryl_home_store::test_faults::FaultPoint;
use beryl_model::{AssetId, GeneratedMediaUnavailableReason as Reason};
use beryl_state::{GeneratedMediaLifecycle, GeneratedMediaTarget};
use sha2::{Digest, Sha256};
use std::{cell::Cell, fs, rc::Rc, time::Duration};
use syndic_storage::{
    GeneratedMediaResourceDisposition as Disposition, ResourceBacking, SyndicPointReadLimit,
};

fn with_domain(run: impl FnOnce(&mut gpui::App) + 'static) {
    let finished = Rc::new(Cell::new(false));
    let done = finished.clone();
    gpui::Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            run(app);
            done.set(true);
            app.quit();
        });
    assert!(finished.get());
}

fn start(app: &mut gpui::App, fixture: &support::Fixture, pause: bool) -> Coordinator {
    let domain = app
        .image_resource_domain(qualification::domain_configuration())
        .unwrap();
    let live = fixture.service().live_home_command().unwrap();
    Coordinator::start(
        live.home().service_reference(),
        fixture.storage.clone(),
        &fixture.state,
        fixture.service(),
        domain,
        pause,
    )
    .unwrap()
}

fn png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 3, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[127; 24]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn png_asset() -> AssetId {
    let bytes = png();
    AssetId::sha256_v1(
        Sha256::digest(&bytes).into(),
        std::num::NonZeroU64::new(bytes.len() as u64).unwrap(),
    )
}

fn target(proof: &syndic_storage::GeneratedMediaSourceProof) -> GeneratedMediaTarget {
    GeneratedMediaTarget::new(
        proof.resource_id(),
        proof.thread_id(),
        proof.turn_id(),
        proof.item_id(),
        proof.source_fingerprint(),
    )
}

fn backing(fixture: &support::Fixture, target: GeneratedMediaTarget) -> ResourceBacking {
    let live = fixture.service().live_home_command().unwrap();
    fixture
        .storage
        .resource(
            live.home(),
            target.resource(),
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .backing()
}

fn published(fixture: &support::Fixture, target: GeneratedMediaTarget) -> Option<AssetId> {
    let live = fixture.service().live_home_command().unwrap();
    let admission = match fixture
        .state
        .assets()
        .generated_media_admission(live.home(), target)
    {
        Ok(admission) => admission?,
        Err(beryl_state::GeneratedMediaError::ConcurrentChange) => return None,
        Err(error) => panic!("generated admission failed: {error:?}"),
    };
    if admission.lifecycle() != GeneratedMediaLifecycle::Published {
        return None;
    }
    let asset = admission.facts().asset();
    assert_eq!(
        backing(fixture, target),
        ResourceBacking::GeneratedMedia(Disposition::Asset(asset))
    );
    let sweep = match fixture.storage.begin_generated_media_sweep(live.home()) {
        Ok(sweep) => sweep,
        Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return None,
        Err(error) => panic!("generated pending sweep failed: {error:?}"),
    };
    let page = match fixture
        .storage
        .pending_generated_media_page(live.home(), &sweep, 8, 65_536)
    {
        Ok(page) => page,
        Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return None,
        Err(error) => panic!("generated pending page failed: {error:?}"),
    };
    assert!(
        page.entries()
            .iter()
            .all(|entry| entry.resource_id() != target.resource())
    );
    Some(asset)
}

fn stop(coordinator: &mut Coordinator) {
    coordinator.begin_retirement();
    coordinator.join_retirement().unwrap();
    let snapshot = coordinator.snapshot();
    assert_eq!(
        (
            snapshot.queued,
            snapshot.active,
            snapshot.workers,
            snapshot.path_slots,
            snapshot.verification_slots
        ),
        (0, 0, 0, 0, 0)
    );
    support::wait_until(|| coordinator.retire_process());
}

fn presentation(
    fixture: &support::Fixture,
    selected: beryl_model::SyndicThreadId,
    resource: beryl_model::SyndicResourceId,
    previous: Option<&qualification::GeneratedPresentation>,
) -> qualification::GeneratedPresentation {
    let mut prepared = None;
    wait_generated(
        "prepare generated presentation",
        fixture,
        selected,
        resource,
        || {
            let live = fixture.service().live_home_command().unwrap();
            prepared = qualification::inspect_generated_presentation(
                live.home().service_reference(),
                fixture.storage.clone(),
                fixture.service(),
                selected,
                resource,
                previous,
            )
            .unwrap();
            prepared.is_some()
        },
    );
    let prepared = prepared.unwrap();
    assert_eq!(prepared.view_thread, selected);
    assert_eq!(prepared.source_thread, thread_id());
    assert_eq!(prepared.resource_id, resource);
    assert!(!prepared.has_authored_source);
    prepared
}

#[track_caller]
fn wait_generated(
    stage: &str,
    fixture: &support::Fixture,
    selected: beryl_model::SyndicThreadId,
    resource: beryl_model::SyndicResourceId,
    mut condition: impl FnMut() -> bool,
) {
    let deadline = std::time::Instant::now() + support::TIMEOUT;
    while !condition() {
        if std::time::Instant::now() >= deadline {
            let live = fixture.service().live_home_command().unwrap();
            let home = live.home();
            let point = SyndicPointReadLimit::new(65_536).unwrap();
            let proof = fixture.storage.generated_media_source(home, resource);
            let details = proof.as_ref().ok().and_then(|proof| proof.as_ref()).map(|proof| {
                let head = fixture.storage.item_projection_head(home, proof.item_id(), point);
                let set = head.as_ref().ok().and_then(|head| head.as_ref()).map(|head| {
                    fixture.storage.item_projection_set(home, proof.item_id(), head.generation(), point)
                });
                format!("item={:?}, resource={:?}, projection_head={head:?}, set={set:?}, turn_state={:?}",
                    proof.item(), proof.resource(), fixture.storage.turn_state(home, proof.turn_id(), point))
            });
            panic!(
                "generated stage {stage} timed out: selected={selected:?}, resource={resource:?}, thread={:?}, head={:?}, details={details:?}, workers={:?}, scheduler={:?}, failure={:?}",
                fixture.storage.thread(home, selected, point),
                fixture.storage.transcript_view_head(home, selected, point),
                fixture.service().worker_pool_diagnostics(),
                fixture.service().accepted_input_scheduler_diagnostics(),
                fixture.service().persistent_failure_cut_snapshot()
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn inherited_branch(
    fixture: &support::Fixture,
    resource: beryl_model::SyndicResourceId,
) -> beryl_model::SyndicThreadId {
    let mut tail = None;
    wait_generated(
        "finalized parent tail",
        fixture,
        thread_id(),
        resource,
        || {
            let live = fixture.service().live_home_command().unwrap();
            tail = match fixture.storage.thread_tail(
                live.home(),
                thread_id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            ) {
                Ok(tail) => tail,
                Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                Err(error) => panic!("generated parent tail failed: {error:?}"),
            };
            tail.as_ref().is_some_and(|tail| tail.complete())
        },
    );
    let tail = tail.unwrap();
    let child = beryl_model::SyndicThreadId::from_bytes([242; 16]);
    let creation = syndic_storage::CreateThread::from_tail(
        child,
        beryl_model::SyndicDraftId::from_bytes([243; 16]),
        tail.last_activity_at(),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap();
    let live = fixture.service().live_home_command().unwrap();
    let mut command = beryl_home_store::HomeCommand::new(live.home().home_revision().unwrap());
    command
        .add(
            fixture
                .storage
                .create_thread(fixture.storage.revision(live.home()).unwrap(), creation),
        )
        .unwrap();
    assert!(matches!(
        live.home().execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    child
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_coordinator_publishes_actual_original_then_sleeps_and_drains() {
    with_domain(|app| {
        let (mut fixture, sessions, _attention) = fixture();
        fs::write(fixture.root(1).join("fixture-mode"), "execution-generated").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submit(&fixture, thread_id());
        let proof = source(&fixture);
        fs::write(proof.saved_path().unwrap().unwrap(), png()).unwrap();
        let target = target(&proof);
        let mut coordinator = start(app, &fixture, false);
        support::wait_until(|| published(&fixture, target) == Some(png_asset()));
        support::wait_until(|| coordinator.snapshot().idle);
        let snapshot = coordinator.snapshot();
        assert_eq!(
            (
                snapshot.queued,
                snapshot.active,
                snapshot.workers,
                snapshot.path_slots,
                snapshot.verification_slots
            ),
            (0, 0, 2, 0, 0)
        );
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(coordinator.snapshot(), snapshot);
        fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
        support::wait_until(|| {
            sessions.diagnostics().checked_out == 0 && sessions.diagnostics().available == 1
        });
        stop(&mut coordinator);
        drop(view);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_coordinator_fresh_service_publishes_durable_prepared_without_original() {
    with_domain(|app| {
        let (mut fixture, sessions, _attention) = fixture();
        fs::write(fixture.root(1).join("fixture-mode"), "execution-generated").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submit(&fixture, thread_id());
        let proof = source(&fixture);
        let path = proof.saved_path().unwrap().unwrap().to_owned();
        fs::write(&path, png()).unwrap();
        let target = target(&proof);
        let mut coordinator = start(app, &fixture, true);
        support::wait_until(|| coordinator.preparation_reached());
        let live = fixture.service().live_home_command().unwrap();
        let prepared = fixture
            .state
            .assets()
            .generated_media_admission(live.home(), target)
            .unwrap()
            .unwrap();
        assert_eq!(prepared.lifecycle(), GeneratedMediaLifecycle::Prepared);
        assert_eq!(prepared.facts().asset(), png_asset());
        assert_eq!(
            backing(&fixture, target),
            ResourceBacking::GeneratedMedia(Disposition::PendingAsset)
        );
        assert_eq!(
            (
                coordinator.snapshot().path_slots,
                coordinator.snapshot().verification_slots
            ),
            (0, 0)
        );
        drop(live);
        stop(&mut coordinator);
        fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
        support::wait_until(|| {
            sessions.diagnostics().checked_out == 0 && sessions.diagnostics().available == 1
        });
        drop(view);
        let (provider, _replacement_sessions) = ProcessScheduledExecutionProvider::new();
        fixture.reopen(Box::new(provider), 8);
        fs::remove_file(&path).unwrap();
        let mut replacement = start(app, &fixture, false);
        support::wait_until(|| published(&fixture, target) == Some(prepared.facts().asset()));
        stop(&mut replacement);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_coordinator_cancel_retains_original_stages_until_actual_source_close() {
    with_domain(|app| {
        let (mut fixture, sessions, _attention) = fixture();
        fs::write(fixture.root(1).join("fixture-mode"), "execution-generated").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submit(&fixture, thread_id());
        let proof = source(&fixture);
        fs::write(proof.saved_path().unwrap().unwrap(), png()).unwrap();
        let blocked = fixture.faults.block_next(FaultPoint::BeforeSidecarWrite);
        let mut coordinator = start(app, &fixture, false);
        assert!(blocked.wait_until_reached(support::TIMEOUT));
        assert_eq!(
            (
                coordinator.snapshot().active,
                coordinator.snapshot().path_slots,
                coordinator.snapshot().verification_slots
            ),
            (1, 1, 1)
        );
        coordinator.begin_retirement();
        assert_eq!(
            (
                coordinator.snapshot().active,
                coordinator.snapshot().path_slots,
                coordinator.snapshot().verification_slots
            ),
            (1, 1, 1)
        );
        blocked.release();
        stop(&mut coordinator);
        assert_eq!(
            backing(&fixture, target(&proof)),
            ResourceBacking::GeneratedMedia(Disposition::PendingAsset)
        );
        fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
        support::wait_until(|| {
            sessions.diagnostics().checked_out == 0 && sessions.diagnostics().available == 1
        });
        drop(view);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_coordinator_replacement_terminalizes_unfinished_original_as_lost() {
    with_domain(|app| {
        let (mut fixture, sessions, _attention) = fixture();
        fs::write(
            fixture.root(1).join("fixture-mode"),
            "execution-generated-awaiting",
        )
        .unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submit(&fixture, thread_id());
        let mut original = None;
        support::wait_until(|| {
            let live = fixture.service().live_home_command().unwrap();
            let sweep = match fixture.storage.begin_generated_media_sweep(live.home()) {
                Ok(sweep) => sweep,
                Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                Err(error) => panic!("unfinished sweep failed: {error:?}"),
            };
            let page =
                match fixture
                    .storage
                    .pending_generated_media_page(live.home(), &sweep, 8, 65_536)
                {
                    Ok(page) => page,
                    Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                    Err(error) => panic!("unfinished page failed: {error:?}"),
                };
            if let Some(entry) = page.entries().first() {
                original = match fixture
                    .storage
                    .generated_media_source(live.home(), entry.resource_id())
                {
                    Ok(proof) => proof,
                    Err(syndic_storage::SyndicReadError::ConcurrentChange { .. }) => return false,
                    Err(error) => panic!("unfinished source proof failed: {error:?}"),
                };
            }
            original.is_some()
        });
        let proof = original.unwrap();
        assert!(proof.saved_path().is_none());
        let target = target(&proof);
        let pending = presentation(&fixture, thread_id(), target.resource(), None);
        assert_eq!(pending.lifecycle, Disposition::PendingAsset);
        let mut original = start(app, &fixture, false);
        support::wait_until(|| original.snapshot().idle);
        assert_eq!(
            backing(&fixture, target),
            ResourceBacking::GeneratedMedia(Disposition::PendingAsset)
        );
        assert_eq!(original.snapshot().active, 0);
        stop(&mut original);
        fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
        support::wait_until(|| {
            sessions.diagnostics().checked_out == 0 && sessions.diagnostics().available == 1
        });
        drop(view);
        let (provider, _replacement_sessions) = ProcessScheduledExecutionProvider::new();
        fixture.reopen(Box::new(provider), 8);
        let mut replacement = start(app, &fixture, false);
        support::wait_until(|| {
            backing(&fixture, target)
                == ResourceBacking::GeneratedMedia(Disposition::Unavailable(
                    Reason::LostSourceAuthority,
                ))
        });
        let unavailable = presentation(&fixture, thread_id(), target.resource(), Some(&pending));
        assert_eq!(
            unavailable.lifecycle,
            Disposition::Unavailable(Reason::LostSourceAuthority)
        );
        assert_eq!(unavailable.projection_id, pending.projection_id);
        assert_eq!(unavailable.same_presentation, Some(false));
        stop(&mut replacement);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
    });
}

#[test]
#[ignore = "requires an announced native GPUI qualification run; opens no window"]
fn generated_coordinator_inherited_branch_uses_prepared_selected_view_for_original_and_rendition() {
    with_domain(|app| {
        let (mut fixture, sessions, _attention) = fixture();
        fs::write(fixture.root(1).join("fixture-mode"), "execution-generated").unwrap();
        let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
        support::ready(&view);
        submit(&fixture, thread_id());
        let proof = source(&fixture);
        fs::write(proof.saved_path().unwrap().unwrap(), png()).unwrap();
        let target = target(&proof);
        let pending = presentation(&fixture, thread_id(), target.resource(), None);
        assert_eq!(pending.lifecycle, Disposition::PendingAsset);
        fs::write(fixture.root(1).join("execution-release-0"), "complete").unwrap();
        wait_generated(
            "provider session return",
            &fixture,
            thread_id(),
            target.resource(),
            || sessions.diagnostics().checked_out == 0 && sessions.diagnostics().available == 1,
        );
        let frozen_pending = presentation(&fixture, thread_id(), target.resource(), Some(&pending));
        assert_eq!(frozen_pending.projection_id, pending.projection_id);
        assert_eq!(frozen_pending.lifecycle, Disposition::PendingAsset);
        let mut coordinator = None;
        wait_generated(
            "prepare pending stale election",
            &fixture,
            thread_id(),
            target.resource(),
            || {
                let live = fixture.service().live_home_command().unwrap();
                let mut reached = false;
                let stale = qualification::inspect_generated_presentation_after_preparation(
                    live.home().service_reference(),
                    fixture.storage.clone(),
                    fixture.service(),
                    thread_id(),
                    target.resource(),
                    Some(&frozen_pending),
                    || {
                        reached = true;
                        coordinator = Some(start(app, &fixture, false));
                        wait_generated(
                            "publish frozen original",
                            &fixture,
                            thread_id(),
                            target.resource(),
                            || published(&fixture, target) == Some(png_asset()),
                        );
                    },
                )
                .unwrap();
                assert!(stale.is_none());
                reached
            },
        );
        let mut coordinator = coordinator.unwrap();
        let admitted = presentation(
            &fixture,
            thread_id(),
            target.resource(),
            Some(&frozen_pending),
        );
        assert_eq!(admitted.lifecycle, Disposition::Asset(png_asset()));
        assert_eq!(admitted.projection_id, pending.projection_id);
        assert_eq!(admitted.same_presentation, Some(false));
        let child = inherited_branch(&fixture, target.resource());
        let inherited_admitted = presentation(&fixture, child, target.resource(), None);
        assert_eq!(
            inherited_admitted.lifecycle,
            Disposition::Asset(png_asset())
        );
        assert_eq!(inherited_admitted.projection_id, pending.projection_id);
        let mut resolution = None;
        wait_generated(
            "resolve inherited original and rendition",
            &fixture,
            child,
            target.resource(),
            || {
                let live = fixture.service().live_home_command().unwrap();
                resolution = qualification::resolve_generated_presentation(
                    live.home().service_reference(),
                    fixture.storage.clone(),
                    &fixture.state,
                    fixture.service(),
                    child,
                    target.resource(),
                )
                .unwrap();
                resolution.is_some()
            },
        );
        let resolution = resolution.unwrap();
        assert_eq!(resolution.view_thread, child);
        assert_eq!(resolution.source_thread, thread_id());
        assert_eq!(resolution.original, png_asset());
        assert_eq!(resolution.rendition.unwrap(), png_asset());
        assert_eq!(resolution.metadata_dimensions, Some([3, 2]));
        assert_eq!(resolution.presentation.projection_id, pending.projection_id);
        stop(&mut coordinator);
        drop(view);
        assert!(matches!(
            fixture.service.take().unwrap().close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
    });
}
