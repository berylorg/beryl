use super::*;
use beryl_app::main_window::{
    MainWindowComposerClipboardFeedbackKind, MainWindowComposerPasteResourceError,
    MainWindowComposerPasteResources, MainWindowComposerSelectionIdentity,
    MainWindowPrivateClipboardOwner,
};
use gpui::{CheckedClipboardSnapshot, ClipboardError, ClipboardItem};
use gpui_text_input::{RangeHistoryFrontier, RangeTextInput};

struct PasteSource {
    service: Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    marker_before: SourcePosition,
}

fn make_source(fixture: &Fixture, seed: u8, foreign: bool, image: bool) -> PasteSource {
    let (selected, target) = fixture.claims();
    make_source_with_claim(
        fixture,
        seed,
        foreign,
        image,
        if foreign { target } else { selected },
    )
}

fn make_source_with_claim(
    fixture: &Fixture,
    seed: u8,
    foreign: bool,
    image: bool,
    claim: beryl_state::WindowClaimSelection,
) -> PasteSource {
    let assets = fixture.assets();
    let thread = if foreign {
        fixture.target_thread
    } else {
        fixture.selected_thread
    };
    let mut host = SyndicComposerHost::new(fixture.storage.clone());
    host.test_activate(
        &fixture.store,
        ComposerHostActivationRequest::new(
            thread,
            syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([seed; 16]),
            support::operation_id(seed.wrapping_add(1)),
            NonZeroU64::new(1).unwrap(),
            None,
            Box::new([]),
        ),
        &CommandCancellation::new(),
    )
    .unwrap();
    let binding = host.binding().unwrap();
    let mut binding =
        composer_support::commit_text(&mut host, &fixture.store, binding, 404, 0, 0, "AB", 2, 1);
    if image {
        let bytes: &[u8] = if foreign {
            b"destination-image"
        } else {
            b"source-image"
        };
        let asset = publication_support::publish_image_asset(&fixture.store, assets.clone(), bytes);
        binding = insert_marker_at(&mut host, &fixture.store, &assets, binding, 405, asset, 0);
    }
    host.dispose_composer_service(&fixture.store).unwrap();
    let mut host = SyndicComposerHost::new(fixture.storage.clone());
    let rebound = activate_with_initial_pages(
        &mut host,
        &fixture.store,
        thread,
        seed,
        seed.wrapping_add(1),
        2,
        image.then_some(DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: 1,
            marker_id: beryl_model::SyndicDraftMarkerId::from_bytes(0x1001_u128.to_be_bytes()),
        }),
    );
    assert_eq!(rebound.root(), binding.root());
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(assets),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    if image {
        let protection = fixture
            .storage
            .draft_image_label_protection_head(
                &fixture.store,
                thread,
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert!(
            protection.protected_maximum().contains(
                selection
                    .binding()
                    .root()
                    .marker_commitment()
                    .maximum_image_label()
                    .unwrap()
            )
        );
    }
    PasteSource {
        service: Arc::new(MainWindowConversationComposerService::new(
            fixture.store.service_reference(),
            slot,
        )),
        selection,
        marker_before: SourcePosition::new(
            ByteOffset::new(0),
            InlineObjectGap::before(InlineObjectNeighbor::new(
                InlineObjectId::new(0x1001),
                InlineObjectOrder::new(1),
            )),
        ),
    }
}

fn mount<'a>(
    cx: &'a mut gpui::TestAppContext,
    source: &PasteSource,
) -> (
    gpui::Entity<MainWindowConversationComposer>,
    &'a mut gpui::VisualTestContext,
) {
    mount_with_mutation_page(cx, source, 4096)
}

fn mount_with_mutation_page<'a>(
    cx: &'a mut gpui::TestAppContext,
    source: &PasteSource,
    mutation_page_bytes: usize,
) -> (
    gpui::Entity<MainWindowConversationComposer>,
    &'a mut gpui::VisualTestContext,
) {
    cx.update(ensure_text_input_bindings);
    let mut widget = widget_config(source.selection.binding().range_binding(), 1024);
    widget.mutation_limits = MutationLimits::new(8, mutation_page_bytes).unwrap();
    let config = MainWindowConversationComposerConfig::new(source.selection, widget).unwrap();
    let (composer, cx) = cx.add_window_view(|window, cx| {
        MainWindowConversationComposer::new(
            config,
            source.service.clone(),
            Box::new(|_, _| ClipboardWriteOutcome::Failed),
            window,
            cx,
        )
        .unwrap()
    });
    drive_owner(cx, 32);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        if source.selection.binding().root().summary().marker_count() == 0 {
            composer_support::position(0)
        } else {
            source.marker_before
        },
        composer_support::position(1),
        cx,
    );
    drive_current_surface(&composer, &input, cx, 256);
    (composer, cx)
}

fn select(
    input: &gpui::Entity<RangeTextInput>,
    selection: MainWindowComposerSelectionIdentity,
    anchor: SourcePosition,
    head: SourcePosition,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input
                .rebind(
                    selection.binding().range_binding(),
                    Some(RangeSourceSelection { anchor, head }),
                    window,
                    cx,
                )
                .unwrap();
        });
    });
    drive_owner(cx, 16);
}

fn inject(
    composer: &gpui::Entity<MainWindowConversationComposer>,
    item: ClipboardItem,
    cx: &mut gpui::VisualTestContext,
) {
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
            Ok(CheckedClipboardSnapshot {
                item: item.clone(),
                sequence: 1,
            })
        }));
    });
}

fn surface_state(
    input: &gpui::Entity<RangeTextInput>,
    cx: &gpui::VisualTestContext,
) -> (RangeSourceSelection, SourcePosition, RangeHistoryFrontier) {
    input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        (
            surface.selection(),
            surface.caret(),
            input.history_frontier(),
        )
    })
}

fn settle(
    composer: &gpui::Entity<MainWindowConversationComposer>,
    cx: &mut gpui::VisualTestContext,
) {
    for _ in 0..512 {
        drive_owner(cx, 1);
        if composer.read_with(cx, |composer, _| !composer.paste_pending()) {
            let input = composer.read_with(cx, |composer, _| composer.gpui_input());
            drive_current_surface(composer, &input, cx, 256);
            for _ in 0..256 {
                if input.read_with(cx, |input, _| input.is_semantically_quiescent()) {
                    return;
                }
                drive_owner(cx, 1);
            }
            panic!("captured paste surface did not become semantically quiescent");
        }
    }
    panic!("captured paste did not settle");
}

fn drive_committed_surface(
    composer: &gpui::Entity<MainWindowConversationComposer>,
    input: &gpui::Entity<RangeTextInput>,
    service: &MainWindowConversationComposerService,
    previous: MainWindowComposerSelectionIdentity,
    cx: &mut gpui::VisualTestContext,
    rounds: usize,
) -> MainWindowComposerSelectionIdentity {
    for _ in 0..rounds {
        drive_owner(cx, 1);
        let selected = service.selected_identity().unwrap();
        if selected.binding().root() != previous.binding().root()
            && composer.read_with(cx, |composer, _| {
                !composer.paste_pending() && !composer.test_has_active_flight()
            })
            && input.read_with(cx, |input, _| {
                input.is_surface_current_and_interactive()
                    && input.surface().is_some_and(|surface| {
                        surface.binding() == selected.binding().range_binding()
                    })
            })
        {
            return selected;
        }
    }
    let summarize = |selection: MainWindowComposerSelectionIdentity| {
        let binding = selection.binding();
        (
            binding.candidate().candidate_generation(),
            binding.candidate().session_generation(),
            binding.range_binding(),
            binding.root().summary().marker_count(),
        )
    };
    let build = service.test_with_selected_host(|host| {
        use syndic_storage::{
            DraftPieceReconciledCommandV1 as Command, DraftPieceSettlementOutcomeV1 as Outcome,
            DraftPieceSettlementProofV1 as Proof, DraftPieceTransactionOutcomeV1 as Terminal,
            OccupiedIdentityDifferenceV1 as Difference,
        };
        host.mutation_build_diagnostics().map(|diagnostics| {
            let result = match diagnostics.result {
                None => "None".to_owned(),
                Some(Command::Pending(_)) => "Pending".to_owned(),
                Some(Command::Terminal(terminal)) => {
                    let (kind, proof) = match terminal {
                        Terminal::Committed(proof) => ("Committed", proof),
                        Terminal::Rejected(proof) => ("Rejected", proof),
                        Terminal::Conflict(proof) => ("Conflict", proof),
                        Terminal::Cancelled(proof) => ("Cancelled", proof),
                        Terminal::Error(proof) => ("Error", proof),
                    };
                    let reason = match proof {
                        Proof::Settlement(settlement) => match settlement.outcome() {
                            Outcome::Committed { candidate_generation, .. } => {
                                format!("Committed generation={candidate_generation}")
                            }
                            Outcome::Conflict { current_candidate_generation, .. } => {
                                format!("Conflict generation={current_candidate_generation}")
                            }
                            outcome => format!("{outcome:?}"),
                        },
                        Proof::OccupiedIdentityNoncommit(proof) => {
                            let difference = match proof.difference() {
                                Difference::Header { .. } => "Header",
                                Difference::Fragment { .. } => "Fragment",
                                Difference::Root { .. } => "Root",
                            };
                            format!("OccupiedIdentityNoncommit difference={difference}")
                        }
                    };
                    format!("Terminal {kind}: {reason}")
                }
            };
            format!(
                "state={:?}, classification={:?}, result={result}, original={:?}, later={:?}, local={:?}, cleanup={:?}",
                diagnostics.state,
                diagnostics.classification,
                diagnostics.original_failure,
                diagnostics.later_failure,
                diagnostics.local_failure,
                diagnostics.cleanup_failure,
            )
        })
    });
    panic!(
        "captured mutation did not commit: selected={:?}, previous={:?}, owner={:?}, surface={:?}, build={build:?}",
        service.selected_identity().map(summarize),
        summarize(previous),
        composer.read_with(cx, |composer, _| (
            composer.last_error().map(str::to_owned),
            composer.paste_pending(),
            composer.test_has_active_flight(),
            composer
                .clipboard_feedback()
                .map(|feedback| (feedback.kind, feedback.paste)),
            composer.mutation_feedback().map(|feedback| feedback.kind),
        )),
        surface_state(input, cx),
    );
}

fn copied_item(
    composer: &gpui::Entity<MainWindowConversationComposer>,
    cx: &mut gpui::VisualTestContext,
) -> ClipboardItem {
    settle(composer, cx);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_writer(Box::new(move |text, metadata, _| {
            captured
                .lock()
                .unwrap()
                .push(ClipboardItem::new_string_with_metadata(
                    text.to_owned(),
                    metadata.unwrap().to_owned(),
                ));
            ClipboardWriteOutcome::Written
        }));
    });
    cx.simulate_keystrokes("ctrl-c");
    drive_owner(cx, 64);
    let mut writes = writes.lock().unwrap();
    assert_eq!(
        writes.len(),
        1,
        "private copy did not write: {:?}",
        composer.read_with(cx, |composer, app| {
            let input = composer.gpui_input();
            let input = input.read(app);
            (
                composer.last_error().map(str::to_owned),
                composer.clipboard_feedback(),
                composer.test_has_active_flight(),
                input.is_semantically_quiescent(),
                input
                    .surface()
                    .map(|surface| (surface.selection(), surface.caret())),
            )
        })
    );
    writes.pop().unwrap()
}

fn encoded_png() -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x03\x08\x06\0\0\0".to_vec();
    bytes.extend_from_slice(&crc(&bytes[12..29]).to_be_bytes());
    let mut data = vec![0x78, 0x01, 0x01, 27, 0, 0xe4, 0xff];
    data.extend_from_slice(&[0; 27]);
    data.extend_from_slice(&[0, 27, 0, 1]);
    for (kind, data) in [(b"IDAT", data.as_slice()), (b"IEND", &[][..])] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = bytes.len();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&crc(&bytes[start..]).to_be_bytes());
    }
    bytes
}

fn marker_ids(
    fixture: &Fixture,
    selection: MainWindowComposerSelectionIdentity,
) -> Vec<beryl_model::SyndicDraftMarkerId> {
    marker_facts(fixture, selection)
        .iter()
        .map(|marker| marker.marker().marker_id())
        .collect()
}

fn marker_facts(
    fixture: &Fixture,
    selection: MainWindowComposerSelectionIdentity,
) -> Vec<syndic_storage::DraftPieceMarkerAtV1> {
    let result = fixture
        .storage
        .candidate_draft_piece_marker_demand(
            &fixture.store,
            selection.binding().candidate(),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::InclusiveRange {
                    start: 0,
                    end: selection.binding().logical_extent().logical_utf8_bytes(),
                },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                4,
                65_536,
            ),
        )
        .unwrap();
    assert!(result.value().requested_side_complete());
    result.value().markers().to_vec()
}

fn publish_candidate_for_restart(fixture: &Fixture, host: &mut SyndicComposerHost) {
    use beryl_app::composer_host::{
        ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
        ComposerHostFlushPurpose, ComposerHostFlushState,
    };
    let assets = fixture.assets();
    let seals = publication_support::service(
        &fixture.store,
        fixture.storage.clone(),
        assets.clone(),
        1,
        32,
    );
    let flush = match host
        .begin_flush(ComposerHostFlushPurpose::Submission)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. }
        | ComposerHostFlushAdmission::Joined { ticket, .. } => ticket,
        other => panic!("restart candidate did not require publication: {other:?}"),
    };
    let capture = host
        .capture_flush_publication(
            &fixture.store,
            flush,
            assets.clone(),
            &seals,
            support::operation_id(227),
            Some(publication_support::authority(228)),
            syndic_storage::SyndicTimestamp::from_unix_millis(227),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(matches!(capture, ComposerHostFlushCapture::Captured(_)));
    for _ in 0..128 {
        match host.advance_flush(&fixture.store, flush).unwrap() {
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission) => return,
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired) => {
                let convergence = host
                    .capture_flush_publication(
                        &fixture.store,
                        flush,
                        assets.clone(),
                        &seals,
                        support::operation_id(229),
                        None,
                        syndic_storage::SyndicTimestamp::from_unix_millis(229),
                        &CommandCancellation::new(),
                    )
                    .unwrap();
                assert!(matches!(
                    convergence,
                    ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission)
                ));
                return;
            }
            ComposerHostFlushAdvance::Progress(_)
            | ComposerHostFlushAdvance::ReconciliationPending => {}
            other => panic!("restart candidate publication did not settle: {other:?}"),
        }
    }
    panic!("restart candidate publication exceeded its bounded work budget");
}

#[test]
fn captured_paste_resource_limits_reject_zero_and_overflow() {
    assert_eq!(
        MainWindowComposerPasteResources::new(0, 7),
        Err(MainWindowComposerPasteResourceError::ZeroQueueCapacity)
    );
    assert_eq!(
        MainWindowComposerPasteResources::new(2, 0),
        Err(MainWindowComposerPasteResourceError::ZeroSidecarPage)
    );
    assert_eq!(
        MainWindowComposerPasteResources::new(usize::MAX, 2),
        Err(MainWindowComposerPasteResourceError::CapacityOverflow)
    );
    assert_eq!(
        MainWindowComposerPasteResources::new(1, usize::MAX),
        Err(MainWindowComposerPasteResourceError::CapacityOverflow)
    );
    let resources = MainWindowComposerPasteResources::new(2, 7).unwrap();
    assert_eq!(resources.queue_items().get(), 2);
    assert_eq!(resources.sidecar_page_bytes().get(), 7);
}

#[gpui::test]
fn captured_image_sidecar_storage_failure_preserves_exact_draft_and_reports_storage(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-image-storage", 211);
    let source = make_source(&fixture, 214, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    let assets = fixture.assets();
    assert!(assets.revision(&fixture.store).is_ok());
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeSidecarWrite);
    inject(
        &composer,
        ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            encoded_png(),
        )),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    for _ in 0..512 {
        drive_owner(cx, 1);
        if !composer.read_with(cx, |composer, _| composer.paste_pending()) {
            break;
        }
    }
    assert!(!composer.read_with(cx, |composer, _| composer.paste_pending()));
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(
        fixture.store.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert!(fixture.store.home_revision().is_err());
    assert!(assets.revision(&fixture.store).is_err());
    let feedback = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerClipboardFeedbackKind::StorageUnavailable
    );
    assert_eq!(feedback.selection, source.selection);
    assert!(feedback.paste);
}

#[gpui::test]
fn captured_image_fresh_preparation_preserves_prior_marker_identity_across_owner_replacement(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-image-identity", 221);
    let source = make_source(&fixture, 224, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let item = ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        encoded_png(),
    ));
    inject(&composer, item.clone(), cx);
    cx.simulate_keystrokes("ctrl-v");
    let first = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    let first_ids = marker_ids(&fixture, first);
    assert_eq!(first_ids.len(), 1);
    let first_marker = marker_facts(&fixture, first)[0];
    let marker_before = SourcePosition::new(
        ByteOffset::new(first_marker.anchor()),
        InlineObjectGap::before(InlineObjectNeighbor::new(
            InlineObjectId::new(u128::from_be_bytes(
                *first_marker.marker().marker_id().as_bytes(),
            )),
            InlineObjectOrder::new(u128::from(first_marker.marker().order_key())),
        )),
    );
    cx.update(|window, app| {
        composer.update(app, |composer, cx| {
            composer.begin_widget_release_fence(window, cx).unwrap()
        })
    });
    source.service.private_clipboard_owner().unwrap().retire();
    source.service.test_with_selected_host(|host| {
        publish_candidate_for_restart(&fixture, host);
        let published = fixture
            .storage
            .current_draft_piece_text_demand(
                &fixture.store,
                fixture.selected_thread,
                DraftPieceTextDemandV1::Validate(0),
                4,
            )
            .unwrap()
            .unwrap();
        assert_eq!(published.selector().root(), first.binding().root());
        host.dispose_composer_service(&fixture.store).unwrap();
    });
    let mut host = SyndicComposerHost::new(fixture.storage.clone());
    let rebound = activate_with_initial_pages(
        &mut host,
        &fixture.store,
        fixture.selected_thread,
        225,
        226,
        first.binding().logical_extent().logical_utf8_bytes(),
        None,
    );
    assert_eq!(rebound.root(), first.binding().root());
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        first.claim(),
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let replacement_selection = slot.selected_identity().unwrap();
    let replacement_service = Arc::new(MainWindowConversationComposerService::new(
        fixture.store.service_reference(),
        slot,
    ));
    replacement_service.set_private_clipboard_owner(MainWindowPrivateClipboardOwner::new());
    let replacement = PasteSource {
        service: replacement_service,
        selection: replacement_selection,
        marker_before,
    };
    let (composer, cx) = mount(&mut cx.cx, &replacement);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        replacement_selection,
        composer_support::position(1),
        composer_support::position(1),
        cx,
    );
    inject(&composer, item, cx);
    let (selection, caret, _) = surface_state(&input, cx);
    let insertion = composer_support::position(1);
    assert_eq!(
        selection,
        RangeSourceSelection {
            anchor: insertion,
            head: insertion
        }
    );
    assert_eq!(caret, insertion);
    assert_eq!(
        input.read_with(cx, |input, _| input.surface().unwrap().binding()),
        replacement_selection.binding().range_binding(),
    );
    cx.simulate_keystrokes("ctrl-v");
    let second = drive_committed_surface(
        &composer,
        &input,
        &replacement.service,
        replacement_selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(second.binding().root().summary().marker_count(), 2);
    let second_ids = marker_ids(&fixture, second);
    assert_eq!(second_ids.len(), 2);
    assert!(second_ids.contains(&first_ids[0]));
    assert_ne!(second_ids[0], second_ids[1]);
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &replacement.service, second, cx, 512);
    assert_eq!(undone.binding().root(), first.binding().root());
}

#[gpui::test]
fn captured_paste_configured_queue_holds_two_sources_and_admits_seven_byte_image_pages(
    cx: &mut gpui::TestAppContext,
) {
    let resources = MainWindowComposerPasteResources::new(2, 7).unwrap();
    let owner = MainWindowPrivateClipboardOwner::with_paste_resources(resources);
    assert_eq!(owner.paste_resources(), resources);
    let first_fixture = Fixture::new("configured-paste-first", 231);
    let first = make_source(&first_fixture, 234, false, false);
    first.service.set_private_clipboard_owner(owner.clone());
    let (first_composer, cx) = mount(cx, &first);
    let first_release = first.service.test_block_next_cut_preparation();
    inject(
        &first_composer,
        ClipboardItem::new_string("X".to_owned()),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(first_composer.read_with(cx, |composer, _| composer.paste_pending()));
    cx.simulate_keystrokes("escape");
    let second_fixture = Fixture::new("configured-paste-second", 241);
    let second = make_source(&second_fixture, 244, false, false);
    second.service.set_private_clipboard_owner(owner.clone());
    let (second_composer, cx) = mount(&mut cx.cx, &second);
    let second_release = second.service.test_block_next_cut_preparation();
    inject(
        &second_composer,
        ClipboardItem::new_string("Y".to_owned()),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(second_composer.read_with(cx, |composer, _| composer.paste_pending()));
    cx.simulate_keystrokes("escape");
    let third_fixture = Fixture::new("configured-paste-third", 251);
    let third = make_source(&third_fixture, 254, false, false);
    third.service.set_private_clipboard_owner(owner);
    let (composer, cx) = mount(&mut cx.cx, &third);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(CheckedClipboardSnapshot {
                item: ClipboardItem::new_image(&gpui::Image::from_bytes(
                    gpui::ImageFormat::Png,
                    encoded_png(),
                )),
                sequence: 1,
            })
        }))
    });
    let before = surface_state(&input, cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::CapacityUnavailable
    );
    first_release.release();
    second_release.release();
    drive_owner(cx, 64);
    assert!(!first_composer.read_with(cx, |composer, _| composer.paste_pending()));
    assert!(!second_composer.read_with(cx, |composer, _| composer.paste_pending()));
    assert_eq!(first.service.selected_identity(), Some(first.selection));
    assert_eq!(second.service.selected_identity(), Some(second.selection));
    cx.simulate_keystrokes("ctrl-v");
    let pasted =
        drive_committed_surface(&composer, &input, &third.service, third.selection, cx, 512);
    settle(&composer, cx);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(pasted.binding().root().summary().marker_count(), 1);
}

#[gpui::test]
fn captured_text_replaces_directed_range_with_one_undo_redo(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("captured-text", 31);
    let source = make_source(&fixture, 34, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        composer_support::position(1),
        composer_support::position(0),
        cx,
    );
    inject(
        &composer,
        ClipboardItem::new_string("[Image A]X".to_owned()),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"[Image A]XB"
    );
    assert_eq!(pasted.binding().root().summary().marker_count(), 0);
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &source.service, pasted, cx, 512);
    assert_eq!(undone.binding().root(), source.selection.binding().root());
    cx.simulate_keystrokes("ctrl-y");
    let redone = drive_committed_surface(&composer, &input, &source.service, undone, cx, 512);
    assert_eq!(redone.binding().root(), pasted.binding().root());
}

#[gpui::test]
fn captured_paste_checked_read_refusal_preserves_exact_surface_and_releases_queue(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-read-refusal", 41);
    let source = make_source(&fixture, 44, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    for (error, kind) in [
        (
            ClipboardError::OverLimit,
            MainWindowComposerClipboardFeedbackKind::TooLarge,
        ),
        (
            ClipboardError::SnapshotChanged,
            MainWindowComposerClipboardFeedbackKind::Unavailable,
        ),
        (
            ClipboardError::Unsupported,
            MainWindowComposerClipboardFeedbackKind::Unavailable,
        ),
        (
            ClipboardError::Malformed,
            MainWindowComposerClipboardFeedbackKind::Unavailable,
        ),
        (
            ClipboardError::Read,
            MainWindowComposerClipboardFeedbackKind::Unavailable,
        ),
    ] {
        composer.update(cx, |composer, _| {
            composer.test_set_checked_clipboard_reader(Box::new(move |limits, _| {
                assert_eq!(limits.total_bytes, 1024);
                Err(error)
            }));
        });
        cx.simulate_keystrokes("ctrl-v");
        settle(&composer, cx);
        assert_eq!(source.service.selected_identity(), Some(source.selection));
        assert_eq!(surface_state(&input, cx), before);
        assert_eq!(
            composer.read_with(cx, |composer, _| composer
                .clipboard_feedback()
                .unwrap()
                .kind),
            kind
        );
    }
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
}

#[gpui::test]
fn captured_paste_pending_blocks_edits_and_retains_original_clipboard_bytes(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-pending", 51);
    let source = make_source(&fixture, 54, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let release = source.service.test_block_next_cut_preparation();
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    let held = surface_state(&input, cx);
    cx.simulate_keystrokes("Q ctrl-v");
    assert_eq!(surface_state(&input, cx), held);
    inject(&composer, ClipboardItem::new_string("later".to_owned()), cx);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    release.release();
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"XB"
    );
}

#[gpui::test]
fn captured_paste_escape_before_admission_preserves_selection_history_and_retry(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-cancel", 61);
    let source = make_source(&fixture, 64, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    let assets = fixture.assets();
    let asset_revision = assets.revision(&fixture.store).unwrap();
    let release = source.service.test_block_next_cut_preparation();
    inject(
        &composer,
        ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            encoded_png(),
        )),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    cx.simulate_keystrokes("escape");
    release.release();
    settle(&composer, cx);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(assets.revision(&fixture.store).unwrap(), asset_revision);
    cx.simulate_keystrokes("ctrl-v");
    drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
}

#[gpui::test]
fn captured_paste_escape_after_durable_begin_retains_exact_commit_custody(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-admitted-cancel", 181);
    let source = make_source(&fixture, 184, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let release = source.service.test_block_next_paste_staging();
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    for _ in 0..512 {
        drive_owner(cx, 1);
        if composer.read_with(cx, |composer, _| {
            composer.test_paste_durable_begin() && composer.test_has_active_flight()
        }) {
            break;
        }
    }
    assert!(composer.read_with(cx, |composer, _| composer.test_paste_durable_begin()));
    assert!(composer.read_with(cx, |composer, _| composer.test_has_active_flight()));
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    cx.simulate_keystrokes("escape");
    drive_owner(cx, 16);
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    release.release();
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"XB"
    );
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &source.service, pasted, cx, 512);
    assert_eq!(undone.binding().root(), source.selection.binding().root());
}

#[gpui::test]
fn captured_image_cancel_after_asset_commit_preserves_draft_and_leaves_inert_asset(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-inert-image", 201);
    let source = make_source(&fixture, 204, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    let assets = fixture.assets();
    let asset_revision = assets.revision(&fixture.store).unwrap();
    let release = source.service.test_block_next_paste_asset_completion();
    inject(
        &composer,
        ClipboardItem::new_image(&gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            encoded_png(),
        )),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 128);
    let admitted_asset_revision = assets.revision(&fixture.store).unwrap();
    assert_ne!(admitted_asset_revision, asset_revision);
    assert!(
        composer.read_with(cx, |composer, _| composer.paste_pending()
            && composer.test_has_active_flight())
    );
    assert!(!composer.read_with(cx, |composer, _| composer.test_paste_durable_begin()));
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    cx.simulate_keystrokes("escape");
    release.release();
    settle(&composer, cx);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(
        assets.revision(&fixture.store).unwrap(),
        admitted_asset_revision
    );
    assert_eq!(
        source.selection.binding().root().summary().marker_count(),
        0
    );
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(pasted.binding().root().summary().marker_count(), 1);
}

#[gpui::test]
fn captured_paste_ambiguous_build_retains_source_gate_and_exact_reconciliation(
    cx: &mut gpui::TestAppContext,
) {
    use beryl_home_store::test_faults::FaultPoint;
    use syndic_storage::{
        DraftPieceReconciledCommandV1, StagedDraftPieceDurableClassificationV1,
        StagedDraftPieceOutcomeStateV1,
    };

    let fixture = Fixture::new("captured-ambiguous", 191);
    let source = make_source(&fixture, 194, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    let mut gate = source.service.test_block_next_selected_dispatch();
    source.service.test_with_selected_host(|host| {
        host.test_set_mutation_transition_limit(1);
    });
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    let mut transferred = false;
    for _ in 0..256 {
        for _ in 0..512 {
            drive_owner(cx, 1);
            if gate.is_blocked() {
                break;
            }
        }
        assert!(
            gate.is_blocked(),
            "captured paste did not reach selected dispatch hold"
        );
        transferred = source.service.test_with_selected_host(|host| {
            host.mutation_build_diagnostics()
                .is_some_and(|diagnostics| {
                    diagnostics.state.is_none()
                        && matches!(
                            diagnostics.result,
                            Some(DraftPieceReconciledCommandV1::Pending(_))
                        )
                })
        });
        if transferred {
            break;
        }
        let next = source.service.test_block_next_selected_dispatch();
        gate.release();
        gate = next;
    }
    assert!(
        transferred,
        "captured paste did not transfer finished staging into durable build custody"
    );
    let faults = fixture.faults.clone();
    source.service.test_with_selected_host(|host| {
        host.test_arm_mutation_before_execute_fault(move |_, _| {
            faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        });
    });
    let next = source.service.test_block_next_selected_dispatch();
    gate.release();
    gate = next;
    for _ in 0..512 {
        drive_owner(cx, 1);
        if gate.is_blocked() {
            break;
        }
    }
    assert!(gate.is_blocked());
    source.service.test_with_selected_host(|host| {
        let diagnostics = host.mutation_build_diagnostics().unwrap();
        assert_eq!(
            diagnostics.state,
            Some(StagedDraftPieceOutcomeStateV1::Reconciling)
        );
        assert_eq!(
            diagnostics.classification,
            Some(StagedDraftPieceDurableClassificationV1::Unresolved)
        );
        assert!(diagnostics.original_failure.is_some());
        assert_eq!(host.settlement_custody_in_use(), 1);
    });
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    assert!(composer.read_with(cx, |composer, _| composer.test_paste_durable_begin()));
    cx.simulate_keystrokes("q ctrl-v ctrl-z ctrl-y escape");
    drive_owner(cx, 16);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    source
        .service
        .test_with_selected_host(|host| assert_eq!(host.settlement_custody_in_use(), 1));
    gate.release();
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"XB"
    );
    source
        .service
        .test_with_selected_host(|host| assert_eq!(host.settlement_custody_in_use(), 0));
}

#[gpui::test]
fn captured_paste_stale_destination_discards_prepared_completion(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("captured-stale-destination", 151);
    let source = make_source(&fixture, 154, false, false);
    let (composer, cx) = mount(cx, &source);
    let release = source.service.test_block_next_cut_preparation();
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(composer.read_with(cx, |composer, _| composer.paste_pending()));
    let successor = source.service.test_with_selected_host(|host| {
        composer_support::commit_text(
            host,
            &fixture.store,
            source.selection.binding(),
            406,
            2,
            2,
            "C",
            3,
            1,
        )
    });
    release.release();
    drive_owner(cx, 128);
    assert!(!composer.read_with(cx, |composer, _| composer.paste_pending()));
    let selected = source.service.selected_identity().unwrap();
    assert_eq!(selected.binding().root(), successor.root());
    assert_eq!(
        composer_support::candidate_text(
            fixture.storage.clone(),
            &fixture.store,
            selected.binding()
        ),
        b"ABC"
    );
}

#[gpui::test]
fn captured_private_paste_preserves_local_label_and_expires_adopted_source(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-local", 71);
    let source = make_source(&fixture, 74, false, true);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        source.marker_before,
        composer_support::position(1),
        cx,
    );
    let item = copied_item(&composer, cx);
    let token = item.metadata().unwrap().clone();
    let owner = source.service.private_clipboard_owner().unwrap();
    assert!(owner.descriptor(&token).is_some());
    select(
        &input,
        source.selection,
        composer_support::position(2),
        composer_support::position(2),
        cx,
    );
    inject(&composer, item, cx);
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(pasted.binding().root().summary().marker_count(), 2);
    assert_eq!(
        pasted
            .binding()
            .root()
            .marker_commitment()
            .maximum_image_label(),
        source
            .selection
            .binding()
            .root()
            .marker_commitment()
            .maximum_image_label()
    );
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"ABA"
    );
    assert!(owner.descriptor(&token).is_none());
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &source.service, pasted, cx, 512);
    assert_eq!(undone.binding().root(), source.selection.binding().root());
}

#[gpui::test]
fn captured_private_paste_metadata_limit_preserves_source_and_releases_queue_before_begin(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-private-metadata-limit", 181);
    let (selected_claim, target_claim) = fixture.claims();
    let origin = make_source_with_claim(&fixture, 184, false, true, selected_claim);
    let destination = make_source_with_claim(&fixture, 186, true, false, target_claim);
    let (origin_composer, cx) = mount(cx, &origin);
    let item = copied_item(&origin_composer, cx);
    let token = item.metadata().unwrap().clone();
    let owner = origin.service.private_clipboard_owner().unwrap();
    let (composer, cx) = mount_with_mutation_page(&mut cx.cx, &destination, 256);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let before = surface_state(&input, cx);
    inject(&composer, item, cx);
    cx.simulate_keystrokes("ctrl-v");
    settle(&composer, cx);
    assert_eq!(
        destination.service.selected_identity(),
        Some(destination.selection)
    );
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(origin.service.selected_identity(), Some(origin.selection));
    assert!(owner.descriptor(&token).is_some());
    assert!(
        !composer.read_with(cx, |composer, _| composer.paste_pending()
            || composer.test_paste_durable_begin()
            || composer.test_has_active_flight())
    );
    let feedback = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerClipboardFeedbackKind::TooLarge
    );
    assert_eq!(feedback.selection, destination.selection);
    assert!(feedback.paste);
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Err(ClipboardError::Read)
        }))
    });
    cx.simulate_keystrokes("ctrl-v");
    settle(&composer, cx);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        destination.service.selected_identity(),
        Some(destination.selection)
    );
    assert_eq!(surface_state(&input, cx), before);
    assert!(owner.descriptor(&token).is_some());
}

#[gpui::test]
fn captured_private_foreign_paste_allocates_destination_label_and_preserves_origin(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-foreign", 121);
    let (selected_claim, target_claim) = fixture.claims();
    let origin = make_source_with_claim(&fixture, 124, false, true, selected_claim);
    let destination = make_source_with_claim(&fixture, 126, true, true, target_claim);
    let (origin_composer, cx) = mount(cx, &origin);
    let origin_input = origin_composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &origin_input,
        origin.selection,
        origin.marker_before,
        composer_support::position(1),
        cx,
    );
    let item = copied_item(&origin_composer, cx);
    let token = item.metadata().unwrap().clone();
    let owner = origin.service.private_clipboard_owner().unwrap();
    let (composer, cx) = mount(&mut cx.cx, &destination);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        destination.selection,
        composer_support::position(2),
        composer_support::position(2),
        cx,
    );
    inject(&composer, item, cx);
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &destination.service,
        destination.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(pasted.binding().root().summary().marker_count(), 2);
    assert_ne!(
        pasted
            .binding()
            .root()
            .marker_commitment()
            .maximum_image_label(),
        destination
            .selection
            .binding()
            .root()
            .marker_commitment()
            .maximum_image_label()
    );
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"ABA"
    );
    assert_eq!(origin.service.selected_identity(), Some(origin.selection));
    assert!(owner.descriptor(&token).is_some());
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &destination.service, pasted, cx, 512);
    assert_eq!(
        undone.binding().root(),
        destination.selection.binding().root()
    );
    cx.simulate_keystrokes("ctrl-y");
    let redone = drive_committed_surface(&composer, &input, &destination.service, undone, cx, 512);
    assert_eq!(redone.binding().root(), pasted.binding().root());
}

#[gpui::test]
fn captured_encoded_image_replaces_range_with_one_marker_and_one_history_step(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-image", 131);
    let source = make_source(&fixture, 134, false, false);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, encoded_png());
    inject(&composer, ClipboardItem::new_image(&image), cx);
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(pasted.binding().root().summary().marker_count(), 1);
    assert_eq!(
        composer_support::candidate_text(fixture.storage.clone(), &fixture.store, pasted.binding()),
        b"B"
    );
    assert!(
        pasted
            .binding()
            .root()
            .marker_commitment()
            .maximum_image_label()
            .is_some()
    );
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &source.service, pasted, cx, 512);
    assert_eq!(undone.binding().root(), source.selection.binding().root());
    cx.simulate_keystrokes("ctrl-y");
    let redone = drive_committed_surface(&composer, &input, &source.service, undone, cx, 512);
    assert_eq!(redone.binding().root(), pasted.binding().root());
}

#[gpui::test]
fn captured_private_paste_noncommit_preserves_exact_prior_draft_and_source_eligibility(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-noncommit", 141);
    let source = make_source(&fixture, 144, false, true);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        source.marker_before,
        composer_support::position(1),
        cx,
    );
    let item = copied_item(&composer, cx);
    let token = item.metadata().unwrap().clone();
    let owner = source.service.private_clipboard_owner().unwrap();
    select(
        &input,
        source.selection,
        composer_support::position(2),
        composer_support::position(2),
        cx,
    );
    let before = surface_state(&input, cx);
    source.service.test_cancel_next_mutation_commit();
    inject(&composer, item, cx);
    cx.simulate_keystrokes("ctrl-v");
    settle(&composer, cx);
    let refused = source.service.selected_identity().unwrap();
    assert_eq!(refused.binding().root(), source.selection.binding().root());
    assert_eq!(
        refused.binding().history(),
        source.selection.binding().history()
    );
    assert_eq!(
        refused.binding().range_binding(),
        source.selection.binding().range_binding()
    );
    assert_eq!(surface_state(&input, cx), before);
    assert!(owner.descriptor(&token).is_some());
    assert_eq!(
        source.selection.binding().root().summary().marker_count(),
        1
    );
    cx.simulate_keystrokes("ctrl-v");
    let pasted = drive_committed_surface(
        &composer,
        &input,
        &source.service,
        source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(pasted.binding().root().summary().marker_count(), 2);
    assert!(owner.descriptor(&token).is_none());
    cx.simulate_keystrokes("ctrl-z");
    let undone = drive_committed_surface(&composer, &input, &source.service, pasted, cx, 512);
    assert_eq!(undone.binding().root(), source.selection.binding().root());
}

#[gpui::test]
fn captured_private_paste_rejects_complete_text_mismatch_without_fallback(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-correlation", 81);
    let source = make_source(&fixture, 84, false, true);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        source.marker_before,
        composer_support::position(1),
        cx,
    );
    let item = copied_item(&composer, cx);
    let token = item.metadata().unwrap().clone();
    let before = surface_state(&input, cx);
    inject(
        &composer,
        ClipboardItem::new_string_with_metadata("[Image A]Z".to_owned(), token.clone()),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    settle(&composer, cx);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert!(
        source
            .service
            .private_clipboard_owner()
            .unwrap()
            .descriptor(&token)
            .is_none()
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::Unavailable
    );
    inject(&composer, item, cx);
    cx.simulate_keystrokes("ctrl-v");
    settle(&composer, cx);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert_eq!(surface_state(&input, cx), before);
    assert!(
        source
            .service
            .private_clipboard_owner()
            .unwrap()
            .descriptor(&token)
            .is_none()
    );
}

#[gpui::test]
fn captured_private_paste_rejects_malformed_and_replaced_tokens_without_text_fallback(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-stale-token", 91);
    let source = make_source(&fixture, 94, false, true);
    let (composer, cx) = mount(cx, &source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    select(
        &input,
        source.selection,
        source.marker_before,
        composer_support::position(1),
        cx,
    );
    let stale = copied_item(&composer, cx);
    let current = copied_item(&composer, cx);
    let before = surface_state(&input, cx);
    for item in [
        ClipboardItem::new_string_with_metadata(
            "[Image A]A".to_owned(),
            "beryl.private-composer.invalid".to_owned(),
        ),
        stale,
        current,
    ] {
        inject(&composer, item, cx);
        cx.simulate_keystrokes("ctrl-v");
        settle(&composer, cx);
        assert_eq!(source.service.selected_identity(), Some(source.selection));
        assert_eq!(surface_state(&input, cx), before);
        assert_eq!(
            composer.read_with(cx, |composer, _| composer
                .clipboard_feedback()
                .unwrap()
                .kind),
            MainWindowComposerClipboardFeedbackKind::Unavailable
        );
    }
}

#[gpui::test]
fn captured_paste_retirement_fences_late_completion_and_releases_queue(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new("captured-retired", 101);
    let source = make_source(&fixture, 104, false, false);
    let (composer, cx) = mount(cx, &source);
    let release = source.service.test_block_next_cut_preparation();
    inject(&composer, ClipboardItem::new_string("X".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    cx.update(|window, app| {
        composer.update(app, |composer, cx| {
            composer.begin_widget_release_fence(window, cx).unwrap()
        })
    });
    release.release();
    drive_owner(cx, 128);
    assert_eq!(source.service.selected_identity(), Some(source.selection));
    assert!(!composer.read_with(cx, |composer, _| composer.paste_pending()));
    let second = Fixture::new("captured-after-retirement", 111);
    let second_source = make_source(&second, 114, false, false);
    let (composer, cx) = mount(&mut cx.cx, &second_source);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    inject(&composer, ClipboardItem::new_string("Y".to_owned()), cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_committed_surface(
        &composer,
        &input,
        &second_source.service,
        second_source.selection,
        cx,
        512,
    );
    settle(&composer, cx);
}

#[gpui::test]
fn captured_paste_queue_refuses_before_acquisition_and_releases_after_cancelled_drain(
    cx: &mut gpui::TestAppContext,
) {
    let first_fixture = Fixture::new("captured-queue-first", 161);
    let first = make_source(&first_fixture, 164, false, false);
    let (first_composer, cx) = mount(cx, &first);
    let release = first.service.test_block_next_cut_preparation();
    inject(
        &first_composer,
        ClipboardItem::new_string("X".to_owned()),
        cx,
    );
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert!(first_composer.read_with(cx, |composer, _| composer.paste_pending()));
    cx.simulate_keystrokes("escape");
    let second_fixture = Fixture::new("captured-queue-second", 171);
    let second = make_source(&second_fixture, 174, false, false);
    let (composer, cx) = mount(&mut cx.cx, &second);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(CheckedClipboardSnapshot {
                item: ClipboardItem::new_string("Y".to_owned()),
                sequence: 1,
            })
        }));
    });
    let before = surface_state(&input, cx);
    cx.simulate_keystrokes("ctrl-v");
    drive_owner(cx, 16);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(surface_state(&input, cx), before);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer
            .clipboard_feedback()
            .unwrap()
            .kind),
        MainWindowComposerClipboardFeedbackKind::CapacityUnavailable
    );
    release.release();
    drive_owner(cx, 64);
    assert!(!first_composer.read_with(cx, |composer, _| composer.paste_pending()));
    assert_eq!(first.service.selected_identity(), Some(first.selection));
    cx.simulate_keystrokes("ctrl-v");
    drive_committed_surface(
        &composer,
        &input,
        &second.service,
        second.selection,
        cx,
        512,
    );
    settle(&composer, cx);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}
