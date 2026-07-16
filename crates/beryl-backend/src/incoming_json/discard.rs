use std::{
    cell::RefCell,
    io::{self, BufRead, BufReader, Read},
    rc::Rc,
};

use super::DecodeStats;

#[derive(Clone, Default)]
pub(super) struct DiscardController {
    shared: Rc<RefCell<DiscardShared>>,
}

#[derive(Default)]
struct DiscardShared {
    state: DiscardState,
    stats: DecodeStats,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DiscardState {
    #[default]
    Idle,
    BeforeColon,
    AfterColon,
    EmitClosingQuote,
}

impl DiscardController {
    pub(super) fn arm_image_result<E>(&self) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        let mut shared = self.shared.borrow_mut();
        if shared.state != DiscardState::Idle {
            return Err(E::custom(
                "imageGeneration result discard was already active",
            ));
        }
        shared.state = DiscardState::BeforeColon;
        Ok(())
    }

    pub(super) fn require_image_result_discarded<E>(&self) -> Result<(), E>
    where
        E: serde::de::Error,
    {
        if self.shared.borrow().state == DiscardState::Idle {
            Ok(())
        } else {
            Err(E::custom(
                "imageGeneration result did not cross the bounded discard boundary",
            ))
        }
    }

    pub(super) fn stats(&self) -> DecodeStats {
        self.shared.borrow().stats
    }

    fn state(&self) -> DiscardState {
        self.shared.borrow().state
    }

    fn set_state(&self, state: DiscardState) {
        self.shared.borrow_mut().state = state;
    }

    fn note_buffered_input(&self, bytes: usize) {
        let mut shared = self.shared.borrow_mut();
        shared.stats.maximum_buffered_input_bytes =
            shared.stats.maximum_buffered_input_bytes.max(bytes);
    }

    fn note_discarded_bytes(&self, bytes: usize) {
        let mut shared = self.shared.borrow_mut();
        shared.stats.discarded_image_result_bytes = shared
            .stats
            .discarded_image_result_bytes
            .saturating_add(bytes);
    }
}

pub(super) struct DiscardingReader<R> {
    input: BufReader<R>,
    controller: DiscardController,
}

impl<R> DiscardingReader<R>
where
    R: Read,
{
    pub(super) fn new(input: R, input_buffer_bytes: usize, controller: DiscardController) -> Self {
        Self {
            input: BufReader::with_capacity(input_buffer_bytes.max(1), input),
            controller,
        }
    }

    fn next_input_byte(&mut self) -> io::Result<Option<u8>> {
        let available = self.input.fill_buf()?;
        self.controller.note_buffered_input(available.len());
        let Some(byte) = available.first().copied() else {
            return Ok(None);
        };
        self.input.consume(1);
        Ok(Some(byte))
    }

    fn required_input_byte(&mut self, context: &'static str) -> io::Result<u8> {
        self.next_input_byte()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("incoming JSON ended while {context}"),
            )
        })
    }

    fn discard_string_body(&mut self) -> io::Result<usize> {
        let mut discarded = 1_usize;
        loop {
            let byte = self.required_input_byte("discarding imageGeneration result")?;
            discarded = discarded.saturating_add(1);
            match byte {
                b'"' => return Ok(discarded),
                b'\\' => {
                    let escape =
                        self.required_input_byte("discarding an imageGeneration result escape")?;
                    discarded = discarded.saturating_add(1);
                    match escape {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                        b'u' => {
                            for _ in 0..4 {
                                let digit = self.required_input_byte(
                                    "discarding an imageGeneration unicode escape",
                                )?;
                                discarded = discarded.saturating_add(1);
                                if !digit.is_ascii_hexdigit() {
                                    return Err(invalid_data(
                                        "imageGeneration result had an invalid unicode escape",
                                    ));
                                }
                            }
                        }
                        _ => {
                            return Err(invalid_data(
                                "imageGeneration result had an invalid JSON escape",
                            ));
                        }
                    }
                }
                0x00..=0x1f => {
                    return Err(invalid_data(
                        "imageGeneration result had an unescaped control byte",
                    ));
                }
                0x80..=0xff => {
                    return Err(invalid_data(
                        "imageGeneration result was not an ASCII base64 string",
                    ));
                }
                _ => {}
            }
        }
    }

    fn read_filtered_byte(&mut self) -> io::Result<Option<u8>> {
        loop {
            match self.controller.state() {
                DiscardState::Idle => return self.next_input_byte(),
                DiscardState::BeforeColon => {
                    let byte =
                        self.required_input_byte("locating the imageGeneration result value")?;
                    if byte.is_ascii_whitespace() {
                        return Ok(Some(byte));
                    }
                    if byte != b':' {
                        return Err(invalid_data(
                            "imageGeneration result key was not followed by a colon",
                        ));
                    }
                    self.controller.set_state(DiscardState::AfterColon);
                    return Ok(Some(byte));
                }
                DiscardState::AfterColon => {
                    let byte =
                        self.required_input_byte("locating the imageGeneration result string")?;
                    if byte.is_ascii_whitespace() {
                        return Ok(Some(byte));
                    }
                    if byte != b'"' {
                        return Err(invalid_data("imageGeneration result was not a JSON string"));
                    }
                    let discarded = self.discard_string_body()?;
                    self.controller.note_discarded_bytes(discarded);
                    self.controller.set_state(DiscardState::EmitClosingQuote);
                    return Ok(Some(b'"'));
                }
                DiscardState::EmitClosingQuote => {
                    self.controller.set_state(DiscardState::Idle);
                    return Ok(Some(b'"'));
                }
            }
        }
    }
}

impl<R> Read for DiscardingReader<R>
where
    R: Read,
{
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        match self.read_filtered_byte()? {
            Some(byte) => {
                output[0] = byte;
                Ok(1)
            }
            None => Ok(0),
        }
    }
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
