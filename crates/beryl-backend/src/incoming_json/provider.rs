mod capture;
mod dynamic_capture;
mod machine;
mod schema;
mod steering_capture;

use std::io::{self, BufRead, BufReader, Read};

use bounded_json::{Parser, Progress};

use self::machine::{Machine, MachineError};
use super::{DecodeReaderError, DecodeStats, DecodedValue, ResponseExpectationSlot};
use crate::{OrderedTurnStreamSink, turn::StreamedUserMessageVerifierHandle};

const CLASSIFICATION_PREFIX_BYTES: usize = 1024;
const STRUCTURED_DEPTH: usize = 128;
const ENVELOPE_DEPTH_OVERHEAD: usize = 16;
const JSON_DEPTH: usize = STRUCTURED_DEPTH + ENVELOPE_DEPTH_OVERHEAD;

pub(super) fn decode<'a, R: Read>(
    input: R,
    input_buffer_bytes: usize,
    verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    response_authority_generation: u64,
    response_expectation: &mut ResponseExpectationSlot,
) -> Result<DecodedValue, DecodeReaderError> {
    let result = decode_inner(
        input,
        input_buffer_bytes,
        verifier,
        sink,
        response_authority_generation,
        response_expectation,
    );
    if result.is_err() {
        // A valid rejection returns `Ok`, while a fully validated success for an unrestored family
        // completes its expectation before returning its typed gap. Every other decoder error is
        // connection-terminal and poisons any expectation that was still installed.
        response_expectation.poison();
    }
    result
}

fn decode_inner<'a, R: Read>(
    input: R,
    input_buffer_bytes: usize,
    verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    response_authority_generation: u64,
    response_expectation: &mut ResponseExpectationSlot,
) -> Result<DecodedValue, DecodeReaderError> {
    let mut input = BufReader::with_capacity(input_buffer_bytes.max(1), input);
    let mut parser = Parser::<JSON_DEPTH>::new();
    let mut machine = Machine::new(
        verifier,
        sink,
        response_authority_generation,
        response_expectation.current(),
    );
    let mut scratch = [0_u8; 256];
    let mut stats = DecodeStats::default();
    let mut classification_bytes = 0_usize;

    loop {
        machine.flush_full_page().map_err(map_machine_error)?;
        if machine.uses_classification_prefix()
            && classification_bytes == CLASSIFICATION_PREFIX_BYTES
        {
            machine.resolve_classification_prefix_pressure();
        }

        let available = match input.fill_buf() {
            Ok(available) => available,
            Err(source) => {
                machine.mark_transport_lost();
                return Err(json_io(source));
            }
        };
        stats.maximum_buffered_input_bytes =
            stats.maximum_buffered_input_bytes.max(available.len());
        let end_of_input = available.is_empty();
        let input_limit = if machine.uses_classification_prefix() {
            available
                .len()
                .min(CLASSIFICATION_PREFIX_BYTES - classification_bytes)
        } else {
            available.len()
        };
        let parser_input = &available[..input_limit];
        let was_classifying = machine.uses_classification_prefix();
        let direct = machine.uses_capture_output();
        let result = if direct {
            let output = machine.capture_output_window().map_err(map_machine_error)?;
            parser.advance(parser_input, output, end_of_input)
        } else {
            parser.advance(parser_input, &mut scratch, end_of_input)
        };

        let (consumed, produced, progress, failure) = match result {
            Ok(step) => (
                step.consumed(),
                step.produced(),
                Some(step.progress()),
                None,
            ),
            Err(failure) => (failure.consumed(), failure.produced(), None, Some(failure)),
        };

        // A failing parser call may still have decoded committed scalar bytes. Deliver them before
        // surfacing the terminal syntax error, exactly as the parser progress contract requires.
        if direct {
            machine
                .commit_capture_output(produced)
                .map_err(map_machine_error)?;
        } else {
            machine
                .commit_scratch_output(&scratch[..produced])
                .map_err(map_machine_error)?;
        }
        if was_classifying {
            classification_bytes = classification_bytes.saturating_add(consumed);
        }
        input.consume(consumed);
        stats.input_bytes = stats.input_bytes.saturating_add(consumed);

        if let Some(failure) = failure {
            machine.flush_capture_output().map_err(map_machine_error)?;
            response_expectation.poison();
            return Err(machine.map_parse_failure(failure));
        }

        let progress = progress.expect("successful parser call reports progress");
        match progress {
            Progress::Event(event) => {
                if let Err(error) = machine.event(event) {
                    response_expectation.poison();
                    return Err(map_machine_error(error));
                }
            }
            Progress::NeedInput => {
                if end_of_input {
                    return Err(json_error("bounded JSON parser requested input after EOF"));
                }
            }
            Progress::NeedOutput => {
                return Err(machine.map_output_pressure());
            }
            Progress::Complete => {
                let is_response = machine.is_response_message();
                let finished = if is_response {
                    complete_response(machine.finish(), response_expectation)
                } else {
                    machine.finish()
                };
                let incoming = finished.map_err(map_machine_error)?;
                let machine_stats = machine.stats();
                stats.discarded_image_result_bytes = machine_stats.discarded_image_result_bytes;
                stats.verified_user_text_wire_bytes = machine_stats.verified_user_text_wire_bytes;
                return Ok(DecodedValue { incoming, stats });
            }
        }
    }
}

fn complete_response(
    finished: Result<super::DecodedIncoming, MachineError>,
    response_expectation: &mut ResponseExpectationSlot,
) -> Result<super::DecodedIncoming, MachineError> {
    match finished {
        Ok(super::DecodedIncoming::Response { id, result }) => {
            response_expectation
                .complete_response(id, &result)
                .map_err(MachineError::from)?;
            Ok(super::DecodedIncoming::Response { id, result })
        }
        Ok(super::DecodedIncoming::Rejection { id, error }) => {
            response_expectation
                .complete_rejection(id)
                .map_err(MachineError::from)?;
            Ok(super::DecodedIncoming::Rejection { id, error })
        }
        Err(
            error @ MachineError::Envelope(
                super::ForegroundIngressError::ResponseFamilyUnavailable { .. },
            ),
        ) => {
            let completion = response_expectation
                .current()
                .ok_or(super::ForegroundIngressError::MalformedResponse)
                .and_then(|expectation| response_expectation.complete_unavailable(expectation.id));
            match completion {
                Ok(()) => Err(error),
                Err(source) => Err(source.into()),
            }
        }
        Ok(_) => {
            response_expectation.poison();
            Err(super::ForegroundIngressError::MalformedResponse.into())
        }
        Err(error) => {
            response_expectation.poison();
            Err(error)
        }
    }
}

fn map_machine_error(error: MachineError) -> DecodeReaderError {
    match error {
        MachineError::Provider(source) => DecodeReaderError::Provider(source),
        MachineError::Correlation(source) => DecodeReaderError::Correlation(source),
        MachineError::Steering(source) => DecodeReaderError::Steering(source),
        MachineError::IncompatibleEnvelopeOrder => {
            json_error("incoming JSON envelope does not match the pinned wire order")
        }
        MachineError::Approval { kind, source } => DecodeReaderError::Approval { kind, source },
        MachineError::DynamicTool(source) => DecodeReaderError::DynamicTool(source),
        MachineError::Ordered(source) => DecodeReaderError::Ordered(source),
        MachineError::OrderedUnexpectedCompletion => DecodeReaderError::OrderedUnexpectedCompletion,
        MachineError::Envelope(source) => DecodeReaderError::Envelope(source),
    }
}

fn json_io(source: io::Error) -> DecodeReaderError {
    DecodeReaderError::Json(serde_json::Error::io(source))
}

fn json_error(message: &'static str) -> DecodeReaderError {
    json_io(io::Error::new(io::ErrorKind::InvalidData, message))
}
