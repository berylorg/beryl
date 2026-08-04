impl<'a> TargetMachine<'a> {
    fn request_scoped_user_message(&self) -> bool {
        self.verifier.is_some()
    }

    fn steering_capture_mut(
        &mut self,
    ) -> Result<&mut SteeringUserMessageCapture<'a>, MachineError> {
        self.steering_capture
            .as_mut()
            .ok_or_else(|| SteeringUserMessageError::MissingOrMalformedCorrelation.into())
    }

    fn begin_steering_user_message(
        &mut self,
        client_user_message_id: &str,
    ) -> Result<(), MachineError> {
        if self.request_scoped_user_message() || self.steering_capture.is_some() {
            return Err(SteeringUserMessageError::MissingOrMalformedCorrelation.into());
        }
        let lifecycle = match self.top() {
            Frame::ItemUser { lifecycle, .. } => lifecycle,
            _ => return Err(SteeringUserMessageError::MissingOrMalformedCorrelation.into()),
        };
        let item_id = self
            .item_id
            .as_ref()
            .cloned()
            .ok_or(SteeringUserMessageError::MissingOrMalformedCorrelation)?;
        let client_user_message_id =
            crate::ClientUserMessageId::try_new(client_user_message_id)
                .map_err(|_| SteeringUserMessageError::MissingOrMalformedCorrelation)?;
        let sink = self
            .sink
            .take()
            .ok_or(crate::OrderedTurnStreamSubmitCause::Unavailable)?;
        self.steering_capture = Some(SteeringUserMessageCapture::begin(
            sink,
            lifecycle,
            item_id,
            client_user_message_id,
        )?);
        Ok(())
    }

    fn expected_user_item_count(&self) -> Result<u64, MachineError> {
        if self.request_scoped_user_message() {
            return Ok(self.verifier_ref()?.expected_item_count());
        }
        self.steering_capture
            .as_ref()
            .map(SteeringUserMessageCapture::expected_item_count)
            .ok_or_else(|| SteeringUserMessageError::MissingOrMalformedCorrelation.into())
    }

    fn begin_user_input(&mut self, index: u64) -> Result<&'static str, MachineError> {
        if self.request_scoped_user_message() {
            return self.verifier_ref()?.begin_input(index).map_err(Into::into);
        }
        self.steering_capture_mut()?
            .begin_input(index)
            .map_err(Into::into)
    }

    fn expected_user_image_detail(
        &mut self,
        index: u64,
    ) -> Result<Option<ImageDetail>, MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .expected_image_detail(index)
                .map_err(Into::into);
        }
        self.steering_capture
            .as_mut()
            .ok_or(SteeringUserMessageError::MissingOrMalformedCorrelation)?
            .expected_image_detail(index)
            .map_err(Into::into)
    }

    fn compare_user_text_bytes(
        &mut self,
        index: u64,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .compare_text_bytes(index, bytes)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .compare_text_bytes(index, bytes)
            .map_err(Into::into)
    }

    fn finish_user_text(&mut self, index: u64) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .finish_text(index)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .finish_text(index)
            .map_err(Into::into)
    }

    fn compare_user_image_path_bytes(
        &mut self,
        index: u64,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .compare_image_path_bytes(index, bytes)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .compare_image_path_bytes(index, bytes)
            .map_err(Into::into)
    }

    fn finish_user_image_path(&mut self, index: u64) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .finish_image_path(index)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .finish_image_path(index)
            .map_err(Into::into)
    }

    fn finish_user_input(&mut self, index: u64) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .finish_input(index)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .finish_input(index)
            .map_err(Into::into)
    }

    fn finish_user_content(&mut self, item_count: u64) -> Result<(), MachineError> {
        if self.request_scoped_user_message() {
            return self
                .verifier_ref()?
                .finish_lifecycle_content(item_count)
                .map_err(Into::into);
        }
        self.steering_capture_mut()?
            .finish_content(item_count)
            .map_err(Into::into)
    }

    fn mark_steering_route_failure(&mut self) {
        if let Some(capture) = self.steering_capture.as_mut() {
            capture.mark_route_failure();
        }
    }
}
