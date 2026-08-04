use std::{net::Ipv6Addr, str::FromStr};

use super::{is_sub_delim, is_unreserved};

#[derive(Clone, Copy, Debug)]
pub(super) struct AuthorityValidator {
    at_seen: bool,
    userinfo: UserInfoCandidate,
    host_port: HostPortCandidate,
    invalid: bool,
}

impl AuthorityValidator {
    pub(super) const fn new() -> Self {
        Self {
            at_seen: false,
            userinfo: UserInfoCandidate::new(),
            host_port: HostPortCandidate::new(),
            invalid: false,
        }
    }

    pub(super) fn push(&mut self, byte: u8) {
        if byte == b'@' {
            if self.at_seen || !self.userinfo.finish() {
                self.invalid = true;
            }
            self.at_seen = true;
            self.host_port = HostPortCandidate::new();
            return;
        }
        if !self.at_seen {
            self.userinfo.push(byte);
        }
        self.host_port.push(byte);
    }

    pub(super) fn finish(self) -> bool {
        !self.invalid && self.host_port.finish()
    }
}

#[derive(Clone, Copy, Debug)]
struct UserInfoCandidate {
    percent_digits_remaining: u8,
    invalid: bool,
}

impl UserInfoCandidate {
    const fn new() -> Self {
        Self {
            percent_digits_remaining: 0,
            invalid: false,
        }
    }

    fn push(&mut self, byte: u8) {
        if consume_component_percent(&mut self.percent_digits_remaining, &mut self.invalid, byte) {
            return;
        }
        if byte == b'%' {
            self.percent_digits_remaining = 2;
        } else if !(is_unreserved(byte) || is_sub_delim(byte) || byte == b':') {
            self.invalid = true;
        }
    }

    const fn finish(self) -> bool {
        !self.invalid && self.percent_digits_remaining == 0
    }
}

#[derive(Clone, Copy, Debug)]
enum HostPortState {
    RegName,
    IpLiteral(IpLiteralValidator),
    AfterIpLiteral,
    Port,
}

#[derive(Clone, Copy, Debug)]
struct HostPortCandidate {
    state: HostPortState,
    percent_digits_remaining: u8,
    reg_name_seen: bool,
    invalid: bool,
}

impl HostPortCandidate {
    const fn new() -> Self {
        Self {
            state: HostPortState::RegName,
            percent_digits_remaining: 0,
            reg_name_seen: false,
            invalid: false,
        }
    }

    fn push(&mut self, byte: u8) {
        match self.state {
            HostPortState::RegName => self.push_reg_name(byte),
            HostPortState::IpLiteral(mut literal) => {
                if byte == b']' {
                    if !literal.finish() {
                        self.invalid = true;
                    }
                    self.state = HostPortState::AfterIpLiteral;
                } else {
                    literal.push(byte);
                    self.state = HostPortState::IpLiteral(literal);
                }
            }
            HostPortState::AfterIpLiteral => {
                if byte == b':' {
                    self.state = HostPortState::Port;
                } else {
                    self.invalid = true;
                }
            }
            HostPortState::Port => {
                if !byte.is_ascii_digit() {
                    self.invalid = true;
                }
            }
        }
    }

    fn push_reg_name(&mut self, byte: u8) {
        if consume_component_percent(&mut self.percent_digits_remaining, &mut self.invalid, byte) {
            return;
        }
        match byte {
            b'[' => {
                if self.reg_name_seen || self.percent_digits_remaining != 0 {
                    self.invalid = true;
                }
                self.state = HostPortState::IpLiteral(IpLiteralValidator::new());
            }
            b':' => self.state = HostPortState::Port,
            b'%' => {
                self.reg_name_seen = true;
                self.percent_digits_remaining = 2;
            }
            _ if is_unreserved(byte) || is_sub_delim(byte) => self.reg_name_seen = true,
            _ => self.invalid = true,
        }
    }

    fn finish(self) -> bool {
        !self.invalid
            && self.percent_digits_remaining == 0
            && !matches!(self.state, HostPortState::IpLiteral(_))
    }
}

#[derive(Clone, Copy, Debug)]
enum IpLiteralState {
    Empty,
    Ipv6 {
        bytes: [u8; 45],
        length: u8,
        invalid: bool,
    },
    FutureVersion {
        digit_seen: bool,
        invalid: bool,
    },
    FutureAddress {
        character_seen: bool,
        invalid: bool,
    },
}

#[derive(Clone, Copy, Debug)]
struct IpLiteralValidator {
    state: IpLiteralState,
}

impl IpLiteralValidator {
    const fn new() -> Self {
        Self {
            state: IpLiteralState::Empty,
        }
    }

    fn push(&mut self, byte: u8) {
        self.state = match self.state {
            IpLiteralState::Empty if matches!(byte, b'v' | b'V') => IpLiteralState::FutureVersion {
                digit_seen: false,
                invalid: false,
            },
            IpLiteralState::Empty => append_ipv6_byte([0; 45], 0, false, byte),
            IpLiteralState::Ipv6 {
                bytes,
                length,
                invalid,
            } => append_ipv6_byte(bytes, length, invalid, byte),
            IpLiteralState::FutureVersion {
                mut digit_seen,
                mut invalid,
            } => {
                if byte.is_ascii_hexdigit() {
                    digit_seen = true;
                    IpLiteralState::FutureVersion {
                        digit_seen,
                        invalid,
                    }
                } else if byte == b'.' && digit_seen {
                    IpLiteralState::FutureAddress {
                        character_seen: false,
                        invalid,
                    }
                } else {
                    invalid = true;
                    IpLiteralState::FutureVersion {
                        digit_seen,
                        invalid,
                    }
                }
            }
            IpLiteralState::FutureAddress {
                mut character_seen,
                mut invalid,
            } => {
                if is_unreserved(byte) || is_sub_delim(byte) || byte == b':' {
                    character_seen = true;
                } else {
                    invalid = true;
                }
                IpLiteralState::FutureAddress {
                    character_seen,
                    invalid,
                }
            }
        };
    }

    fn finish(self) -> bool {
        match self.state {
            IpLiteralState::Ipv6 {
                bytes,
                length,
                invalid: false,
            } => std::str::from_utf8(&bytes[..usize::from(length)])
                .ok()
                .and_then(|value| Ipv6Addr::from_str(value).ok())
                .is_some(),
            IpLiteralState::FutureAddress {
                character_seen: true,
                invalid: false,
            } => true,
            IpLiteralState::Empty
            | IpLiteralState::Ipv6 { .. }
            | IpLiteralState::FutureVersion { .. }
            | IpLiteralState::FutureAddress { .. } => false,
        }
    }
}

fn append_ipv6_byte(
    mut bytes: [u8; 45],
    length: u8,
    mut invalid: bool,
    byte: u8,
) -> IpLiteralState {
    if !byte.is_ascii_hexdigit() && !matches!(byte, b':' | b'.') {
        invalid = true;
    }
    let index = usize::from(length);
    let next_length = if index < bytes.len() {
        bytes[index] = byte;
        length + 1
    } else {
        invalid = true;
        length
    };
    IpLiteralState::Ipv6 {
        bytes,
        length: next_length,
        invalid,
    }
}

fn consume_component_percent(remaining: &mut u8, invalid: &mut bool, byte: u8) -> bool {
    if *remaining == 0 {
        return false;
    }
    if byte.is_ascii_hexdigit() {
        *remaining -= 1;
    } else {
        *invalid = true;
        *remaining = 0;
    }
    true
}
