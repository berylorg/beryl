use super::*;
use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostImageMarkerMetadata, SyndicComposerHost,
};
use beryl_home_store::{
    CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace, test_faults::FaultPoint,
};
use beryl_model::AssetId;
use beryl_state::{AssetMediaType, PublishAssetMetadata};
use gpui::{Entity, EntityInputHandler};
use gpui_text_input::{
    InlineObjectId, InlineObjectOrder, RangeHistoryFrontier, RangeSourceSelection,
};
use std::num::NonZeroU64;
use syndic_storage::DraftMarkerAdmissionLimitsV1;

fn limited_mount(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
) -> support::Mounted {
    configured_mount(cx, seed, limits, |_| {}).0
}

pub(super) fn configured_mount(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
    configure: impl FnOnce(&mut SyndicComposerHost) + Send + 'static,
) -> (support::Mounted, Arc<MainWindowConversationComposerService>) {
    configured_seed_mount(cx, seed, limits, false, None, configure)
}

pub(super) fn configured_large_mount(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
) -> (support::Mounted, Arc<MainWindowConversationComposerService>) {
    configured_seed_mount(cx, seed, limits, true, None, |_| {})
}

pub(super) fn configured_large_clipboard_mount(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
    clipboard_bytes: usize,
    mutation_page_bytes: usize,
) -> (support::Mounted, Arc<MainWindowConversationComposerService>) {
    configured_seed_mount(
        cx,
        seed,
        limits,
        true,
        Some((clipboard_bytes, mutation_page_bytes)),
        |_| {},
    )
}

fn configured_seed_mount(
    cx: &mut gpui::TestAppContext,
    seed: u8,
    limits: DraftMarkerAdmissionLimitsV1,
    large: bool,
    clipboard_profile: Option<(usize, usize)>,
    configure: impl FnOnce(&mut SyndicComposerHost) + Send + 'static,
) -> (support::Mounted, Arc<MainWindowConversationComposerService>) {
    let retained = Arc::new(std::sync::Mutex::new(None));
    let captured = retained.clone();
    let mounted = support::mount_with_preparation_timeout(
        cx,
        seed,
        std::time::Duration::from_secs(if large { 180 } else { 20 }),
        move |fixture, services, appearance| {
            let acquisition = fixture.acquire(seed.wrapping_add(1));
            let mut host = SyndicComposerHost::new(fixture.storage.clone());
            assert!(matches!(
                host.test_activate(
                    &fixture.store,
                    composer_support::activation_with_marker_proof(
                        acquisition.thread_id(),
                        seed.wrapping_add(2),
                        seed.wrapping_add(3),
                        1,
                        0
                    ),
                    &CommandCancellation::new(),
                )
                .unwrap(),
                ComposerHostActivationOutcome::Activated { .. }
            ));
            if large {
                host.dispose_composer_service(&fixture.store).unwrap();
                large_draft_support::seed_large_candidate_draft(
                    &fixture.storage,
                    &fixture.store,
                    acquisition.thread_id(),
                );
            } else {
                let binding = host.binding().unwrap();
                mutation_support::commit_text(
                    &mut host,
                    &fixture.store,
                    binding,
                    2000,
                    0,
                    0,
                    "draft",
                    5,
                    1,
                );
                host.dispose_composer_service(&fixture.store).unwrap();
            }
            let mut host = SyndicComposerHost::new(fixture.storage.clone());
            let activation = host
                .test_activate(
                    &fixture.store,
                    composer_support::activation_with_marker_proof(
                        acquisition.thread_id(),
                        if large { 197 } else { seed.wrapping_add(2) },
                        if large { 198 } else { seed.wrapping_add(3) },
                        1,
                        if large {
                            large_draft_support::LARGE_DRAFT_BYTES
                        } else {
                            5
                        },
                    ),
                    &CommandCancellation::new(),
                )
                .unwrap();
            assert!(
                matches!(activation, ComposerHostActivationOutcome::Activated { .. }),
                "large seeded activation: {activation:?}"
            );
            host.test_set_mutation_admission_retained_limits(limits);
            configure(&mut host);
            let slot = MainWindowComposerSlot::new(
                acquisition.window_id(),
                fixture.claim(acquisition.window_id()),
                host,
                fixture.storage.clone(),
                MainWindowComposerMarkerMetadataAuthority::new(fixture.state.assets()),
            )
            .unwrap();
            let service = Arc::new(MainWindowConversationComposerService::new(
                fixture.store.service_reference(),
                slot,
            ));
            *captured.lock().unwrap() = Some(service.clone());
            MainWindowShellPrepared::prepare(
                &fixture.process,
                &fixture.service,
                MainWindowShellPreparationRequest::new(
                    acquisition,
                    service,
                    Box::new(move |selection| {
                        if !large {
                            return config(selection);
                        }
                        let mut widget = composer_support::widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        );
                        widget.layout.limits.segment_bytes = 4096;
                        widget.geometry_limits = gpui_text_input::ExactGeometryLimits::new(
                            49_152,
                            16,
                            4 * 1024 * 1024,
                            65_536,
                        )
                        .unwrap();
                        widget.segmentation_limits =
                            gpui_text_input::SegmentationLimits::new(49_152, 4096).unwrap();
                        widget.limits = gpui_text_input::RangeTextInputLimits::new(
                            8 * 1024 * 1024,
                            131_072,
                            64,
                            gpui::px(64.),
                            49_152,
                            49_152,
                            gpui::px(16.),
                        )
                        .unwrap();
                        if let Some((clipboard_bytes, mutation_page_bytes)) = clipboard_profile {
                            widget.clipboard_limits = gpui_text_input::ClipboardLimits::new(
                                clipboard_bytes,
                                mutation_page_bytes as u64,
                            )
                            .unwrap();
                            widget.mutation_limits =
                                gpui_text_input::MutationLimits::new(64, mutation_page_bytes)
                                    .unwrap();
                        }
                        MainWindowConversationComposerConfig::new(selection, widget)
                            .map_err(|error| error.to_string())
                    }),
                    services.marker_seals.clone(),
                    MainWindowComposerSubmissionRequestSource::new(
                        beryl_app::cas_projection::SubmissionExecutionWake::storage_only_for_test(),
                        services.turn_start_requirement,
                    ),
                    appearance,
                ),
            )
            .unwrap_or_else(|failure| match failure {
                MainWindowShellPreparationFailure::Composer { error, .. } => {
                    panic!("prepare exact limited composer shell: {error}")
                }
                MainWindowShellPreparationFailure::Reservation { error, .. } => {
                    panic!("reserve limited composer shell: {error:?}")
                }
            })
        },
    );
    let service = retained.lock().unwrap().take().unwrap();
    (mounted, service)
}

pub(super) fn composer(
    mounted: &support::Mounted,
    cx: &gpui::TestAppContext,
) -> Entity<MainWindowConversationComposer> {
    mounted
        .window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
        })
        .unwrap()
}

pub(super) fn prepare_editor(
    mounted: &support::Mounted,
    composer: &Entity<MainWindowConversationComposer>,
    cx: &mut gpui::TestAppContext,
) {
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    mounted
        .window
        .update(cx, |_, window, app| {
            input.update(app, |input, _| input.focus(window));
        })
        .unwrap();
    support::draw(cx);
    for _ in 0..512 {
        support::draw(cx);
        if input.read_with(cx, |input, _| {
            input
                .surface()
                .is_some_and(|surface| !surface.object_pages().is_empty())
        }) {
            return;
        }
    }
    panic!(
        "notice composer did not realize its marker gap: {:?}; realization: {:?}",
        composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        input.read_with(cx, |input, _| input.realization_diagnostics())
    );
}

pub(super) fn asset(mounted: &support::Mounted, cx: &mut gpui::TestAppContext) -> AssetId {
    let store = mounted.fixture.store.clone();
    let assets = mounted.fixture.state.assets();
    home_support::join(
        home_support::worker(move || {
            let sidecar = store
                .admit_sidecar(
                    SidecarNamespace::new("images").unwrap(),
                    b"composer-notice-image",
                    SidecarByteLimit::new(NonZeroU64::new(1024).unwrap()),
                )
                .unwrap();
            let asset = AssetId::sha256_v1(
                sidecar.address().digest().as_bytes(),
                NonZeroU64::new(sidecar.address().length()).unwrap(),
            );
            let revision = assets.revision(&store).unwrap();
            let contribution = assets
                .publish_metadata(
                    revision,
                    sidecar,
                    PublishAssetMetadata::new(
                        asset,
                        AssetMediaType::new("image/png").unwrap(),
                        None,
                        revision.checked_next().unwrap(),
                    ),
                )
                .unwrap();
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            contribution.add_to(&mut command).unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            asset
        }),
        cx,
    )
}

pub(super) fn insert(
    mounted: &support::Mounted,
    composer: &Entity<MainWindowConversationComposer>,
    asset: AssetId,
    object: u64,
    cx: &mut gpui::TestAppContext,
) {
    let previous = composer.read_with(cx, |composer, _| {
        composer.mutation_feedback().map(|feedback| feedback.key)
    });
    request_marker(mounted, composer, asset, object, cx);
    support::drive_until(cx, |cx| {
        composer.read_with(cx, |composer, _| {
            composer
                .mutation_feedback()
                .is_some_and(|feedback| Some(feedback.key) != previous)
        })
    });
    support::draw(cx);
}

pub(super) fn request_marker(
    mounted: &support::Mounted,
    composer: &Entity<MainWindowConversationComposer>,
    asset: AssetId,
    object: u64,
    cx: &mut gpui::TestAppContext,
) {
    mounted
        .window
        .update(cx, |_, _, app| {
            composer.update(app, |composer, cx| {
                composer
                    .insert_authenticated_image_marker(
                        ComposerHostImageMarkerMetadata::new(
                            InlineObjectId::new(object.into()),
                            asset,
                        ),
                        InlineObjectOrder::new(object.into()),
                        cx,
                    )
                    .unwrap();
            })
        })
        .unwrap();
}

#[derive(Debug, PartialEq)]
pub(super) struct EditorState {
    pub(super) selection: MainWindowComposerSelectionIdentity,
    pub(super) caret_and_selection: RangeSourceSelection,
    pub(super) history: RangeHistoryFrontier,
    markers: usize,
}

pub(super) fn editor_state(
    composer: &Entity<MainWindowConversationComposer>,
    cx: &gpui::TestAppContext,
) -> EditorState {
    composer.read_with(cx, |composer, app| {
        let surface = composer.surface_snapshot(app).unwrap();
        EditorState {
            selection: composer.selection_identity(),
            caret_and_selection: surface.source_selection,
            history: composer.gpui_input().read(app).history_frontier(),
            markers: surface.realized_object_count,
        }
    })
}

pub(super) fn notice(
    mounted: &support::Mounted,
    cx: &gpui::TestAppContext,
) -> (NoticeVisibleToken, NoticeContent) {
    mounted
        .window
        .read_with(cx, |root, _| {
            let projection = root
                .notice_projection()
                .expect("composer feedback is visible");
            assert_eq!(projection.kind, NoticeKind::Error);
            (projection.token.clone(), projection.content.clone())
        })
        .unwrap()
}

#[gpui::test]
fn marker_limit_feedback_preserves_editor_and_exact_notice_routes(cx: &mut gpui::TestAppContext) {
    for (seed, limits, kind, guidance) in [
        (
            110,
            DraftMarkerAdmissionLimitsV1::new(64, 0, u64::MAX),
            MainWindowComposerMutationFeedbackKind::OperationTooLarge,
            "smaller selection",
        ),
        (
            120,
            DraftMarkerAdmissionLimitsV1::new(0, u64::MAX, u64::MAX),
            MainWindowComposerMutationFeedbackKind::CapacityUnavailable,
            "after capacity is released",
        ),
    ] {
        let mounted = limited_mount(cx, seed, limits);
        let composer = composer(&mounted, cx);
        prepare_editor(&mounted, &composer, cx);
        let asset = asset(&mounted, cx);
        let before = editor_state(&composer, cx);
        let geometry = support::shell_geometry(mounted.window, cx);
        insert(&mounted, &composer, asset, 1, cx);
        assert_eq!(editor_state(&composer, cx), before);
        assert_eq!(support::shell_geometry(mounted.window, cx), geometry);
        composer.read_with(cx, |composer, _| {
            assert_eq!(composer.mutation_feedback().unwrap().kind, kind);
            assert_eq!(composer.last_error(), None);
        });
        let (first, content) = notice(&mounted, cx);
        let first_key =
            composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap().key);
        assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
        assert!(content.detail().as_str().contains(guidance));
        assert_eq!(content.commands().count(), 0);
        let ingress = support::ingress(mounted.window, cx);
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(first.clone()), app))
            .unwrap();
        for _ in 0..3 {
            support::draw(cx);
        }
        assert!(projection(mounted.window, cx).is_none());
        assert_eq!(editor_state(&composer, cx), before);
        insert(&mounted, &composer, asset, 2, cx);
        let (second, _) = notice(&mounted, cx);
        let second_feedback =
            composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
        assert_eq!(second_feedback.kind, kind);
        assert_ne!(second_feedback.key, first_key);
        assert!(!first.record().same_identity(second.record()));
        assert!(matches!(
            cx.update(
                |app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(first.clone()), app)
            ),
            Err(MainWindowNoticeRouteRejection::Notice(
                NoticeRejection::StaleRecord
            ))
        ));
        assert_eq!(notice(&mounted, cx).0, second);
        if kind == MainWindowComposerMutationFeedbackKind::CapacityUnavailable {
            assert_eq!(editor_state(&composer, cx), before);
            assert_eq!(
                composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
                None
            );
            drop(composer);
            support::finish(mounted, cx);
            continue;
        }
        let input = composer.read_with(cx, |composer, _| composer.gpui_input());
        mounted
            .window
            .update(cx, |_, window, app| {
                input.update(app, |input, cx| {
                    input.focus(window);
                    input.replace_text_in_range(None, "a", window, cx);
                })
            })
            .unwrap();
        let mut committed = false;
        for _ in 0..512 {
            support::draw(cx);
            if composer.read_with(cx, |composer, _| {
                composer
                    .selection_identity()
                    .binding()
                    .candidate()
                    .candidate_generation()
                    > before
                        .selection
                        .binding()
                        .candidate()
                        .candidate_generation()
            }) {
                committed = true;
                break;
            }
        }
        assert!(
            committed,
            "next ordinary edit did not settle: {:?}",
            composer.read_with(cx, |composer, _| (
                composer.last_error().map(str::to_owned),
                composer.mutation_feedback()
            ))
        );
        let current = composer.read_with(cx, |composer, _| composer.selection_identity());
        assert_ne!(current.binding().root(), before.selection.binding().root());
        assert_eq!(
            mutation_support::candidate_text(
                mounted.fixture.storage.clone(),
                &mounted.fixture.store,
                current.binding()
            ),
            b"adraft"
        );
        assert!(projection(mounted.window, cx).is_none());
        assert_eq!(
            composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
            None
        );
        drop((composer, input));
        support::finish(mounted, cx);
    }
}

#[gpui::test]
fn marker_storage_refusal_has_distinct_dismissible_feedback(cx: &mut gpui::TestAppContext) {
    let mounted = limited_mount(
        cx,
        130,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
    );
    let composer = composer(&mounted, cx);
    prepare_editor(&mounted, &composer, cx);
    let asset = asset(&mounted, cx);
    let before = editor_state(&composer, cx);
    mounted.fixture.faults.fail_next(FaultPoint::BeforeCommit);
    insert(&mounted, &composer, asset, 1, cx);
    assert_eq!(editor_state(&composer, cx), before);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap().kind),
        MainWindowComposerMutationFeedbackKind::Storage
    );
    let (token, content) = notice(&mounted, cx);
    assert!(content.title().as_str().contains("storage failure"));
    assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
    let ingress = support::ingress(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token), app))
        .unwrap();
    for _ in 0..3 {
        support::draw(cx);
    }
    assert!(projection(mounted.window, cx).is_none());
    assert_eq!(editor_state(&composer, cx), before);
    drop(composer);
    support::finish(mounted, cx);
}

#[gpui::test]
fn marker_preparation_failure_keeps_persistent_exact_request_feedback(
    cx: &mut gpui::TestAppContext,
) {
    let mounted = limited_mount(
        cx,
        140,
        DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
    );
    let composer = composer(&mounted, cx);
    prepare_editor(&mounted, &composer, cx);
    let asset = asset(&mounted, cx);
    let before = editor_state(&composer, cx);
    mounted.fixture.faults.fail_next(FaultPoint::AfterPersist);
    insert(&mounted, &composer, asset, 1, cx);
    assert_eq!(editor_state(&composer, cx), before);
    let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
    );
    let (token, content) = notice(&mounted, cx);
    assert_eq!(content.dismissal, NoticeDismissal::Persistent);
    assert!(
        content
            .detail()
            .as_str()
            .contains("edit outcome is unavailable")
    );
    assert_eq!(content.commands().count(), 0);
    let ingress = support::ingress(mounted.window, cx);
    assert!(matches!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token.clone()), app)),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::Persistent
        ))
    ));
    mounted
        .window
        .update(cx, |_, window, app| {
            composer.update(app, |composer, cx| {
                assert!(composer.retry_mutation_admission(window, cx).is_err());
            })
        })
        .unwrap();
    for _ in 0..3 {
        support::draw(cx);
    }
    assert_eq!(notice(&mounted, cx).0, token);
    assert_eq!(editor_state(&composer, cx), before);
    mounted
        .window
        .update(cx, |root, window, cx| root.retire_notices(window, cx))
        .unwrap();
    support::draw(cx);
    assert!(projection(mounted.window, cx).is_none());
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.mutation_feedback()),
        Some(feedback)
    );
    assert!(matches!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token), app)),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::Disposed
        ))
    ));
    drop(composer);
    support::finish(mounted, cx);
}
