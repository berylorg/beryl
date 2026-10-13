use super::BootstrapLifetime;
use crate::{
    diagnostic_child_protocol::{
        DiagnosticChildCommand, DiagnosticProtocolRequest, DiagnosticProtocolResponse,
    },
    diagnostic_child_target::{DiagnosticTargetServer, DiagnosticTargetShellRequest},
};
use gpui::App;
use serde_json::{Value, json};

mod activation;
mod params;

#[cfg(test)]
pub(super) struct DiagnosticExitObservation(pub(super) std::rc::Rc<std::cell::Cell<usize>>);

#[cfg(test)]
impl gpui::Global for DiagnosticExitObservation {}

pub(super) fn attach(endpoint: DiagnosticTargetServer, app: &mut App) {
    app.spawn(async move |cx| {
        while let Some(request) = endpoint.next().await {
            match request {
                DiagnosticTargetShellRequest::Execute(request) => {
                    if request.try_claim() {
                        if request.request().command() == DiagnosticChildCommand::SwitchThread {
                            let _ = cx.update(|app| activation::dispatch(request, app));
                            continue;
                        }
                        let response = cx.update(|app| respond(request.request(), app));
                        if let Ok(response) = response {
                            request.respond(response);
                        }
                    }
                }
                DiagnosticTargetShellRequest::Shutdown => break,
            }
        }
        let _ = cx.update(|app| {
            app.global::<BootstrapLifetime>()
                .commands
                .request_process_exit();
            #[cfg(test)]
            if let Some(observation) = app.try_global::<DiagnosticExitObservation>() {
                observation.0.set(observation.0.get() + 1);
            }
        });
    })
    .detach();
}

pub(super) fn respond(
    request: &DiagnosticProtocolRequest,
    app: &App,
) -> DiagnosticProtocolResponse {
    let limit = if request.command() == DiagnosticChildCommand::ReadUiState {
        match params::observation_limit(request.params()) {
            Ok(limit) => limit,
            _ => {
                return DiagnosticProtocolResponse::error(
                    Some(request.id().to_owned()),
                    "invalid_arguments",
                    "The UI observation limit must be within its declared bound.",
                );
            }
        }
    } else {
        0
    };
    let lifetime = app.global::<BootstrapLifetime>();
    let stage = if lifetime.running.is_some() {
        "running"
    } else if lifetime.unavailable.is_some() {
        "unavailable"
    } else {
        "starting"
    };
    let facts = lifetime
        .running
        .as_ref()
        .map(|owner| owner.borrow().diagnostic_window_facts(app));
    let surface_state = app.windows().into_iter().find_map(|window| {
        window
            .downcast::<crate::startup_surface::StartupSurface>()
            .and_then(|window| window.read(app).ok())
            .map(|surface| surface.diagnostic_home_state())
    });
    let home_state = facts
        .as_ref()
        .and_then(|facts| facts["homeState"].as_str())
        .or(surface_state)
        .unwrap_or(if lifetime.unavailable.is_some() {
            "retained_unavailable"
        } else {
            "opening"
        });
    let blocked = !matches!(home_state, "healthy" | "opening");
    let has_main_window = facts.as_ref().is_some_and(|facts| {
        facts["mainWindowIds"]
            .as_array()
            .is_some_and(|ids| !ids.is_empty())
    });
    let closing =
        lifetime.commands.diagnostic_exit_pending() || (facts.is_some() && !has_main_window);
    let zero_runtime = facts
        .as_ref()
        .is_some_and(|facts| facts["threadless"] == true);
    let shell_state = if blocked || closing {
        "blocked"
    } else if stage == "starting" {
        "opening"
    } else if zero_runtime {
        "backend_unavailable"
    } else {
        "ready"
    };
    let selected = facts
        .as_ref()
        .map(|facts| facts["selectedThreadId"].clone())
        .unwrap_or(Value::Null);
    let target = lifetime
        .running
        .as_ref()
        .and_then(|owner| owner.borrow().diagnostic_main_window(app).ok())
        .and_then(|window| window.read(app).ok());
    let selected = target
        .as_ref()
        .and_then(|root| root.diagnostic_selected_thread(app))
        .map(|thread| json!(thread))
        .unwrap_or(selected);
    let mounted = target
        .filter(|_| request.command() == DiagnosticChildCommand::ReadUiState)
        .map(|root| root.diagnostic_transcript_facts(app, limit));
    let selected = mounted
        .as_ref()
        .map(|facts| facts["selectedThreadId"].clone())
        .unwrap_or(selected);
    let path = |path: &std::path::Path| {
        path.to_str()
            .filter(|path| path.len() <= 32768)
            .map(str::to_owned)
    };
    match request.command() {
        DiagnosticChildCommand::ReadProcess => DiagnosticProtocolResponse::success(
            request.id(),
            json!({ "pid": std::process::id(), "stage": stage,
                "executablePath": lifetime.executable.as_deref().and_then(path),
                "berylHome": path(&lifetime.home), "selectedThreadId": selected,
                "managedBackendChildPids": null, "managedBackendChildPidsAvailability": "unavailable" }),
        ),
        DiagnosticChildCommand::ReadUiState => DiagnosticProtocolResponse::success(
            request.id(),
            json!({ "stage": stage, "shellState": shell_state, "homeState": home_state,
                "lifecycleState": if closing { "closing" } else { stage },
                "homeFailure": if blocked { json!({"kind":home_state}) } else { Value::Null },
                "mainWindowIds": facts.as_ref().map(|facts| facts["mainWindowIds"].clone()).unwrap_or(json!([])),
                "selectedThreadId": selected, "selectedSurface": if has_main_window { json!("main_window") } else if surface_state.is_some() { json!("startup") } else { Value::Null },
                "backendAvailability": if zero_runtime { "zero_runtime" } else { "unavailable" },
                "backendUnavailable": if zero_runtime { json!({"kind":"zero_runtime", "message":"No runtime is configured."}) } else { Value::Null },
                "turnState": {"selectedThreadState":null,"cancellableActiveTurn":null,"availability":"unavailable"},
                "backgroundWork": {"turnStreamPending":null,"backendWorkReceivers":null,"availability":"unavailable"},
                "exitRequested": lifetime.commands.diagnostic_exit_pending(),
                "windowId": mounted.as_ref().map(|facts| facts["windowId"].clone()),
                "pendingThreadId": mounted.as_ref().map(|facts| facts["pendingThreadId"].clone()),
                "activationPending": mounted.as_ref().map(|facts| facts["activationPending"].clone()),
                "activationFailure": mounted.as_ref().map(|facts| facts["activationFailure"].clone()),
                "resourcePreparationPending": mounted.as_ref().map(|facts| facts["resourcePreparationPending"].clone()),
                "transcript": mounted.as_ref().map(|facts| facts["transcript"].clone()),
                "features": {"transcript": if mounted.is_some() { "resident_observation" } else { "unavailable" }, "settings": "unavailable", "runtimeConfiguration": "unavailable"} }),
        ),
        _ => DiagnosticProtocolResponse::error(
            Some(request.id().to_owned()),
            "feature_unavailable",
            "This diagnostic command has no mounted feature binding.",
        ),
    }
}
