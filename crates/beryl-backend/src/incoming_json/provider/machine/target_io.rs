impl<'a> TargetMachine<'a> {
    fn new(
        method: TargetMethod,
        verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
        sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    ) -> Self {
        let mut frames = [Frame::Unused; STACK_CAPACITY];
        frames[0] = Frame::Root { params_seen: false };
        Self {
            method,
            verifier,
            sink,
            capture: None,
            steering_capture: None,
            frames,
            depth: 1,
            expected: None,
            scalar: ScalarHandler::None,
            thread_id: None,
            turn_id: None,
            item_id: None,
            timestamp: None,
            stats: DecodeStats::default(),
            root_complete: false,
        }
    }

    fn uses_capture_output(&self) -> bool {
        matches!(
            self.scalar,
            ScalarHandler::Stream { .. }
                | ScalarHandler::McpKey {
                    streaming: true,
                    ..
                }
                | ScalarHandler::McpType {
                    streaming: true,
                    ..
                }
        )
    }

    fn capture_output_window(&mut self) -> Result<&mut [u8], MachineError> {
        self.capture_mut()?.output_window().map_err(Into::into)
    }

    fn commit_capture_output(&mut self, produced: usize) -> Result<(), MachineError> {
        if produced == 0 {
            return Ok(());
        }
        self.capture_mut()?.commit_output(produced)?;
        Ok(())
    }

    fn flush_full_page(&mut self) -> Result<(), MachineError> {
        if self.uses_capture_output() {
            self.capture_mut()?.flush_if_full()?;
        }
        Ok(())
    }

    fn flush_capture_output(&mut self) -> Result<(), MachineError> {
        if let Some(capture) = self.capture.as_mut() {
            capture.flush_nonempty()?;
        }
        Ok(())
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let handler = std::mem::replace(&mut self.scalar, ScalarHandler::None);
        self.scalar = match handler {
            ScalarHandler::Fixed {
                purpose,
                bytes: mut fixed,
                after,
            } => {
                if matches!(purpose, FixedPurpose::Identity(RouteValue::ItemId))
                    && self.capture.is_some()
                {
                    self.capture_mut()?.write_fixed(bytes)?;
                }
                fixed.push(bytes)?;
                ScalarHandler::Fixed {
                    purpose,
                    bytes: fixed,
                    after,
                }
            }
            ScalarHandler::ThreadId {
                context,
                end,
                bytes: mut fixed,
            } => {
                self.capture_mut()?.write_fixed(bytes)?;
                fixed
                    .push(bytes)
                    .map_err(|_| ProviderObservationSchemaError::InvalidIdentity)?;
                ScalarHandler::ThreadId {
                    context,
                    end,
                    bytes: fixed,
                }
            }
            ScalarHandler::Number {
                purpose,
                mut number,
                after,
            } => {
                number.push(bytes);
                ScalarHandler::Number {
                    purpose,
                    number,
                    after,
                }
            }
            ScalarHandler::Discard { reason, after } => {
                match reason {
                    DiscardReason::ImageResult => {
                        self.stats.discarded_image_result_bytes = self
                            .stats
                            .discarded_image_result_bytes
                            .saturating_add(bytes.len());
                    }
                    DiscardReason::ReasoningText => {}
                    DiscardReason::OtherPayload => {}
                }
                ScalarHandler::Discard { reason, after }
            }
            ScalarHandler::WebAction(mut probe) => {
                probe.push(bytes);
                ScalarHandler::WebAction(probe)
            }
            ScalarHandler::OtherName(mut probe) => {
                probe.push(bytes);
                ScalarHandler::OtherName(probe)
            }
            ScalarHandler::UserText {
                index,
                after,
            } => {
                self.compare_user_text_bytes(index, bytes)?;
                self.stats.verified_user_text_wire_bytes = self
                    .stats
                    .verified_user_text_wire_bytes
                    .saturating_add(bytes.len());
                ScalarHandler::UserText { index, after }
            }
            ScalarHandler::UserPath {
                index,
                after,
            } => {
                self.compare_user_image_path_bytes(index, bytes)?;
                ScalarHandler::UserPath { index, after }
            }
            ScalarHandler::McpKey {
                root,
                depth,
                entry,
                bytes: fixed,
                streaming,
            } => self.mcp_key_bytes(root, depth, entry, fixed, streaming, bytes)?,
            ScalarHandler::McpType {
                root,
                depth,
                entry,
                bytes: fixed,
                streaming,
            } => self.mcp_type_bytes(root, depth, entry, fixed, streaming, bytes)?,
            ScalarHandler::None if bytes.is_empty() => ScalarHandler::None,
            ScalarHandler::None => {
                return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
            }
            ScalarHandler::Stream { .. } => {
                unreachable!("streaming scalar writes directly into its page lease")
            }
        };
        Ok(())
    }

    fn event(&mut self, event: Event) -> Result<(), MachineError> {
        if matches!(self.top(), Frame::OtherDiscard { .. }) {
            return self.other_discard_event(event);
        }
        match event {
            Event::ContainerStart(kind) => self.start_container(kind),
            Event::ContainerEnd(kind) => self.end_container(kind),
            Event::ScalarStart(kind) => self.start_scalar(kind),
            Event::ScalarFragment(kind) => self.scalar_fragment(kind),
            Event::ScalarEnd(kind) => self.end_scalar(kind),
            Event::Boolean(value) => self.literal(ProviderScalar::Boolean(value)),
            Event::Null => self.literal(ProviderScalar::Null),
        }
    }

    fn scalar_fragment(&self, kind: ScalarKind) -> Result<(), MachineError> {
        let valid = matches!(
            (&self.scalar, kind),
            (
                ScalarHandler::Fixed { .. },
                ScalarKind::Name | ScalarKind::String
            ) | (
                ScalarHandler::Stream { .. },
                ScalarKind::Name | ScalarKind::String
            ) | (ScalarHandler::Number { .. }, ScalarKind::Number)
                | (ScalarHandler::Discard { .. }, ScalarKind::String)
                | (
                    ScalarHandler::Discard { .. },
                    ScalarKind::Name | ScalarKind::Number
                )
                | (ScalarHandler::WebAction(_), ScalarKind::String)
                | (ScalarHandler::OtherName(_), ScalarKind::Name)
                | (ScalarHandler::UserText { .. }, ScalarKind::String)
                | (ScalarHandler::UserPath { .. }, ScalarKind::String)
                | (ScalarHandler::McpKey { .. }, ScalarKind::Name)
                | (ScalarHandler::McpType { .. }, ScalarKind::String)
                | (ScalarHandler::ThreadId { .. }, ScalarKind::String)
        );
        if valid {
            Ok(())
        } else {
            Err(ProviderObservationSchemaError::WrongType.into())
        }
    }

    fn capture_mut(&mut self) -> Result<&mut ObservationCapture<'a>, MachineError> {
        self.capture
            .as_mut()
            .ok_or_else(|| OrderedTurnStreamSubmitCause::Unavailable.into())
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), MachineError> {
        if self.depth == self.frames.len() {
            return Err(ProviderObservationSchemaError::StructuredDepthExceeded.into());
        }
        self.frames[self.depth] = frame;
        self.depth += 1;
        Ok(())
    }

    fn pop_frame(&mut self) -> Frame {
        self.depth -= 1;
        let frame = self.frames[self.depth];
        self.frames[self.depth] = Frame::Unused;
        frame
    }

    fn top(&self) -> Frame {
        self.frames[self.depth - 1]
    }

    fn set_top(&mut self, frame: Frame) {
        self.frames[self.depth - 1] = frame;
    }
}
