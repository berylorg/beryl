use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptBuildKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
}

impl ScanKey for ThreadTranscriptBuildKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptBuildKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }

    pub(crate) fn first_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            generation: TranscriptGeneration::FIRST,
        }
    }

    pub(crate) fn last_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptPathKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
    pub(crate) depth: TurnDepth,
}

impl ScanKey for ThreadTranscriptPathKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
            depth: TurnDepth::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
            depth: TurnDepth::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptPathKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        enc_turn_depth(&mut e, self.depth);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
            depth: dec_turn_depth(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
    pub(crate) position: TranscriptPosition,
}

impl ScanKey for ThreadTranscriptKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
            position: TranscriptPosition::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
            position: TranscriptPosition::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        enc_transcript_pos(&mut e, self.position);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
            position: dec_transcript_pos(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BindingKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) revision: BindingRevision,
}

impl ScanKey for BindingKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            revision: BindingRevision::new(1).expect("nonzero"),
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            revision: BindingRevision::new(u64::MAX).expect("nonzero"),
        }
    }
}

impl BindingKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_binding_rev(&mut e, self.revision);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            revision: dec_binding_rev(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}
