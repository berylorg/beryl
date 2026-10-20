use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeLaunchForm, RuntimeMode, RuntimeNativePath, WindowId,
};
use beryl_state::{
    AvailabilitySnapshot, CreateRuntimeWithHomeRoot, InitializeThreadlessWindow, RememberedTarget,
    RemoveSessionWindow, RootRegistration, RuntimeRegistration, UnixMillis,
};
use gpui::{EntityInputHandler, Focusable};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn seed(path: &Path) -> PathBuf {
    let configured = PathBuf::from(
        std::env::var_os("BERYL_MANAGED_RUNTIME_FIXTURE")
            .expect("BERYL_MANAGED_RUNTIME_FIXTURE must name the built managed runtime fixture"),
    );
    assert!(configured.is_absolute() && configured.is_file());
    let executable = std::fs::canonicalize(configured).unwrap();
    let root = path.join("discussion-runtime");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(path.join("runtime-tokens")).unwrap();
    std::fs::write(root.join("fixture-mode"), "discussion-flow").unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let host = |path: &Path| {
        AdmittedHostPath::from_admitted(
            PathFlavor::Windows,
            path.to_str().unwrap().trim_start_matches(r"\\?\"),
        )
        .unwrap()
    };
    let native = |path: &Path| {
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            host(path).as_str(),
        )
        .unwrap()
    };
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("discussion flow Home must open");
    };
    let home = candidate.publish().unwrap();
    let runtime = RuntimeId::from_bytes([194; 16]);
    let root_id = RootId::from_bytes([195; 16]);
    let available =
        AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1)).unwrap();
    let registration = CreateRuntimeWithHomeRoot::new(
        RuntimeRegistration::new(
            runtime,
            host(&executable),
            RuntimeMode::host(),
            RuntimeLaunchForm::CodexCli,
            native(&executable),
            UnixMillis::new(1),
            available,
        )
        .unwrap(),
        RootRegistration::new(
            root_id,
            native(&root),
            host(&root),
            UnixMillis::new(1),
            available,
        ),
    )
    .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&home).unwrap(),
            registration,
        ))
        .unwrap();
    let temporary_window = WindowId::from_bytes([250; 16]);
    command
        .add(state.session().initialize_threadless(
            state.session().revision(&home).unwrap(),
            InitializeThreadlessWindow::new(temporary_window, window_services::placement()),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let bootstrap = state.session().minimal_bootstrap(&home).unwrap().unwrap();
    let temporary = &bootstrap.windows()[0];
    let removal = state
        .session()
        .remove_window(
            &home,
            state.session().revision(&home).unwrap(),
            RemoveSessionWindow::new(
                bootstrap.header().revision(),
                temporary_window,
                temporary.revision(),
                temporary.selected_thread(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    removal.add_to(&mut command).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let process =
        crate::window_acquisition::RuntimeBackedWindowProcessRegistry::new(Default::default());
    let acquisition = crate::window_acquisition::RuntimeBackedWindowAcquisitionService::new(
        &process,
        std::sync::Arc::new(home.service_reference()),
        state.clone(),
        syndic.clone(),
    );
    for seed in [195u8, 196] {
        let request = crate::window_acquisition::RuntimeBackedWindowAcquisitionRequest::new(
            WindowId::from_bytes([seed; 16]),
            RememberedTarget::new(runtime, root_id),
            window_services::placement(),
            SyndicThreadId::from_bytes([seed.wrapping_add(40); 16]),
            SyndicDraftId::from_bytes([seed.wrapping_add(41); 16]),
            ExecutionBinding::new(runtime, root_id, native(&root)),
            SyndicTimestamp::from_unix_millis(1000),
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        )
        .unwrap();
        let outcome = acquisition.acquire(request, beryl_home_store::CommandCancellation::new());
        assert!(
            matches!(
                outcome,
                crate::window_acquisition::RuntimeBackedWindowAcquisitionOutcome::Committed { .. }
            ),
            "typed discussion fixture window acquisition refused: {outcome:?}"
        );
    }
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(syndic.create_thread(
            syndic.revision(&home).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([237; 16]),
                SyndicDraftId::from_bytes([238; 16]),
                ExecutionBinding::new(runtime, root_id, native(&root)),
                SyndicTimestamp::from_unix_millis(1000),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    drop((acquisition, process, state, syndic));
    home.close().unwrap();
    root
}

pub(super) fn publish(root: &Path, name: &str, bytes: &[u8]) {
    let destination = root.join(name);
    assert!(!destination.exists());
    let pending = destination.with_extension("pending");
    std::fs::write(&pending, bytes).unwrap();
    std::fs::rename(pending, destination).unwrap();
}

pub(super) fn records(root: &Path, prefix: &str) -> Vec<Value> {
    let mut values = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if name.starts_with(prefix) && name.ends_with(".json") {
            assert!(values.len() < 256, "managed evidence must remain bounded");
            let bytes = std::fs::read(entry.path()).unwrap();
            assert!(bytes.len() <= 1_000_000);
            values.push(serde_json::from_slice(&bytes).unwrap());
        }
    }
    values
}

pub(super) async fn started(
    window: WindowHandle<MainWindowShellRoot>,
    root: &Path,
    text: &str,
    cx: &mut AsyncApp,
) -> Value {
    let mut result = None;
    wait_until(
        window,
        cx,
        |cx| {
            let matches: Vec<_> = records(root, "discussion-started-")
                .into_iter()
                .filter(|value| value["input"][0]["text"] == text)
                .collect();
            assert!(matches.len() <= 1, "ordinary input must not dispatch twice");
            result = matches.into_iter().next();
            result.is_some()
        },
        "the original ordinary scheduler must dispatch the exact native input",
    )
    .await;
    result.unwrap()
}

pub(super) async fn response(
    window: WindowHandle<MainWindowShellRoot>,
    root: &Path,
    turn: &Value,
    resolution: &str,
    cx: &mut AsyncApp,
) -> Value {
    let turn = turn["turn"].as_str().unwrap();
    publish(
        root,
        &format!("discussion-resolve-{turn}.json"),
        &serde_json::to_vec(&serde_json::json!({"resolution":resolution})).unwrap(),
    );
    let path = root.join(format!("discussion-response-{turn}.json"));
    wait_until(
        window,
        cx,
        |_| path.exists(),
        "the actual dynamic resolution tool must receive its original response",
    )
    .await;
    let wire: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    serde_json::from_str(wire["result"]["contentItems"][0]["text"].as_str().unwrap()).unwrap()
}

pub(super) fn complete(root: &Path, turn: &Value) {
    publish(
        root,
        &format!("discussion-complete-{}", turn["turn"].as_str().unwrap()),
        b"",
    );
}

pub(super) async fn prepare_source(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    window: WindowHandle<MainWindowShellRoot>,
    root: &Path,
    cx: &mut AsyncApp,
) {
    let question = "Create the exact assistant passage for this discussion.";
    let selected = window
        .read_with(cx, |root, app| root.coherent_viewed_thread(app).unwrap())
        .unwrap();
    submit(window, question, cx).await;
    let turn = started(window, root, question, cx).await;
    let turn_id = turn["turn"].as_str().unwrap();
    publish(
        root,
        &format!("discussion-assistant-{turn_id}.json"),
        &serde_json::to_vec(&serde_json::json!({"text":SOURCE})).unwrap(),
    );
    complete(root, &turn);
    let emitted = root.join(format!("discussion-assistant-emitted-{turn_id}.json"));
    wait_until(
        window,
        cx,
        |_| emitted.exists(),
        "the actual ordinary source turn must emit its exact assistant passage before terminal completion",
    )
    .await;
    let emitted: Value = serde_json::from_slice(&std::fs::read(emitted).unwrap()).unwrap();
    assert_eq!(emitted["text"], SOURCE);
    wait_until(
        window,
        cx,
        |_| {
            let owner = owner.borrow();
            if !owner.test_services_ready() {
                return false;
            }
            let Some(graph) = owner.test_services().graph() else {
                return false;
            };
            let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
            let complete = graph
                .syndic()
                .thread(graph.home(), selected, limit)
                .ok()
                .flatten()
                .and_then(|record| record.committed_tail())
                .and_then(|turn| {
                    graph
                        .syndic()
                        .turn_state(graph.home(), turn, limit)
                        .ok()
                        .flatten()
                })
                .is_some_and(|state| {
                    state.terminal_outcome() == Some(syndic_storage::TurnTerminalOutcome::Complete)
                });
            complete
                && graph
                    .syndic()
                    .input_gate(graph.home(), selected, limit)
                    .ok()
                    .flatten()
                    .is_some_and(|gate| gate.state() == &syndic_storage::InputGateState::Idle)
        },
        "the original source turn must finish terminal settlement before branch capture",
    )
    .await;
    let panel = window
        .read_with(cx, |root, _| root.test_transcript_panel())
        .unwrap();
    ready(window, &panel, cx, &["branch prefix"]).await;
}

pub(super) async fn type_text(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    text: &str,
    cx: &mut AsyncApp,
) {
    perform_native_input(window, resident, None, Some(text), cx).await;
}

pub(super) async fn submit(
    window: WindowHandle<MainWindowShellRoot>,
    text: &str,
    cx: &mut AsyncApp,
) {
    let resident = composer(window, cx).await;
    cx.update(gpui_text_input::ensure_text_input_bindings)
        .unwrap();
    perform_native_input(window, &resident, None, Some(text), cx).await;
    let predecessor =
        perform_native_input(window, &resident, Some(text.len() as u64), None, cx).await;
    let draft = predecessor.binding().candidate().draft_id();
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    root.test_submission_composer(app).is_some_and(|editor| {
                        let editor = editor.read(app);
                        let current = editor.selection_identity();
                        current.binding().candidate().draft_id() != draft
                            && current.binding().logical_extent().logical_utf8_bytes() == 0
                    })
                })
                .unwrap()
        },
        "native Enter must publish the ordinary empty successor",
    )
    .await;
}

async fn perform_native_input(
    window: WindowHandle<MainWindowShellRoot>,
    resident: &gpui::Entity<MainWindowConversationComposer>,
    bytes: Option<u64>,
    text: Option<&str>,
    cx: &mut AsyncApp,
) -> crate::main_window::MainWindowComposerSelectionIdentity {
    let root_entity = window.entity(cx).unwrap();
    let mut issued = None;
    wait_until(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, app| {
                let ready = {
                    let root = root_entity.read(app);
                    let owner = resident.read(app);
                    let input = owner.gpui_input();
                    let input = input.read(app);
                    let actual_bytes = owner
                        .selection_identity()
                        .binding()
                        .logical_extent()
                        .logical_utf8_bytes();
                    let submission = root
                        .test_submission_observation(app)
                        .and_then(|proof| proof.availability())
                        .err();
                    let synchronized = root
                        .test_submission_mount()
                        .and_then(|mount| mount.read(app).selected_identity())
                        == Some(owner.selection_identity());
                    let discussion = root.test_discussion_presentation(app).map(
                        |(_, gated, _, entity, fresh)| {
                            (gated, fresh, entity == resident.entity_id())
                        },
                    );
                    let platform_selection = input
                        .surface()
                        .is_some_and(|surface| surface.platform_selection().is_some());
                    let ready = bytes.is_none_or(|expected| expected == actual_bytes)
                        && submission.is_none()
                        && owner.submission_unavailable().is_none()
                        && synchronized
                        && discussion == Some((false, true, true))
                        && platform_selection
                        && input.is_enabled()
                        && input.is_semantically_quiescent()
                        && input.is_surface_current_and_interactive()
                        && owner.test_edit_position_proof_ready(app)
                        && !owner.test_has_active_flight()
                        && !owner.test_has_pending_realizer();
                    ready
                };
                if ready {
                    issued = Some(resident.read(app).selection_identity());
                    let input = resident.read(app).gpui_input();
                    input.read(app).focus(native);
                    assert!(input.read(app).focus_handle(app).is_focused(native));
                    if let Some(text) = text {
                        input.update(app, |input, app| {
                            input.replace_and_mark_text_in_range(None, text, None, native, app)
                        });
                    } else {
                        key(native, app, "enter");
                    }
                }
                ready
            })
            .unwrap()
        },
        "the exact native editor must be interactive and settled before typing or Enter",
    )
    .await;
    issued.unwrap()
}

pub(super) async fn select(
    window: WindowHandle<MainWindowShellRoot>,
    thread: SyndicThreadId,
    cx: &mut AsyncApp,
) {
    window
        .update(cx, |root, native, app| {
            root.open_thread_switcher(native, app)
        })
        .unwrap();
    let mut target = None;
    wait_until(
        window,
        cx,
        |cx| {
            target = window
                .read_with(cx, |root, _| {
                    Some((
                        root.test_thread_switcher_picker()?,
                        root.test_thread_switcher_thread_key(thread)?,
                    ))
                })
                .unwrap();
            target.is_some()
        },
        "the actual parent switcher row must become available",
    )
    .await;
    let (picker, key) = target.unwrap();
    window
        .update(cx, |_, _, app| {
            picker.update(app, |picker, cx| picker.activate(&key, cx))
        })
        .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    assert!(root.test_running_activation_failure().is_none(), "thread activation failed for {thread:?}: {:?}; picker: {:?}; operation: {:?}", root.test_running_activation_failure(), root.test_thread_switcher_failure(), root.test_thread_confirmation_diagnostics());
                    root.coherent_viewed_thread(app) == Some(thread)
                        && !root.test_running_thread_activation_pending()
                })
                .unwrap()
        },
        "the original same-window activation must mount the exact parent",
    )
    .await;
}
