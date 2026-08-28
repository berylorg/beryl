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
        let activation_seeds = Self::activation_seeds(selection, initial)?;
        Self::construct(
            config,
            service,
            clipboard_writer,
            MainWindowConversationComposerRoute::Selected,
            activation_seeds,
            window,
            cx,
        )
    }

    pub(in crate::main_window) fn prepare_pending_activation(
        service: &MainWindowConversationComposerService,
        receipt: MainWindowComposerActivationReceipt,
    ) -> Result<
        (
            MainWindowComposerSelectionIdentity,
            VecDeque<MainWindowConversationComposerActivationSeed>,
        ),
        String,
    > {
        let (selection, initial) = service.take_pending_initial_presentation(receipt)?;
        let activation_seeds = Self::activation_seeds(selection, initial)?;
        Ok((selection, activation_seeds))
    }

    pub(in crate::main_window) fn new_pending(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
        receipt: MainWindowComposerActivationReceipt,
        activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
        clipboard_writer: ComposerClipboardWriter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let selection = config.selection();
        if service.pending_identity(receipt) != Some(selection) {
            return Err("pending conversation composer selection is stale".to_owned());
        }
        Self::construct(
            config,
            service,
            clipboard_writer,
            MainWindowConversationComposerRoute::Pending(receipt),
            activation_seeds,
            window,
            cx,
        )
    }

    fn construct(
        config: MainWindowConversationComposerConfig,
        service: Arc<MainWindowConversationComposerService>,
        clipboard_writer: ComposerClipboardWriter,
        route: MainWindowConversationComposerRoute,
        activation_seeds: VecDeque<MainWindowConversationComposerActivationSeed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, String> {
        let selection = config.selection();
        let proof_limits = config.successor_proof_limits();
        let clipboard_limits = config.clipboard_limits();
        let mutation_limits = config.mutation_limits();
        let residency_bound = config.residency_bound()?;
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
            activation_seeds,
            clipboard_writer,
            proof_limits,
            clipboard_limits,
            mutation_limits,
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

    fn activation_seeds(
        selection: MainWindowComposerSelectionIdentity,
        initial: Box<[crate::composer_host::ComposerHostResponse]>,
    ) -> Result<VecDeque<MainWindowConversationComposerActivationSeed>, String> {
        let mut seeds = VecDeque::with_capacity(initial.len());
        let mut seeds_enabled = true;
        for response in initial {
            if response.key().binding() != selection.binding() {
                return Err("composer activation seed binding was corrupt".to_owned());
            }
            match response.value() {
                crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) => {
                    if candidate.binding() != selection.binding().candidate() {
                        return Err("composer activation seed candidate binding was corrupt".into());
                    }
                    if seeds_enabled {
                        seeds.push_back(MainWindowConversationComposerActivationSeed::Page(
                            response,
                        ));
                    }
                }
                crate::composer_host::ComposerHostResponseValue::CandidateMarkers(candidate) => {
                    if candidate.binding() != selection.binding().candidate() {
                        return Err("composer activation seed candidate binding was corrupt".into());
                    }
                    if seeds_enabled {
                        seeds.push_back(MainWindowConversationComposerActivationSeed::ObjectPage(
                            response,
                        ));
                    }
                }
                crate::composer_host::ComposerHostResponseValue::CandidateMarkerProof(
                    candidate,
                ) => {
                    if candidate.binding() != selection.binding().candidate() {
                        return Err("composer activation seed candidate binding was corrupt".into());
                    }
                    seeds_enabled = false;
                    seeds.clear();
                }
                _ => return Err("composer activation response was not activation-legal".to_owned()),
            }
        }
        Ok(seeds)
    }

    pub(super) fn apply_next_activation_seed(
        &mut self,
        request: RangeTextInputRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<RangeTextInputRequest>, String> {
        let Some(seed) = self.activation_seeds.front() else {
            return Ok(Some(request));
        };
        let compatible = match (seed, &request) {
            (
                MainWindowConversationComposerActivationSeed::Page(response),
                RangeTextInputRequest::Page(request),
            ) => Self::page_seed_compatible(response, request),
            (
                MainWindowConversationComposerActivationSeed::ObjectPage(response),
                RangeTextInputRequest::ObjectPage(request),
            ) => Self::object_seed_compatible(response, request),
            (_, RangeTextInputRequest::Page(_) | RangeTextInputRequest::ObjectPage(_)) => false,
            _ => return Ok(Some(request)),
        };
        if !compatible {
            self.activation_seeds.clear();
            return Ok(Some(request));
        }
        let response = match self.activation_seeds.pop_front().unwrap() {
            MainWindowConversationComposerActivationSeed::Page(response)
            | MainWindowConversationComposerActivationSeed::ObjectPage(response) => response,
        };
        let outcome =
            super::super::translate_initial_composer_response(self.selection, request, &response)
                .map_err(|_| "composer activation seed translation failed".to_owned())?;
        self.apply_page_or_object_outcome(outcome, window, cx)?;
        Ok(None)
    }

    fn page_seed_compatible(
        response: &crate::composer_host::ComposerHostResponse,
        request: &gpui_text_input::PageRequest,
    ) -> bool {
        let Some(purpose) = Self::page_purpose(request.key().purpose()) else {
            return false;
        };
        let crate::composer_host::ComposerHostResponseValue::CandidateText(candidate) =
            response.value()
        else {
            return false;
        };
        let result = candidate.value();
        let demand = match request.key().demand() {
            gpui_text_input::PageDemandEnvelope::Adjacent {
                anchor,
                direction: gpui_text_input::PageDirection::Forward,
                ..
            } => syndic_storage::DraftPieceTextDemandV1::Forward(anchor.get()),
            gpui_text_input::PageDemandEnvelope::Adjacent {
                anchor,
                direction: gpui_text_input::PageDirection::Backward,
                ..
            } => syndic_storage::DraftPieceTextDemandV1::Backward(anchor.get()),
            gpui_text_input::PageDemandEnvelope::Validation { candidate, .. } => {
                syndic_storage::DraftPieceTextDemandV1::Validate(candidate.get())
            }
        };
        response.key().purpose() == purpose
            && result.demand() == demand
            && u64::try_from(result.bytes().len())
                .is_ok_and(|bytes| bytes <= request.key().max_payload_bytes())
    }

    fn object_seed_compatible(
        response: &crate::composer_host::ComposerHostResponse,
        request: &gpui_text_input::ObjectRequest,
    ) -> bool {
        let Some(purpose) = Self::object_purpose(request.key().purpose()) else {
            return false;
        };
        let crate::composer_host::ComposerHostResponseValue::CandidateMarkers(candidate) =
            response.value()
        else {
            return false;
        };
        let result = candidate.value();
        let (scope, direction, cursor, max_objects, max_retained_bytes) =
            match request.key().demand() {
                gpui_text_input::ObjectDemandEnvelope::Range {
                    range,
                    cursor,
                    direction,
                    max_objects,
                    max_retained_bytes,
                } => (
                    syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange {
                        start: range.start().get(),
                        end: range.end().get(),
                    },
                    direction,
                    cursor,
                    max_objects,
                    max_retained_bytes,
                ),
                gpui_text_input::ObjectDemandEnvelope::Anchor {
                    anchor,
                    cursor,
                    direction,
                    max_objects,
                    max_retained_bytes,
                } => (
                    syndic_storage::DraftPieceMarkerScopeV1::ExactAnchor(anchor.get()),
                    direction,
                    cursor,
                    max_objects,
                    max_retained_bytes,
                ),
            };
        let direction = match direction {
            gpui_text_input::ObjectDirection::Forward => {
                syndic_storage::DraftPieceMarkerDirectionV1::Forward
            }
            gpui_text_input::ObjectDirection::Backward => {
                syndic_storage::DraftPieceMarkerDirectionV1::Backward
            }
        };
        response.key().purpose() == purpose
            && cursor.is_none()
            && result.scope() == scope
            && result.direction() == direction
            && result.markers().len() <= max_objects
            && result.retained_bytes() <= max_retained_bytes
    }

    fn page_purpose(
        purpose: PagePurpose,
    ) -> Option<crate::composer_host::ComposerHostRequestPurpose> {
        use crate::composer_host::ComposerHostRequestPurpose;
        Some(match purpose {
            PagePurpose::Viewport => ComposerHostRequestPurpose::Viewport,
            PagePurpose::Caret => ComposerHostRequestPurpose::Caret,
            PagePurpose::Selection | PagePurpose::PlatformRange => {
                ComposerHostRequestPurpose::Selection
            }
            PagePurpose::Segmentation => ComposerHostRequestPurpose::Segmentation,
            PagePurpose::Clipboard => ComposerHostRequestPurpose::Clipboard,
            PagePurpose::Restoration => ComposerHostRequestPurpose::Restoration,
            PagePurpose::GeometryIndex | PagePurpose::GeometryTarget => {
                ComposerHostRequestPurpose::Geometry
            }
            _ => return None,
        })
    }

    fn object_purpose(
        purpose: ObjectPurpose,
    ) -> Option<crate::composer_host::ComposerHostRequestPurpose> {
        use crate::composer_host::ComposerHostRequestPurpose;
        Some(match purpose {
            ObjectPurpose::Viewport => ComposerHostRequestPurpose::Viewport,
            ObjectPurpose::Caret => ComposerHostRequestPurpose::Caret,
            ObjectPurpose::Selection
            | ObjectPurpose::MutationSuccessor
            | ObjectPurpose::PlatformRange => ComposerHostRequestPurpose::Selection,
            ObjectPurpose::Clipboard => ComposerHostRequestPurpose::Clipboard,
            ObjectPurpose::Restoration => ComposerHostRequestPurpose::Restoration,
            ObjectPurpose::GeometryIndex | ObjectPurpose::GeometryTarget => {
                ComposerHostRequestPurpose::Geometry
            }
            _ => return None,
        })
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
