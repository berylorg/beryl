// At the worst subnormal denominator, binary64 round-to-nearest boundaries are
// `(2k + 1) * 2^-1075` with `2k + 1 < 2^53`. Their terminating decimal numerator is
// `(2k + 1) * 5^1075`, so `ceil(log10(2^53 * 5^1075)) = 768` significant digits.
// Minimum-normal binade coefficients may use one additional bit but retain the same 768-digit
// ceiling; integral and finite-to-infinity boundaries need fewer digits. Retaining 768 digits
// therefore makes each discarded-tail decimal cell boundary-free in its interior. If the tail
// is nonzero, the exact value and the appended-`1` representative below occupy that same open
// rounding cell; if the tail is all zero, the retained prefix is already exact.
const RETAINED_DECIMAL_DIGITS: usize = 768;
const NORMALIZED_DECIMAL_BYTES: usize = RETAINED_DECIMAL_DIGITS + 32;

struct NumberAccumulator {
    digits: [u8; RETAINED_DECIMAL_DIGITS],
    retained_digits: usize,
    discarded_nonzero: bool,
    negative: bool,
    float: bool,
    in_exponent: bool,
    exponent_negative: bool,
    exponent: usize,
    exponent_overflow: bool,
    integer: u64,
    integer_overflow: bool,
    integer_digits: usize,
    leading_zeroes: usize,
}

impl NumberAccumulator {
    const fn new() -> Self {
        Self {
            digits: [0; RETAINED_DECIMAL_DIGITS],
            retained_digits: 0,
            discarded_nonzero: false,
            negative: false,
            float: false,
            in_exponent: false,
            exponent_negative: false,
            exponent: 0,
            exponent_overflow: false,
            integer: 0,
            integer_overflow: false,
            integer_digits: 0,
            leading_zeroes: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            match *byte {
                b'-' if self.integer_digits == 0 && !self.float => self.negative = true,
                b'-' if self.in_exponent => self.exponent_negative = true,
                b'+' if self.in_exponent => {}
                b'.' => self.float = true,
                b'e' | b'E' => {
                    self.float = true;
                    self.in_exponent = true;
                }
                digit @ b'0'..=b'9' if self.in_exponent => {
                    let Some(exponent) = self
                        .exponent
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(usize::from(digit - b'0')))
                    else {
                        self.exponent_overflow = true;
                        continue;
                    };
                    self.exponent = exponent;
                }
                digit @ b'0'..=b'9' => {
                    let value = u64::from(digit - b'0');
                    if !self.float {
                        self.integer_digits = self.integer_digits.saturating_add(1);
                        self.integer = match self
                            .integer
                            .checked_mul(10)
                            .and_then(|integer| integer.checked_add(value))
                        {
                            Some(value) => value,
                            None => {
                                self.integer_overflow = true;
                                self.integer
                            }
                        };
                    }
                    if self.retained_digits == 0 && digit == b'0' {
                        self.leading_zeroes = self.leading_zeroes.saturating_add(1);
                    } else if self.retained_digits < self.digits.len() {
                        self.digits[self.retained_digits] = digit;
                        self.retained_digits += 1;
                    } else if digit != b'0' {
                        self.discarded_nonzero = true;
                    }
                }
                _ => unreachable!("bounded-json emitted a validated number spelling"),
            }
        }
    }

    fn finish(self) -> Result<ProviderScalar, ProviderObservationSchemaError> {
        if !self.float && !self.integer_overflow {
            if self.negative {
                let limit = (i64::MAX as u64) + 1;
                if self.integer == limit {
                    Ok(ProviderScalar::Signed(i64::MIN))
                } else if self.integer < limit {
                    Ok(ProviderScalar::Signed(-(self.integer as i64)))
                } else {
                    self.finish_finite_float()
                }
            } else {
                Ok(ProviderScalar::Unsigned(self.integer))
            }
        } else {
            self.finish_finite_float()
        }
    }

    fn finish_finite_float(&self) -> Result<ProviderScalar, ProviderObservationSchemaError> {
        let value = self.parse_float()?;
        ProviderFiniteF64::new(value)
            .map(ProviderScalar::FiniteFloat)
            .ok_or(ProviderObservationSchemaError::WrongType)
    }

    fn parse_float(&self) -> Result<f64, ProviderObservationSchemaError> {
        let mut normalized = [0_u8; NORMALIZED_DECIMAL_BYTES];
        let mut length = 0;
        if self.negative {
            push_normalized(&mut normalized, &mut length, b'-')?;
        }
        if self.retained_digits == 0 {
            push_normalized(&mut normalized, &mut length, b'0')?;
        } else {
            push_normalized(&mut normalized, &mut length, self.digits[0])?;
            if self.retained_digits > 1 || self.discarded_nonzero {
                push_normalized(&mut normalized, &mut length, b'.')?;
                for digit in &self.digits[1..self.retained_digits] {
                    push_normalized(&mut normalized, &mut length, *digit)?;
                }
                if self.discarded_nonzero {
                    // Every binary64 rounding boundary has at most 768 significant decimal
                    // digits. A nonzero tail therefore lies in one boundary-free open decimal
                    // cell; this representative has the same correctly rounded result.
                    push_normalized(&mut normalized, &mut length, b'1')?;
                }
            }
        }
        push_normalized(&mut normalized, &mut length, b'e')?;
        let scientific = self.scientific_exponent();
        push_i64(&mut normalized, &mut length, scientific)?;
        std::str::from_utf8(&normalized[..length])
            .map_err(|_| ProviderObservationSchemaError::WrongType)?
            .parse::<f64>()
            .map_err(|_| ProviderObservationSchemaError::WrongType)
    }

    fn scientific_exponent(&self) -> i64 {
        if self.retained_digits == 0 {
            return 0;
        }
        let base = if self.integer_digits > self.leading_zeroes {
            SignedMagnitude::positive(self.integer_digits - self.leading_zeroes - 1)
        } else {
            SignedMagnitude::negative(
                self.leading_zeroes
                    .saturating_sub(self.integer_digits)
                    .saturating_add(1),
            )
        };
        if self.exponent_overflow {
            return if self.exponent_negative {
                -10_000
            } else {
                10_000
            };
        }
        base.add(SignedMagnitude {
            negative: self.exponent_negative,
            magnitude: self.exponent,
        })
        .clamped_i64()
    }
}

#[derive(Clone, Copy)]
struct SignedMagnitude {
    negative: bool,
    magnitude: usize,
}

impl SignedMagnitude {
    const fn positive(magnitude: usize) -> Self {
        Self {
            negative: false,
            magnitude,
        }
    }

    const fn negative(magnitude: usize) -> Self {
        Self {
            negative: true,
            magnitude,
        }
    }

    fn add(self, other: Self) -> Self {
        if self.negative == other.negative {
            return Self {
                negative: self.negative,
                magnitude: self.magnitude.saturating_add(other.magnitude),
            };
        }
        if self.magnitude >= other.magnitude {
            Self {
                negative: self.negative,
                magnitude: self.magnitude - other.magnitude,
            }
        } else {
            Self {
                negative: other.negative,
                magnitude: other.magnitude - self.magnitude,
            }
        }
    }

    fn clamped_i64(self) -> i64 {
        let magnitude = self.magnitude.min(10_000) as i64;
        if self.negative { -magnitude } else { magnitude }
    }
}

fn push_normalized(
    output: &mut [u8; NORMALIZED_DECIMAL_BYTES],
    length: &mut usize,
    byte: u8,
) -> Result<(), ProviderObservationSchemaError> {
    let slot = output
        .get_mut(*length)
        .ok_or(ProviderObservationSchemaError::WrongType)?;
    *slot = byte;
    *length += 1;
    Ok(())
}

fn push_i64(
    output: &mut [u8; NORMALIZED_DECIMAL_BYTES],
    length: &mut usize,
    value: i64,
) -> Result<(), ProviderObservationSchemaError> {
    if value.is_negative() {
        push_normalized(output, length, b'-')?;
    }
    let mut digits = [0_u8; 20];
    let mut count = 0;
    let mut magnitude = value.unsigned_abs();
    loop {
        digits[count] = b'0' + (magnitude % 10) as u8;
        count += 1;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    for digit in digits[..count].iter().rev() {
        push_normalized(output, length, *digit)?;
    }
    Ok(())
}

impl TargetMachine<'_> {
    fn finish_route_number(
        &mut self,
        route: RouteValue,
        value: ProviderScalar,
    ) -> Result<(), MachineError> {
        match route {
            RouteValue::Timestamp => {
                let ProviderScalar::Unsigned(value) = value else {
                    return Err(ProviderObservationSchemaError::InvalidIndex.into());
                };
                self.timestamp = Some(value);
            }
            RouteValue::Unsigned(field, width) => {
                let ProviderScalar::Unsigned(value) = value else {
                    return Err(ProviderObservationSchemaError::InvalidIndex.into());
                };
                if matches!(width, IntegerWidth::Bits32) && u32::try_from(value).is_err() {
                    return Err(ProviderObservationSchemaError::WrongType.into());
                }
                self.capture_mut()?
                    .control(ProviderObservationControl::Scalar {
                        context: ProviderValueContext::Field(field),
                        value: ProviderScalar::Unsigned(value),
                    })?;
            }
            RouteValue::Signed(field, width) => {
                let value = match value {
                    ProviderScalar::Signed(value) => value,
                    ProviderScalar::Unsigned(value) => i64::try_from(value)
                        .map_err(|_| ProviderObservationSchemaError::WrongType)?,
                    ProviderScalar::FiniteFloat(_)
                    | ProviderScalar::Boolean(_)
                    | ProviderScalar::Null => {
                        return Err(ProviderObservationSchemaError::WrongType.into());
                    }
                };
                if matches!(width, IntegerWidth::Bits32) && i32::try_from(value).is_err() {
                    return Err(ProviderObservationSchemaError::WrongType.into());
                }
                self.capture_mut()?
                    .control(ProviderObservationControl::Scalar {
                        context: ProviderValueContext::Field(field),
                        value: ProviderScalar::Signed(value),
                    })?;
            }
            RouteValue::ThreadId | RouteValue::TurnId | RouteValue::ItemId => {
                return Err(ProviderObservationSchemaError::WrongType.into());
            }
        }
        Ok(())
    }

    fn map_parse_failure(&mut self, failure: ParseFailure) -> DecodeReaderError {
        use bounded_json::ErrorKind;
        let schema = match failure.error().kind() {
            ErrorKind::InvalidUtf8 | ErrorKind::InvalidEscape | ErrorKind::UnpairedSurrogate => {
                ProviderObservationSchemaError::InvalidString
            }
            ErrorKind::DepthExceeded => ProviderObservationSchemaError::StructuredDepthExceeded,
            ErrorKind::UnexpectedByte
            | ErrorKind::InvalidNumber
            | ErrorKind::IncompleteDocument
            | ErrorKind::TrailingContent
            | ErrorKind::PositionOverflow
            | ErrorKind::InvalidApiUse => ProviderObservationSchemaError::EnvelopeShape,
        };
        DecodeReaderError::Provider(schema.into())
    }

    fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if !self.root_complete || self.depth != 0 || self.expected.is_some() {
            return Err(ProviderObservationSchemaError::EnvelopeShape.into());
        }
        let thread_id = self
            .thread_id
            .take()
            .ok_or(ProviderObservationSchemaError::MissingOrMalformedRoute)?;
        let turn_id = self
            .turn_id
            .take()
            .ok_or(ProviderObservationSchemaError::MissingOrMalformedRoute)?;
        let item_id = self
            .item_id
            .take()
            .ok_or(ProviderObservationSchemaError::MissingField)?;
        match self.method {
            TargetMethod::Lifecycle(lifecycle) if self.capture.is_none() => {
                let timestamp = self
                    .timestamp
                    .ok_or(ProviderObservationSchemaError::MissingOrMalformedRoute)?;
                if let Some(capture) = self.steering_capture.take() {
                    drop(item_id);
                    capture.seal(
                        thread_id,
                        turn_id,
                        crate::ItemLifecycleTimestampMs::new(timestamp),
                    )?;
                    return Ok(DecodedIncoming::OrderedHandled);
                }
                let lifecycle = user_lifecycle(lifecycle);
                let notification = {
                    let verifier = self
                        .verifier
                        .as_ref()
                        .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
                    verifier.lock()?.commit_lifecycle(
                        lifecycle,
                        thread_id,
                        turn_id,
                        item_id,
                        crate::ItemLifecycleTimestampMs::new(timestamp),
                    )?
                };
                let sink = self
                    .sink
                    .as_deref_mut()
                    .ok_or(OrderedTurnStreamSubmitCause::Unavailable)?;
                match sink.submit(OrderedTurnStreamOperation::CheckedUserMessage(notification)) {
                    Ok(OrderedTurnStreamCompletion::Applied) => Ok(DecodedIncoming::OrderedHandled),
                    Ok(_) => Err(MachineError::OrderedUnexpectedCompletion),
                    Err(source) => Err(MachineError::Ordered(Box::new(source))),
                }
            }
            _ => {
                let route = ProviderObservationRoute::new(thread_id, turn_id);
                let capture = self
                    .capture
                    .take()
                    .ok_or(crate::OrderedTurnStreamSubmitCause::Unavailable)?;
                drop(item_id);
                capture.seal(route)?;
                Ok(DecodedIncoming::OrderedHandled)
            }
        }
    }
}
