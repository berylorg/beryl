use super::*;

impl MainWindowConversationComposer {
    pub fn new(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
        clipboard_writer: ComposerClipboardWriter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let selection = config.selection();
        let proof_limits = config.successor_proof_limits();
        let clipboard_limits = config.clipboard_limits();
        let mutation_limits = config.mutation_limits();
        if service.selected_identity() != Some(selection) {
            return Err("conversation composer selection is stale".to_owned());
        }
        let initial = service.take_initial_presentation(selection)?;
        if initial.is_empty() {
            return Err(
                "conversation composer activation omitted required initial pages".to_owned(),
            );
        }
        let input = cx.new(|input_cx| {
            config
                .mount(window, input_cx)
                .expect("validated conversation composer configuration")
        });
        for response in initial.iter() {
            let request = loop {
                let request = input
                    .update(cx, |input, _| input.take_request())
                    .ok_or_else(|| "initial composer response has no widget request".to_owned())?;
                if matches!(
                    request,
                    RangeTextInputRequest::ReleasePage(_)
                        | RangeTextInputRequest::ReleaseObjectPage(_)
                ) {
                    continue;
                }
                break request;
            };
            let request_diagnostic = format!("{request:?}");
            match super::super::translate_initial_composer_response(selection, request, response)
                .map_err(|error| {
                    format!(
                        "{error}: initial request {request_diagnostic}, response {:?}",
                        response.key()
                    )
                })? {
                MainWindowComposerDispatchOutcome::Page(page) => input
                    .update(cx, |input, cx| input.deliver_page(page, window, cx))
                    .map_err(|error| error.to_string())?,
                MainWindowComposerDispatchOutcome::ObjectPage(page) => input
                    .update(cx, |input, cx| {
                        input.deliver_object_page_in_window(page, window, cx)
                    })
                    .map_err(|error| error.to_string())?,
                _ => return Err("initial composer response was not a bounded page".to_owned()),
            }
        }
        loop {
            if input.update(cx, |input, _| input.surface().is_some()) {
                break;
            }
            match input.update(cx, |input, _| input.take_request()) {
                Some(
                    RangeTextInputRequest::ReleasePage(_)
                    | RangeTextInputRequest::ReleaseObjectPage(_),
                ) => {}
                Some(request) => {
                    return Err(format!(
                        "initial composer pages did not satisfy widget request: {request:?}"
                    ));
                }
                None => {
                    return Err("initial composer pages did not form a coherent surface".to_owned());
                }
            }
        }
        input
            .update(cx, |input, _| {
                input.set_history_frontier(
                    input.history_frontier(),
                    selection.binding().range_history_frontier(),
                )
            })
            .map_err(|error| format!("initial composer history frontier was rejected: {error}"))?;
        let mut this = Self {
            input,
            service,
            selection,
            clipboard_writer,
            proof_limits,
            clipboard_limits,
            mutation_limits,
            next_operation: 1,
            image_surfaces: MainWindowComposerImageSurfaces::default(),
            image_surface_focus: cx.focus_handle(),
            image_surface_attachment: None,
            pending_marker_removal: None,
            propagated_cut: None,
            pending_marker_metadata: None,
            admitted_positions: None,
            next_flight: 1,
            active_flight: None,
            phase: MainWindowConversationComposerPhase::Live,
            scheduled: false,
            last_error: None,
            _input_subscription: None,
            _input_event_subscription: None,
        };
        this._input_subscription =
            Some(cx.observe_in(&this.input, window, |this, _, window, cx| {
                this.schedule_pump(window, cx);
            }));
        this._input_event_subscription = Some(cx.subscribe_in(
            &this.input,
            window,
            |this, _, event: &RangeTextInputEvent, window, cx| match event {
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Copy) => {
                    this.begin_propagated_clipboard(ClipboardKind::Copy, window, cx)
                }
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Cut) => {
                    this.begin_propagated_clipboard(ClipboardKind::Cut, window, cx)
                }
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Paste) => {
                    cx.emit(MainWindowConversationComposerEvent::RichPastePropagated);
                }
                RangeTextInputEvent::InlineObjectActivated(activation) => {
                    if let Err(error) = this.activate_marker(*activation, window, cx) {
                        this.last_error = Some(error.to_string());
                    }
                }
                RangeTextInputEvent::InlineObjectRealizationLost(loss) => {
                    this.image_surface_attachment = None;
                    this.image_surfaces.realization_lost(*loss);
                    cx.notify();
                }
                _ => {}
            },
        ));
        this.schedule_pump(window, cx);
        Ok(this)
    }
}
