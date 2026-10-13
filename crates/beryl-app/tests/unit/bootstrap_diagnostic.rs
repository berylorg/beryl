use super::*;
use crate::diagnostic_child_target::{DiagnosticStdioClient, DiagnosticTargetServer};
use gpui::AsyncApp;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "bootstrap_diagnostic/fixture.rs"]
mod fixture;

#[path = "bootstrap_diagnostic/images.rs"]
mod images;

#[path = "bootstrap_diagnostic/pending.rs"]
mod pending;

mod native_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_shell_appearance.rs"
    ));
}

struct HeldGate(Arc<crate::transcript_provider::IndexedReadGate>);
impl Drop for HeldGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn send(client: &Arc<DiagnosticStdioClient>, frame: Value, cx: &AsyncApp) -> Value {
    let client = client.clone();
    let id = frame["id"].clone();
    let result = cx
        .background_executor()
        .spawn(async move { client.request(&frame) })
        .await;
    assert_eq!(result["id"], id);
    result
}

async fn state(client: &Arc<DiagnosticStdioClient>, cx: &AsyncApp) -> Value {
    let result = send(
        client,
        json!({"id":"state","command":"read_ui_state","params":{"limit":4}}),
        cx,
    )
    .await;
    assert!(result["error"].is_null(), "{result}");
    result["result"].clone()
}

async fn wait_state(
    client: &Arc<DiagnosticStdioClient>,
    cx: &AsyncApp,
    predicate: impl Fn(&Value) -> bool,
    message: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let value = state(client, cx).await;
        if predicate(&value) {
            return value;
        }
        assert!(Instant::now() < deadline, "{message}: {value}");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
}

async fn observe_failure_through_catalog_poll(
    client: &Arc<DiagnosticStdioClient>,
    failed: &Value,
    cx: &AsyncApp,
) {
    let (window, count) = cx
        .update(|app| {
            let owner = app.global::<BootstrapLifetime>().running.clone().unwrap();
            let window = owner.borrow().diagnostic_main_window(app).unwrap();
            let count = window
                .read(app)
                .unwrap()
                .test_diagnostic_count_observation()
                .0;
            window
                .update(app, |root, window, cx| {
                    root.test_diagnostic_query_failure(window, cx)
                })
                .unwrap();
            assert!(
                window
                    .read(app)
                    .unwrap()
                    .test_diagnostic_count_observation()
                    .1
            );
            (window, count)
        })
        .unwrap();
    assert_eq!(
        state(client, cx).await["activationFailure"],
        failed["activationFailure"]
    );
    let polled = wait_state(
        client,
        cx,
        |_| {
            cx.update(|app| {
                let (published, query_failed) = window
                    .read(app)
                    .unwrap()
                    .test_diagnostic_count_observation();
                published > count && !query_failed
            })
            .unwrap()
        },
        "a real successful count poll must settle the independent query error",
    )
    .await;
    assert_eq!(polled["activationFailure"], failed["activationFailure"]);
    assert_eq!(polled["selectedThreadId"], failed["selectedThreadId"]);
    assert_eq!(
        polled["transcript"]["publishedViewId"],
        failed["transcript"]["publishedViewId"]
    );
}

fn run_transcript_observation(history: bool, images: bool) {
    let (path, first, target, revision) = if images {
        images::home()
    } else {
        fixture::history_home()
    };
    let HomeOpenOutcome::Ready {
        candidate,
        state: storage_state,
        syndic,
    } = fixture::open(&path)
    else {
        unreachable!()
    };
    let home = candidate.publish().unwrap();
    native_appearance::install_native_theme(&home, &storage_state);
    drop((storage_state, syndic));
    home.close().unwrap();
    let configuration =
        Configuration::new(path.clone(), Arc::new(|path, _| fixture::open(path)), true).unwrap();
    let (endpoint, client) = DiagnosticTargetServer::test_stdio_endpoint();
    let joined = client.clone();
    let unsuccessful = Rc::new(Cell::new(false));
    let startup_outcome = unsuccessful.clone();
    Application::new().with_quit_on_last_window_close(false).run(move |app| {
        launch(configuration.startup, startup_outcome, Some(endpoint), app);
        app.spawn(async move |cx| {
            let before = send(&client, json!({"id":"before","command":"read_ui_state"}), cx).await;
            assert_eq!(before["error"]["kind"], "handshake_required");
            let hello = send(&client, json!({"id":"hello","command":"handshake"}), cx).await;
            assert_eq!(hello["result"]["protocol"], "beryl_diagnostic_child");
            let initial = wait_state(&client, cx, |state| state["stage"] == "running" && state["selectedThreadId"] == first.to_string() && state["transcript"]["publishedViewId"] == first.to_string(), "initial ordinary publication").await;
            assert_eq!(initial["features"]["transcript"], "resident_observation");
            let gate = crate::transcript_provider::IndexedReadGate::display(revision);
            let held = HeldGate(gate.clone());
            cx.update(|app| {
                let owner = app.global::<BootstrapLifetime>().running.clone().unwrap();
                let window = owner.borrow().diagnostic_main_window(app).unwrap();
                window.update(app, |root, _, _| root.test_diagnostic_resource_gate(gate.clone())).unwrap();
            }).unwrap();
            if history {
                let accepted = send(&client, json!({"id":"switch","command":"switch_thread","params":{"threadId":target}}), cx).await;
                assert_eq!(accepted["result"]["status"], "opening", "{accepted}");
                assert_eq!(accepted["result"]["threadId"], target.to_string());
                let published = wait_state(&client, cx, |state| gate.entered() && state["selectedThreadId"] == target.to_string() && state["transcript"]["publishedViewId"] == target.to_string() && state["activationPending"] == false && state["resourcePreparationPending"] == true, "coherent activation must complete while an ordinary resource worker is held").await;
                let retained = state(&client, cx).await;
                assert_eq!(published["transcript"]["anchor"], retained["transcript"]["anchor"]);
                assert_eq!(published["transcript"]["visibleRange"], retained["transcript"]["visibleRange"]);
                assert!(published["transcript"]["visibleRows"].as_array().unwrap().len() <= 4);
                assert!(published["transcript"]["estimatedResidentBytes"].as_u64().unwrap() <= 8 * 1024 * 1024);
                if images {
                    assert_eq!(published["transcript"]["imageMarkers"], 48);
                    assert_eq!(published["transcript"]["decodedOrUploadedMediaBytes"], 0);
                }
                drop(held);
                let ready = wait_state(&client, cx, |state| state["selectedThreadId"] == target.to_string() && state["transcript"]["codeReady"].as_u64().unwrap_or(0) > 0 && state["resourcePreparationPending"] == false && state["transcript"]["staged"] == false && state["transcript"]["measurementPreparationPending"] == false, "ordinary Code preparation must settle independently after activation").await;
                assert_eq!(published["transcript"]["manualScrollTotalPx"], ready["transcript"]["manualScrollTotalPx"]);
                assert_eq!(published["transcript"]["publishedViewId"], ready["transcript"]["publishedViewId"]);
                if !images {
                assert_eq!(published["transcript"]["scrollMode"], "live-tail-following");
                assert_eq!(published["transcript"]["scrollMode"], ready["transcript"]["scrollMode"]);
                assert!(!published["transcript"]["tailPlacement"].is_null(), "{published}");
                assert_eq!(published["transcript"]["tailPlacement"]["recordId"], ready["transcript"]["tailPlacement"]["recordId"]);
                assert_eq!(published["transcript"]["tailPlacement"]["bottomPx"], ready["transcript"]["tailPlacement"]["bottomPx"]);
                assert_eq!(published["transcript"]["viewportHeightPx"], ready["transcript"]["viewportHeightPx"]);
                for snapshot in [&published, &ready] {
                    let bottom = snapshot["transcript"]["tailPlacement"]["bottomPx"].as_f64().unwrap();
                    let viewport = snapshot["transcript"]["viewportHeightPx"].as_f64().unwrap();
                    assert!((bottom - viewport).abs() < 0.01, "{snapshot}");
                }
                }
                if images {
                    assert_eq!(published["transcript"]["anchor"], ready["transcript"]["anchor"]);
                    assert_eq!(published["transcript"]["visibleRange"], ready["transcript"]["visibleRange"]);
                    assert_eq!(ready["transcript"]["imageMarkers"], 48);
                    assert_eq!(ready["transcript"]["decodedOrUploadedMediaBytes"], 0);
                }
                let current = send(&client, json!({"id":"current","command":"switch_thread","params":{"threadId":target}}), cx).await;
                assert_eq!(current["result"]["status"], "current");
            } else {
                drop(held);
                for (id, params) in [("malformed", json!({"threadId":"missing"})), ("extra", json!({"threadId":target,"fallback":first})), ("absent", json!({"threadId":beryl_model::SyndicThreadId::from_bytes([0x76;16])}))] {
                    let rejected = send(&client, json!({"id":id,"command":"switch_thread","params":params}), cx).await;
                    assert!(!rejected["error"].is_null(), "{rejected}");
                    let unchanged = state(&client, cx).await;
                    assert_eq!(unchanged["selectedThreadId"], first.to_string());
                    assert_eq!(unchanged["transcript"]["publishedViewId"], first.to_string());
                    assert_eq!(unchanged["activationPending"], false);
                    if id == "absent" {
                        assert_eq!(unchanged["activationFailure"]["threadId"], params["threadId"]);
                        observe_failure_through_catalog_poll(&client, &unchanged, cx).await;
                    }
                }
                let limited = send(&client, json!({"id":"limit","command":"read_ui_state","params":{"limit":65}}), cx).await;
                assert_eq!(limited["error"]["kind"], "invalid_arguments");
            }
            client.close();
        }).detach();
    });
    joined.join();
    assert!(!unsuccessful.get());
    let HomeOpenOutcome::Ready { candidate, .. } = fixture::open(&path) else {
        unreachable!()
    };
    candidate.close().unwrap();
}

#[test]
fn native_diagnostic_stdio_history_activation_publishes_before_resource_preparation() {
    run_transcript_observation(true, false);
}

#[test]
fn native_diagnostic_stdio_invalid_and_missing_targets_preserve_exact_source() {
    run_transcript_observation(false, false);
}

#[test]
fn native_diagnostic_stdio_image_heavy_activation_publishes_with_bounded_markers_and_pending_resources()
 {
    run_transcript_observation(true, true);
}
