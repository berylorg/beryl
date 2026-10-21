use super::super::*;
use gpui::{AppContext, Context as ViewContext, IntoElement, Render, Window, div};

#[cfg(all(target_os = "windows", feature = "test-faults"))]
#[path = "../../syndic_composer_host/support.rs"]
mod host_fixture;

#[cfg(all(target_os = "windows", feature = "test-faults"))]
mod source_fixture {
    use super::host_fixture::{committed, execute, point};

    fn marker(seed: u8, order: u64, label: u64) -> syndic_storage::DraftPieceMarkerV1 {
        syndic_storage::DraftPieceMarkerV1::new(
            beryl_model::SyndicDraftMarkerId::from_bytes([seed; 16]),
            order,
            beryl_model::ImageLabelOrdinal::new(label).unwrap(),
            beryl_model::AssetId::sha256_v1([seed; 32], std::num::NonZeroU64::MIN),
        )
    }

    mod insertion {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/syndic_composer_host/support/populate.rs"
        ));

        pub(crate) fn original(
            storage: SyndicStorage,
            store: &HomeStore,
            thread: SyndicThreadId,
            asset: beryl_model::AssetId,
        ) -> DraftPieceMarkerV1 {
            let marker = DraftPieceMarkerV1::new(
                beryl_model::SyndicDraftMarkerId::from_bytes([231; 16]),
                1,
                beryl_model::ImageLabelOrdinal::new(1).unwrap(),
                asset,
            );
            let mut host = SyndicComposerHost::new(storage.clone());
            let ComposerHostActivationOutcome::Activated { binding, .. } = host
                .test_activate(
                    store,
                    ComposerHostActivationRequest::new(
                        thread,
                        DraftEditorCandidateSessionIdV1::from_bytes([212; 16]),
                        DraftPieceOperationIdV1::from_bytes([222; 16]),
                        NonZeroU64::MIN,
                        None,
                        Box::new([]),
                    ),
                    &CommandCancellation::new(),
                )
                .unwrap()
            else {
                panic!("fixture activation")
            };
            let zero = position(0);
            let binding = commit_page(
                &mut host,
                store,
                binding,
                213,
                SourceRange::new(zero, zero).unwrap(),
                vec![MutationPageItem::Utf8 {
                    inserted_offset: 0,
                    text: "\u{03b1}\n\u{03b2}\n".into(),
                }],
                Box::new([]),
                LogicalExtent::new(6, 3),
                MutationPositions::collapsed(position(6)),
            );
            let session = active_session(&storage, store, binding);
            complete_admitted_marker_insertion(&storage, store, &session, marker, 232, point(3));
            marker
        }
    }

    pub(super) use insertion::original;
}

struct Host;
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut ViewContext<Self>) -> impl IntoElement {
        div()
    }
}

pub(super) fn with_window(
    run: impl FnOnce(Arc<ProcessCore>, ImageMediaAttachment, ImageMediaWindow, ImageSourceDemand)
    + 'static,
) {
    use std::{cell::Cell, rc::Rc};
    let completed = Rc::new(Cell::new(false));
    let done = completed.clone();
    gpui::Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let (_directory, home, state, storage, thread) =
                host_fixture::fixture_with_state("original-commands", 211);
            let bytes = b"original GIF bytes without a supported rendition decoder";
            let admitted = home
                .admit_sidecar(
                    beryl_home_store::SidecarNamespace::new("images").unwrap(),
                    bytes,
                    beryl_home_store::SidecarByteLimit::new(
                        std::num::NonZeroU64::new(1024).unwrap(),
                    ),
                )
                .unwrap();
            let asset = beryl_model::AssetId::sha256_v1(
                admitted.address().digest().as_bytes(),
                std::num::NonZeroU64::new(admitted.address().length()).unwrap(),
            );
            let assets = state.assets();
            let revision = assets.revision(&home).unwrap();
            let metadata = assets
                .publish_metadata(
                    revision,
                    admitted,
                    beryl_state::PublishAssetMetadata::new(
                        asset,
                        beryl_state::AssetMediaType::new("image/gif").unwrap(),
                        None,
                        revision.checked_next().unwrap(),
                    ),
                )
                .unwrap();
            let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
            metadata.add_to(&mut command).unwrap();
            host_fixture::committed(home.execute(command));
            let marker = source_fixture::original(storage.clone(), &home, thread, asset);
            let current = host_fixture::current(storage.clone(), &home, thread);
            let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
                .draft_editor_candidate_session(
                    &home,
                    current.draft().id(),
                    syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([212; 16]),
                )
                .unwrap()
            else {
                panic!("active candidate")
            };
            let binding = crate::composer_host::ComposerHostBinding::new(
                home.home_id(),
                home.health().generation().unwrap(),
                crate::composer_host::ComposerHostGeneration::FIRST,
                syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
                std::num::NonZeroU64::MIN,
            );
            let demand = ImageSourceDemand::Composer {
                binding,
                marker_id: marker.marker_id(),
                order_key: marker.order_key(),
            };
            let limits = limits();
            let config = limits.configuration();
            let domain = app
                .image_resource_domain(gpui::ImageResourceDomainConfig {
                    staging_buffers: config.upload_staging_buffers,
                    staging_bytes: config.upload_staging_bytes,
                    texture_entries: config.gpu_texture_entries,
                    texture_bytes: config.gpu_texture_bytes as u64,
                    window_pin_entries: config.window_pin_entries,
                    window_pin_bytes: config.window_pin_gpu_bytes as u64,
                })
                .unwrap();
            let core = Arc::new(ProcessCore {
                original_capacity: Arc::new(OriginalCapacityEvent::new(limits)),
                native_progress: Arc::new(signal::NativeProgress::default()),
                domain,
                limits,
                cpu: CpuSurfaceCache::new(limits),
                state: Mutex::new(ProcessState::default()),
                work: Condvar::new(),
                event: Arc::new(signal::ChangeEvent::default()),
            });
            let attachment = ImageMediaAttachment {
                inner: Arc::new(AttachmentState {
                    id: 1,
                    core: Arc::downgrade(&core),
                    source: Mutex::new(Some(Arc::new(
                        ImageSourceProvider::new(
                            home.service_reference(),
                            storage,
                            state.assets(),
                            limits,
                        )
                        .unwrap(),
                    ))),
                    consumer: core.domain.begin_consumer().unwrap(),
                    retired: AtomicBool::new(false),
                    active_workers: AtomicUsize::new(0),
                    event: Arc::new(signal::ChangeEvent::default()),
                }),
            };
            lock(&core.state).attachment = Arc::downgrade(&attachment.inner);
            let host = app
                .open_window(gpui::WindowOptions::default(), |_, cx| cx.new(|_| Host))
                .unwrap();
            let window = host
                .update(app, |_, window, _| attachment.new_window(window).unwrap())
                .unwrap();
            run(core, attachment, window, demand);
            done.set(true);
            app.quit();
        });
    assert!(completed.get());
}

pub(super) fn wait_prepared(command: &OriginalImageCommand) {
    struct Ready(std::sync::mpsc::SyncSender<()>);
    impl Wake for Ready {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(Ready(sender)));
    let mut context = Context::from_waker(&waker);
    loop {
        let mut changed = command.changed();
        match command.state() {
            OriginalImageState::PreparedSave => return,
            OriginalImageState::Preparing => (),
            other => panic!("unexpected preparation {other:?}"),
        }
        if Pin::new(&mut changed).poll(&mut context).is_pending() {
            receiver
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap();
        }
    }
}
