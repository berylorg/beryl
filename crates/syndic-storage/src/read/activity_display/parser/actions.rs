use super::*;
impl Parser {
    pub(super) fn action(
        &mut self,
        action: Action,
        value: u64,
        facts: &mut Facts,
    ) -> Result<(), Error> {
        match action {
            Action::Ignore => {}
            Action::TextLength(role) => {
                let start = self.cursor;
                self.skip(value)?;
                if !matches!(role, Role::Skip) {
                    self.push(&[Op::Bytes {
                        at: start,
                        end: self.cursor,
                        role,
                    }])?;
                }
            }
            Action::RefStart(role) => self.push(&[Self::number(Action::RefEnd(role, value))])?,
            Action::RefEnd(role, start) => {
                if start >= value || value > self.frame_start {
                    return Err(Error::Invalid(
                        "reused text exceeds original frame frontier",
                    ));
                }
                if matches!(role, Role::Skip) {
                    self.skip(32)?;
                } else {
                    self.push(&[Op::Digest {
                        role,
                        start,
                        end: value,
                        bytes: [0; 32],
                        used: 0,
                    }])?;
                }
            }
            Action::RawLength => self.skip(value)?,
            Action::OptionText(role) => match value {
                0 => {}
                1 => self.push(&[Op::Text(role)])?,
                _ => return Err(Error::Invalid("invalid optional text")),
            },
            Action::Status(count) => {
                if value >= count as u64 {
                    return Err(Error::Invalid("invalid tool status"));
                }
                facts.failed = value >= 2;
            }
            Action::ImageStatus => {
                if value >= 3 {
                    return Err(Error::Invalid("invalid image status"));
                }
                facts.failed = value == 1;
            }
            Action::Changes => {
                facts.reset_files(value);
                self.push(&[Op::Files(value)])?;
            }
            Action::Summaries => {
                facts.reasoning = Prefix::default();
                facts.reason_index = None;
                self.push(&[Op::Summaries {
                    left: value,
                    index: 0,
                }])?;
            }
            Action::Delta => match value {
                3 => self.push(&[Self::number(Action::ReasonIndex)])?,
                7 => self.push(&[Self::number(Action::Changes)])?,
                0..=8 => {}
                _ => return Err(Error::Invalid("invalid provider delta")),
            },
            Action::ReasonIndex => self.push(&[Op::Text(Role::Reason(value))])?,
            Action::PatchKind => match value {
                0 | 1 => {}
                2 => self.push(&[Op::Byte(Action::OptionText(Role::Skip))])?,
                _ => return Err(Error::Invalid("invalid patch kind")),
            },
            Action::Structured(depth) => match value {
                0..=2 => {}
                3..=5 => self.skip(8)?,
                6 => self.push(&[Op::Text(Role::Skip)])?,
                7 | 8 if depth < 128 => {
                    self.push(&[Self::number(Action::ListCount(depth + 1, value == 8))])?
                }
                _ => return Err(Error::Invalid("invalid structured value or depth")),
            },
            Action::ListCount(depth, object) => self.push(&[Op::List {
                left: value,
                depth,
                object,
            }])?,
            Action::DynamicContent => match value {
                0 => {}
                1 => self.push(&[Self::number(Action::DynamicCount)])?,
                _ => return Err(Error::Invalid("invalid dynamic content option")),
            },
            Action::DynamicCount => self.push(&[Op::DynamicItems(value)])?,
            Action::DynamicItem => match value {
                0 => self.push(&[Op::Text(Role::Skip)])?,
                1 => self.push(&[Op::RawText])?,
                2 => self.skip(41)?,
                _ => return Err(Error::Invalid("invalid dynamic output kind")),
            },
            Action::OptionSuccess => match value {
                0 => {}
                1 => self.push(&[Op::Byte(Action::Success)])?,
                _ => return Err(Error::Invalid("invalid dynamic success option")),
            },
            Action::Success => match value {
                0 => facts.failed = true,
                1 => {}
                _ => return Err(Error::Invalid("invalid dynamic success")),
            },
            Action::CollabTool => {
                let text = match value {
                    0 => "spawnAgent",
                    1 => "sendInput",
                    2 => "resumeAgent",
                    3 => "wait",
                    4 => "closeAgent",
                    _ => return Err(Error::Invalid("invalid collaboration tool")),
                };
                facts.tool = Prefix::default();
                for byte in text.bytes() {
                    facts.tool.push(byte);
                }
            }
        }
        Ok(())
    }
}
