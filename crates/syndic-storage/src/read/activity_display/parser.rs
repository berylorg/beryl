mod actions;
use super::{
    ActivityDisplayReadError as Error, ActivityDisplayValue, ActivityFileChangeDisplay,
    source::Window,
    text::{Diff, Prefix, Utf8},
};
use crate::{
    ProviderFrameObservationSummaryV1, ProviderItemKind, SealedProviderFrameReference,
    SyndicStorage,
};
use beryl_home_store::HomeStore;
use beryl_model::SyndicContentId;
use sha2::{Digest, Sha256};

#[derive(Default)]
pub(super) struct Facts {
    pub(super) failed: bool,
    command: Prefix,
    reasoning: Prefix,
    tool: Prefix,
    reason_index: Option<u64>,
    changes: u64,
    diff: Diff,
    path: Prefix,
    sole_path: Option<Vec<u8>>,
    path_seen: bool,
    path_valid: bool,
    utf8: Utf8,
}
impl Facts {
    pub(super) fn value(&mut self, kind: ProviderItemKind) -> ActivityDisplayValue {
        match kind {
            ProviderItemKind::CommandExecution => {
                ActivityDisplayValue::Command(self.command.take_text())
            }
            ProviderItemKind::Reasoning => {
                ActivityDisplayValue::Reasoning(self.reasoning.take_text())
            }
            ProviderItemKind::FileChange => {
                ActivityDisplayValue::FileChange(ActivityFileChangeDisplay {
                    changes: self.changes,
                    additions: self.diff.additions,
                    deletions: self.diff.deletions,
                    path: if self.path_valid {
                        self.sole_path
                            .take()
                            .and_then(|bytes| String::from_utf8(bytes).ok())
                            .map(String::into_boxed_str)
                    } else {
                        None
                    },
                })
            }
            _ if !self.tool.bytes.is_empty() => ActivityDisplayValue::Tool(self.tool.take_text()),
            _ => ActivityDisplayValue::Kind,
        }
    }
    fn reset_files(&mut self, count: u64) {
        self.changes = count;
        self.diff = Diff::default();
        self.path = Prefix::default();
        self.sole_path = None;
        self.path_seen = false;
        self.path_valid = true;
    }
    fn byte(&mut self, role: Role, byte: u8) -> Result<(), Error> {
        let character = self.utf8.byte(byte)?;
        match role {
            Role::Command => {
                if let Some(character) = character {
                    self.command.first_line(character);
                }
            }
            Role::Tool => self.tool.single_line(byte),
            Role::Reason(index) => {
                if self.reason_index.is_none() && !byte.is_ascii_whitespace() {
                    self.reason_index = Some(index);
                }
                if self.reason_index.is_some_and(|current| index < current) {
                    self.reason_index = Some(index);
                    self.reasoning = Prefix::default();
                }
                if self.reason_index == Some(index) {
                    self.reasoning.single_line(byte);
                }
            }
            Role::Path => self.path.push(byte),
            Role::Diff => self.diff.byte(byte)?,
            Role::Skip => {}
        }
        Ok(())
    }
    fn end(&mut self, role: Role) -> Result<(), Error> {
        self.utf8.finish()?;
        self.utf8 = Utf8::default();
        match role {
            Role::Path => {
                if self.path.truncated
                    || self.path.bytes.is_empty()
                    || std::str::from_utf8(&self.path.bytes).is_err()
                {
                    self.path_valid = false;
                }
                if self.path_seen {
                    if self.sole_path.as_ref() != Some(&self.path.bytes) {
                        self.path_valid = false;
                    }
                } else {
                    self.sole_path = Some(std::mem::take(&mut self.path.bytes));
                    self.path_seen = true;
                }
                self.path = Prefix::default();
                if !self.path_valid {
                    self.sole_path = None;
                }
            }
            Role::Diff => self.diff.finish_file()?,
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Role {
    Skip,
    Command,
    Reason(u64),
    Tool,
    Path,
    Diff,
}

#[derive(Clone, Copy)]
enum Action {
    Ignore,
    TextLength(Role),
    RefStart(Role),
    RefEnd(Role, u64),
    RawLength,
    OptionText(Role),
    Status(u8),
    Changes,
    Summaries,
    Delta,
    ReasonIndex,
    PatchKind,
    Structured(u8),
    ListCount(u8, bool),
    DynamicContent,
    DynamicCount,
    DynamicItem,
    OptionSuccess,
    Success,
    CollabTool,
    ImageStatus,
}

#[derive(Clone, Copy)]
enum Op {
    Number {
        left: u8,
        value: u64,
        action: Action,
    },
    Byte(Action),
    Text(Role),
    TextTag(Role),
    Reuse {
        role: Role,
        start: u64,
        end: u64,
        digest: [u8; 32],
    },
    Digest {
        role: Role,
        start: u64,
        end: u64,
        bytes: [u8; 32],
        used: u8,
    },
    Bytes {
        at: u64,
        end: u64,
        role: Role,
    },
    Skip(u64),
    Files(u64),
    Summaries {
        left: u64,
        index: u64,
    },
    Structured(u8),
    List {
        left: u64,
        depth: u8,
        object: bool,
    },
    RawText,
    DynamicItems(u64),
}

pub(super) struct Parser {
    owner: SyndicContentId,
    frontier: u64,
    frame_start: u64,
    frame_end: u64,
    cursor: u64,
    ops: Vec<Op>,
    window: Window,
    reused_digest: Option<(Sha256, [u8; 32])>,
}

impl Parser {
    pub(super) fn new(
        frame: &SealedProviderFrameReference,
        kind: ProviderItemKind,
    ) -> Result<Self, Error> {
        let reference = frame.frame();
        let delta = matches!(
            frame.observation(),
            ProviderFrameObservationSummaryV1::Delta
        );
        let header =
            4 + 8 + 4 + reference.item_id().as_str().len() as u64 + 1 + if delta { 0 } else { 8 };
        let cursor = reference
            .encoded_start()
            .checked_add(header)
            .ok_or(Error::Invalid("frame offset overflow"))?;
        let mut parser = Self {
            owner: frame.content().id(),
            frontier: frame.content().summary().encoded_bytes(),
            frame_start: reference.encoded_start(),
            frame_end: reference.encoded_end(),
            cursor,
            ops: Vec::new(),
            window: Window::default(),
            reused_digest: None,
        };
        if delta {
            parser.push(&[Op::Byte(Action::Delta)])?;
        } else {
            // Admitted frame shape fixes the item tag; extraction needs no payload revalidation.
            parser.cursor = parser
                .cursor
                .checked_add(1)
                .ok_or(Error::Invalid("item offset overflow"))?;
            parser.item(kind)?;
        }
        Ok(parser)
    }
    fn push(&mut self, ops: &[Op]) -> Result<(), Error> {
        if self.ops.len() + ops.len() > 144 {
            return Err(Error::Invalid("display parser depth exceeded"));
        }
        self.ops.extend(ops.iter().rev().copied());
        Ok(())
    }
    fn number(action: Action) -> Op {
        Op::Number {
            left: 8,
            value: 0,
            action,
        }
    }
    fn item(&mut self, kind: ProviderItemKind) -> Result<(), Error> {
        use ProviderItemKind::*;
        match kind {
            CommandExecution => self.push(&[
                Op::Text(Role::Command),
                Op::Text(Role::Skip),
                Op::Byte(Action::OptionText(Role::Skip)),
                Op::Byte(Action::Ignore),
                Op::Byte(Action::Status(4)),
            ]),
            FileChange => self.push(&[Op::Byte(Action::Status(4)), Self::number(Action::Changes)]),
            Reasoning => self.push(&[Self::number(Action::Summaries)]),
            McpToolCall => self.push(&[
                Op::Text(Role::Skip),
                Op::Text(Role::Tool),
                Op::Byte(Action::Status(3)),
            ]),
            DynamicToolCall => self.push(&[
                Op::Byte(Action::OptionText(Role::Skip)),
                Op::Text(Role::Tool),
                Op::Structured(0),
                Op::Byte(Action::Status(3)),
                Op::Byte(Action::DynamicContent),
                Op::Byte(Action::OptionSuccess),
            ]),
            CollabAgentToolCall => {
                self.push(&[Op::Byte(Action::CollabTool), Op::Byte(Action::Status(3))])
            }
            StandaloneImageGeneration => self.push(&[Op::Byte(Action::ImageStatus)]),
            _ => Ok(()),
        }
    }
    fn skip(&mut self, count: u64) -> Result<(), Error> {
        self.cursor = self
            .cursor
            .checked_add(count)
            .filter(|next| *next <= self.frame_end)
            .ok_or(Error::Invalid("field outside admitted frame"))?;
        Ok(())
    }
    pub(super) fn step(
        &mut self,
        home: &HomeStore,
        storage: &SyndicStorage,
        facts: &mut Facts,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, Error> {
        let mut loaded = false;
        for _ in 0..4096 {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let Some(op) = self.ops.pop() else {
                return Ok(true);
            };
            let at = match op {
                Op::Number { .. } | Op::Byte(_) | Op::TextTag(_) | Op::Digest { .. } => {
                    Some(self.cursor)
                }
                Op::Bytes { at, end, .. } if at < end => Some(at),
                _ => None,
            };
            let byte = if let Some(at) = at {
                if self.window.byte(at).is_none() {
                    if loaded {
                        self.ops.push(op);
                        return Ok(false);
                    }
                    self.window
                        .load(home, storage, self.owner, at, self.frontier)?;
                    loaded = true;
                }
                self.window
                    .byte(at)
                    .ok_or(Error::Invalid("source window misses byte"))?
            } else {
                0
            };
            match op {
                Op::Number {
                    left,
                    value,
                    action,
                } => {
                    self.skip(1)?;
                    let value = (value << 8) | byte as u64;
                    if left == 1 {
                        self.action(action, value, facts)?;
                    } else {
                        self.ops.push(Op::Number {
                            left: left - 1,
                            value,
                            action,
                        });
                    }
                }
                Op::Byte(action) => {
                    self.skip(1)?;
                    self.action(action, byte as u64, facts)?;
                }
                Op::Text(role) => self.push(&[Op::TextTag(role)])?,
                Op::TextTag(role) => {
                    self.skip(1)?;
                    match byte {
                        0 => self.push(&[Self::number(Action::TextLength(role))])?,
                        1 => self.push(&[Self::number(Action::RefStart(role))])?,
                        _ => return Err(Error::Invalid("invalid provider text tag")),
                    }
                }
                Op::Digest {
                    role,
                    start,
                    end,
                    mut bytes,
                    used,
                } => {
                    self.skip(1)?;
                    bytes[usize::from(used)] = byte;
                    if used == 31 {
                        self.push(&[Op::Reuse {
                            role,
                            start,
                            end,
                            digest: bytes,
                        }])?;
                    } else {
                        self.ops.push(Op::Digest {
                            role,
                            start,
                            end,
                            bytes,
                            used: used + 1,
                        });
                    }
                }
                Op::Reuse {
                    role,
                    start,
                    end,
                    digest,
                } => {
                    if start > end || end > self.frame_start {
                        return Err(Error::Invalid(
                            "reused text exceeds original frame frontier",
                        ));
                    }
                    if !matches!(role, Role::Skip) {
                        self.reused_digest = Some((Sha256::new(), digest));
                        self.push(&[Op::Bytes {
                            at: start,
                            end,
                            role,
                        }])?;
                    }
                }
                Op::Bytes { at, end, role } => {
                    if at == end {
                        if let Some((hash, expected)) = self.reused_digest.take() {
                            let actual: [u8; 32] = hash.finalize().into();
                            if actual != expected {
                                return Err(Error::Invalid(
                                    "reused Activity source digest disagrees",
                                ));
                            }
                        }
                        facts.end(role)?;
                    } else {
                        if let Some((hash, _)) = &mut self.reused_digest {
                            hash.update([byte]);
                        }
                        facts.byte(role, byte)?;
                        self.ops.push(Op::Bytes {
                            at: at + 1,
                            end,
                            role,
                        });
                    }
                }
                Op::Skip(count) => self.skip(count)?,
                Op::Files(left) => {
                    if left != 0 {
                        self.push(&[
                            Op::Text(Role::Path),
                            Op::Text(Role::Diff),
                            Op::Byte(Action::PatchKind),
                            Op::Files(left - 1),
                        ])?;
                    }
                }
                Op::Summaries { left, index } => {
                    if left != 0 {
                        self.push(&[
                            Op::Text(Role::Reason(index)),
                            Op::Summaries {
                                left: left - 1,
                                index: index
                                    .checked_add(1)
                                    .ok_or(Error::Invalid("summary index overflow"))?,
                            },
                        ])?;
                    }
                }
                Op::Structured(depth) => self.push(&[Op::Byte(Action::Structured(depth))])?,
                Op::List {
                    left,
                    depth,
                    object,
                } => {
                    if left != 0 {
                        if object {
                            self.push(&[
                                Op::RawText,
                                Op::Structured(depth),
                                Op::List {
                                    left: left - 1,
                                    depth,
                                    object,
                                },
                            ])?;
                        } else {
                            self.push(&[
                                Op::Structured(depth),
                                Op::List {
                                    left: left - 1,
                                    depth,
                                    object,
                                },
                            ])?;
                        }
                    }
                }
                Op::RawText => self.push(&[Self::number(Action::RawLength)])?,
                Op::DynamicItems(left) => {
                    if left != 0 {
                        self.push(&[Op::Byte(Action::DynamicItem), Op::DynamicItems(left - 1)])?;
                    }
                }
            }
        }
        Ok(false)
    }
}
