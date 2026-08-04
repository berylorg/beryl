use super::super::ProviderItemValidationError;

mod authority;

use authority::AuthorityValidator;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UriState {
    Scheme,
    AfterScheme,
    AfterFirstSlash,
    Authority,
    Path,
    Query,
    Fragment,
}

/// Fixed-state RFC 3986 validator shared by materialized and streaming image locators.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProviderImageLocatorValidatorV1 {
    scheme_length: usize,
    state: UriState,
    percent_digits_remaining: u8,
    authority: AuthorityValidator,
    invalid: bool,
    data_probe_position: u8,
    data_probe_possible: bool,
    data_scheme: bool,
}

impl ProviderImageLocatorValidatorV1 {
    pub(crate) const fn new() -> Self {
        Self {
            scheme_length: 0,
            state: UriState::Scheme,
            percent_digits_remaining: 0,
            authority: AuthorityValidator::new(),
            invalid: false,
            data_probe_position: 0,
            data_probe_possible: true,
            data_scheme: false,
        }
    }

    pub(crate) fn validate(bytes: &[u8]) -> Result<(), ProviderItemValidationError> {
        let mut validator = Self::new();
        validator.push(bytes);
        validator.finish()
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.observe_data_scheme(byte);
            self.observe_uri(byte);
        }
    }

    pub(crate) fn finish(mut self) -> Result<(), ProviderItemValidationError> {
        if self.data_scheme {
            return Err(ProviderItemValidationError::DynamicImageDataUrlRequiresAsset);
        }
        if self.state == UriState::Authority && !self.authority.finish() {
            self.invalid = true;
        }
        if self.invalid
            || self.state == UriState::Scheme
            || self.scheme_length == 0
            || self.percent_digits_remaining != 0
        {
            return Err(ProviderItemValidationError::InvalidDynamicImageLocator);
        }
        Ok(())
    }

    fn observe_data_scheme(&mut self, byte: u8) {
        const DATA_SCHEME: &[u8; 5] = b"data:";
        if !self.data_probe_possible || self.data_scheme {
            return;
        }
        if self.data_probe_position == 0 && byte.is_ascii_whitespace() {
            return;
        }
        let expected = DATA_SCHEME[usize::from(self.data_probe_position)];
        if byte.eq_ignore_ascii_case(&expected) {
            self.data_probe_position += 1;
            self.data_scheme = usize::from(self.data_probe_position) == DATA_SCHEME.len();
        } else {
            self.data_probe_possible = false;
        }
    }

    fn observe_uri(&mut self, byte: u8) {
        if !byte.is_ascii() || byte.is_ascii_whitespace() || byte.is_ascii_control() {
            self.invalid = true;
            return;
        }
        match self.state {
            UriState::Scheme => self.observe_scheme(byte),
            UriState::AfterScheme => self.observe_after_scheme(byte),
            UriState::AfterFirstSlash => self.observe_after_first_slash(byte),
            UriState::Authority => self.observe_authority(byte),
            UriState::Path => self.observe_path(byte),
            UriState::Query => self.observe_query(byte),
            UriState::Fragment => self.observe_fragment(byte),
        }
    }

    fn observe_scheme(&mut self, byte: u8) {
        if self.scheme_length == 0 {
            if byte.is_ascii_alphabetic() {
                self.scheme_length = 1;
            } else {
                self.invalid = true;
            }
        } else if byte == b':' {
            self.state = UriState::AfterScheme;
        } else if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.') {
            self.scheme_length = self.scheme_length.saturating_add(1);
        } else {
            self.invalid = true;
        }
    }

    fn observe_after_scheme(&mut self, byte: u8) {
        match byte {
            b'/' => self.state = UriState::AfterFirstSlash,
            b'?' => self.state = UriState::Query,
            b'#' => self.state = UriState::Fragment,
            _ => {
                self.state = UriState::Path;
                self.observe_path(byte);
            }
        }
    }

    fn observe_after_first_slash(&mut self, byte: u8) {
        if byte == b'/' {
            self.authority = AuthorityValidator::new();
            self.state = UriState::Authority;
        } else {
            self.state = UriState::Path;
            self.observe_path(byte);
        }
    }

    fn observe_authority(&mut self, byte: u8) {
        match byte {
            b'/' => {
                self.finish_authority();
                self.state = UriState::Path;
            }
            b'?' => {
                self.finish_authority();
                self.state = UriState::Query;
            }
            b'#' => {
                self.finish_authority();
                self.state = UriState::Fragment;
            }
            _ => self.authority.push(byte),
        }
    }

    fn finish_authority(&mut self) {
        if !self.authority.finish() {
            self.invalid = true;
        }
    }

    fn observe_path(&mut self, byte: u8) {
        if self.consume_percent_digit(byte) {
            return;
        }
        match byte {
            b'%' => self.percent_digits_remaining = 2,
            b'?' => self.state = UriState::Query,
            b'#' => self.state = UriState::Fragment,
            _ if is_pchar(byte) || byte == b'/' => {}
            _ => self.invalid = true,
        }
    }

    fn observe_query(&mut self, byte: u8) {
        if self.consume_percent_digit(byte) {
            return;
        }
        match byte {
            b'%' => self.percent_digits_remaining = 2,
            b'#' => self.state = UriState::Fragment,
            _ if is_pchar(byte) || matches!(byte, b'/' | b'?') => {}
            _ => self.invalid = true,
        }
    }

    fn observe_fragment(&mut self, byte: u8) {
        if self.consume_percent_digit(byte) {
            return;
        }
        match byte {
            b'%' => self.percent_digits_remaining = 2,
            _ if is_pchar(byte) || matches!(byte, b'/' | b'?') => {}
            _ => self.invalid = true,
        }
    }

    fn consume_percent_digit(&mut self, byte: u8) -> bool {
        if self.percent_digits_remaining == 0 {
            return false;
        }
        if byte.is_ascii_hexdigit() {
            self.percent_digits_remaining -= 1;
        } else {
            self.invalid = true;
            self.percent_digits_remaining = 0;
        }
        true
    }
}

const fn is_pchar(byte: u8) -> bool {
    is_unreserved(byte) || is_sub_delim(byte) || matches!(byte, b':' | b'@')
}

pub(super) const fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub(super) const fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}
