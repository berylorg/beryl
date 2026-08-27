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
        if service.selected_identity() != Some(selection) {
            return Err("conversation composer selection is stale".to_owned());
        }
        let initial = service.take_initial_presentation(selection)?;
        Self::construct(
            config,
            service,
            clipboard_writer,
            MainWindowConversationComposerRoute::Selected,
            initial,
            window,
            cx,
        )
    }

    pub(in crate::main_window) fn new_pending(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
        receipt: MainWindowComposerActivationReceipt,
        clipboard_writer: ComposerClipboardWriter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let selection = config.selection();
        let (pending, initial) = service.take_pending_initial_presentation(receipt)?;
        if pending != selection || service.pending_identity(receipt) != Some(selection) {
            return Err("pending conversation composer selection is stale".to_owned());
        }
        Self::construct(
            config,
            service,
            clipboard_writer,
            MainWindowConversationComposerRoute::Pending(receipt),
            initial,
            window,
            cx,
        )
    }

    fn construct(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
        clipboard_writer: ComposerClipboardWriter,
        route: MainWindowConversationComposerRoute,
        initial: Box<[crate::composer_host::ComposerHostResponse]>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let selection = config.selection();
        let proof_limits = config.successor_proof_limits();
        let clipboard_limits = config.clipboard_limits();
        let mutation_limits = config.mutation_limits();
        let residency_bound = config.residency_bound()?;
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
        if matches!(route, MainWindowConversationComposerRoute::Pending(_)) {
            input.update(cx, |input, input_cx| {
                input.set_read_only(true, input_cx);
                input.set_enabled(false, input_cx);
            });
        }
        let mut initial_responses = VecDeque::from(initial.into_vec());
        Self::deliver_available_initial_responses(
            selection,
            &input,
            &mut initial_responses,
            window,
            cx,
        )?;
        if matches!(route, MainWindowConversationComposerRoute::Selected) {
            while !input.update(cx, |input, _| input.is_surface_current_and_interactive()) {
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
                        return Err(
                            "initial composer pages did not form a coherent surface".to_owned()
                        );
                    }
                }
            }
            initial_responses.clear();
        }
        input
            .update(cx, |input, _| {
                input.set_history_frontier(
                    input.history_frontier(),
                    selection.binding().range_history_frontier(),
                )
            })
            .map_err(|_| "initial composer history frontier was rejected".to_owned())?;
        let mut this = Self {
            input,
            service,
            selection,
            route,
            pending_realizer: None,
            residency_bound,
            initial_responses,
            clipboard_writer,
            proof_limits,
            clipboard_limits,
            mutation_limits,
            next_operation: 1,
            image_surfaces: MainWindowComposerImageSurfaces::default(),
            image_surface_focus: cx.focus_handle(),
            image_surface_attachment: None,
            pending_marker_removal: None,
            propagated_clipboard: None,
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
        if matches!(this.route, MainWindowConversationComposerRoute::Selected) {
            this.install_interactive_subscription(window, cx);
        }
        this.schedule_pump(window, cx);
        Ok(this)
    }

    fn deliver_available_initial_responses(
        selection: MainWindowComposerSelectionIdentity,
        input: &Entity<RangeTextInput>,
        initial: &mut VecDeque<crate::composer_host::ComposerHostResponse>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        while !initial.is_empty() {
            let Some(request) = input.update(cx, |input, _| input.take_request()) else {
                break;
            };
            if matches!(
                request,
                RangeTextInputRequest::ReleasePage(_) | RangeTextInputRequest::ReleaseObjectPage(_)
            ) {
                continue;
            }
            let response = initial.pop_front().unwrap();
            let outcome =
                super::super::translate_initial_composer_response(selection, request, &response)
                    .map_err(|_| "initial composer response was rejected".to_owned())?;
            match outcome {
                MainWindowComposerDispatchOutcome::Page(page) => input
                    .update(cx, |input, cx| input.deliver_page(page, window, cx))
                    .map_err(|_| "initial composer text page was rejected".to_owned())?,
                MainWindowComposerDispatchOutcome::ObjectPage(page) => input
                    .update(cx, |input, cx| {
                        input.deliver_object_page_in_window(page, window, cx)
                    })
                    .map_err(|_| "initial composer marker page was rejected".to_owned())?,
                _ => return Err("initial composer response was not a bounded page".to_owned()),
            }
        }
        Ok(())
    }

    pub(super) fn deliver_next_initial_response(
        &mut self,
        request: RangeTextInputRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<RangeTextInputRequest>, String> {
        if self.initial_responses.is_empty() {
            return Ok(Some(request));
        }
        if matches!(
            request,
            RangeTextInputRequest::ReleasePage(_) | RangeTextInputRequest::ReleaseObjectPage(_)
        ) {
            return Ok(Some(request));
        }
        let response = self.initial_responses.pop_front().unwrap();
        let outcome =
            super::super::translate_initial_composer_response(self.selection, request, &response)
                .map_err(|_| "pending composer initial response was rejected".to_owned())?;
        match outcome {
            MainWindowComposerDispatchOutcome::Page(page) => self
                .input
                .update(cx, |input, cx| input.deliver_page(page, window, cx))
                .map_err(|_| "pending composer initial text page was rejected".to_owned())?,
            MainWindowComposerDispatchOutcome::ObjectPage(page) => self
                .input
                .update(cx, |input, cx| {
                    input.deliver_object_page_in_window(page, window, cx)
                })
                .map_err(|_| "pending composer initial marker page was rejected".to_owned())?,
            _ => return Err("pending composer initial response was not a bounded page".to_owned()),
        }
        Ok(None)
    }

    pub(super) fn install_interactive_subscription(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._input_event_subscription = Some(cx.subscribe_in(
            &self.input,
            window,
            |this, _, event: &RangeTextInputEvent, window, cx| match event {
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Copy) => {
                    this.begin_propagated_clipboard(ClipboardKind::Copy, window, cx)
                }
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Cut) => {
                    this.begin_propagated_clipboard(ClipboardKind::Cut, window, cx)
                }
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Paste) => {
                    cx.emit(MainWindowConversationComposerEvent::RichPastePropagated {
                        selection: this.selection,
                    });
                }
                RangeTextInputEvent::CommandPropagated(TextInputCommand::Enter) => {
                    cx.emit(MainWindowConversationComposerEvent::SubmitPropagated {
                        selection: this.selection,
                    });
                }
                RangeTextInputEvent::InlineObjectActivated(activation) => {
                    if this.activate_marker(*activation, window, cx).is_err() {
                        this.last_error = Some("composer marker activation was rejected".into());
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
    }
}
