use super::*;
use crate::diagnostic_child_target::DiagnosticTargetCommandRequest;
use std::{cell::RefCell, rc::Rc};

pub(super) fn dispatch(request: DiagnosticTargetCommandRequest, app: &mut App) {
    let thread = params::switch_thread(request.request().params()).ok();
    let Some(thread) = thread else {
        fail(
            request,
            "invalid_arguments",
            "The requested thread identity is invalid.",
        );
        return;
    };
    let lifetime = app.global::<BootstrapLifetime>();
    if lifetime.commands.diagnostic_exit_pending() {
        fail(
            request,
            "feature_unavailable",
            "Thread selection is unavailable while the child is closing.",
        );
        return;
    }
    let Some(owner) = lifetime.running.clone() else {
        fail(
            request,
            "feature_unavailable",
            "The diagnostic child is not running.",
        );
        return;
    };
    let window = match owner.borrow().diagnostic_main_window(app) {
        Ok(window) => window,
        Err(message) => {
            fail(request, "target_unavailable", message);
            return;
        }
    };
    let response = Rc::new(RefCell::new(Some(request)));
    let completion = response.clone();
    let admitted = window.update(app, move |root, window, cx| {
        let window_id = root.controller().map(|controller| controller.window_id());
        root.diagnostic_request_thread_activation(thread, window, cx, move |result, _, _| {
            let Some(request) = completion.borrow_mut().take() else {
                return;
            };
            let response = match result {
                Ok(status) => DiagnosticProtocolResponse::success(
                    request.request().id(),
                    json!({ "status": status, "threadId": thread, "windowId": window_id }),
                ),
                Err(error) => DiagnosticProtocolResponse::error(
                    Some(request.request().id().to_owned()),
                    "activation_failed",
                    &error,
                ),
            };
            request.respond(response);
        })
    });
    let error = match admitted {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(("activation_failed", error)),
        Err(_) => Some((
            "target_unavailable",
            "The diagnostic main window is unavailable.".to_owned(),
        )),
    };
    if let Some((kind, error)) = error {
        if let Some(request) = response.borrow_mut().take() {
            fail(request, kind, &error);
        }
    }
}

fn fail(request: DiagnosticTargetCommandRequest, kind: &str, message: &str) {
    let response =
        DiagnosticProtocolResponse::error(Some(request.request().id().to_owned()), kind, message);
    request.respond(response);
}
