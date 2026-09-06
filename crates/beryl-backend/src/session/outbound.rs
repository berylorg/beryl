use std::{
    cell::RefCell,
    io::{self, Write},
    time::{Duration, Instant},
};

use serde::Serialize;

const SOURCE_FAILURE_WRITER_ERROR: &str = "bounded outbound source failed";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DispatchProgress {
    #[default]
    NeverWritten,
    SomeBytes,
}

impl DispatchProgress {
    pub(crate) fn record_write(&mut self, bytes: usize) {
        if bytes > 0 {
            *self = Self::SomeBytes;
        }
    }

    pub(crate) const fn some_bytes(self) -> bool {
        matches!(self, Self::SomeBytes)
    }
}

pub(crate) struct OutboundWriteMetrics {
    pub(crate) serialize: Duration,
    pub(crate) transport: Duration,
    pub(crate) bytes: usize,
}

#[derive(Debug)]
pub(crate) enum OutboundWriteFailure<E> {
    Serialize {
        source: serde_json::Error,
        progress: DispatchProgress,
    },
    Transport {
        source: E,
        progress: DispatchProgress,
    },
}

impl<E> OutboundWriteFailure<E> {
    pub(crate) const fn progress(&self) -> DispatchProgress {
        match self {
            Self::Serialize { progress, .. } | Self::Transport { progress, .. } => *progress,
        }
    }
}

pub(crate) struct SourceFailureSlot<E> {
    failure: RefCell<Option<E>>,
}

impl<E> Default for SourceFailureSlot<E> {
    fn default() -> Self {
        Self {
            failure: RefCell::new(None),
        }
    }
}

impl<E> SourceFailureSlot<E> {
    pub(crate) fn record(&self, failure: E) {
        let mut stored = self.failure.borrow_mut();
        if stored.is_none() {
            *stored = Some(failure);
        }
    }

    fn is_occupied(&self) -> bool {
        self.failure.borrow().is_some()
    }

    fn take(&self) -> Option<E> {
        self.failure.borrow_mut().take()
    }
}

struct SourceAwareJsonMessageWriter<'a, W, E> {
    inner: &'a mut W,
    source_failure: &'a SourceFailureSlot<E>,
}

impl<'a, W, E> SourceAwareJsonMessageWriter<'a, W, E> {
    fn new(inner: &'a mut W, source_failure: &'a SourceFailureSlot<E>) -> Self {
        Self {
            inner,
            source_failure,
        }
    }

    fn source_failure_error() -> io::Error {
        io::Error::other(SOURCE_FAILURE_WRITER_ERROR)
    }
}

impl<W: Write, E> Write for SourceAwareJsonMessageWriter<'_, W, E> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.source_failure.is_occupied() {
            return Err(Self::source_failure_error());
        }
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.source_failure.is_occupied() {
            return Err(Self::source_failure_error());
        }
        self.inner.flush()
    }
}

pub(crate) trait JsonMessageWriter: Write {
    type TransportError;

    fn encoded_bytes(&self) -> usize;
    fn dispatch_progress(&self) -> DispatchProgress;
    fn transport_elapsed(&self) -> Duration;
    fn take_transport_failure(&mut self) -> Option<Self::TransportError>;
    fn finish_message(&mut self) -> Result<(), Self::TransportError>;
}

impl<W: JsonMessageWriter, E> JsonMessageWriter for SourceAwareJsonMessageWriter<'_, W, E> {
    type TransportError = W::TransportError;

    fn encoded_bytes(&self) -> usize {
        self.inner.encoded_bytes()
    }

    fn dispatch_progress(&self) -> DispatchProgress {
        self.inner.dispatch_progress()
    }

    fn transport_elapsed(&self) -> Duration {
        self.inner.transport_elapsed()
    }

    fn take_transport_failure(&mut self) -> Option<Self::TransportError> {
        self.inner.take_transport_failure()
    }

    fn finish_message(&mut self) -> Result<(), Self::TransportError> {
        self.inner.finish_message()
    }
}

pub(crate) enum SourceAwareWriteFailure<E, T> {
    Source {
        source: E,
        progress: DispatchProgress,
    },
    Outbound(OutboundWriteFailure<T>),
}

impl<E, T> SourceAwareWriteFailure<E, T> {
    pub(crate) const fn progress(&self) -> DispatchProgress {
        match self {
            Self::Source { progress, .. } => *progress,
            Self::Outbound(failure) => failure.progress(),
        }
    }
}

pub(crate) fn write_json<T, W>(
    writer: &mut W,
    message: &T,
) -> Result<OutboundWriteMetrics, OutboundWriteFailure<W::TransportError>>
where
    T: Serialize + ?Sized,
    W: JsonMessageWriter,
{
    let started = Instant::now();
    if let Err(source) = serde_json::to_writer(&mut *writer, message) {
        let progress = writer.dispatch_progress();
        return if let Some(source) = writer.take_transport_failure() {
            Err(OutboundWriteFailure::Transport { source, progress })
        } else {
            Err(OutboundWriteFailure::Serialize { source, progress })
        };
    }

    if let Err(source) = writer.finish_message() {
        return Err(OutboundWriteFailure::Transport {
            source,
            progress: writer.dispatch_progress(),
        });
    }

    let transport = writer.transport_elapsed();
    Ok(OutboundWriteMetrics {
        serialize: started.elapsed().saturating_sub(transport),
        transport,
        bytes: writer.encoded_bytes(),
    })
}

pub(crate) fn write_source_aware_json<T, W, E>(
    writer: &mut W,
    message: &T,
    source_failure: &SourceFailureSlot<E>,
) -> Result<OutboundWriteMetrics, SourceAwareWriteFailure<E, W::TransportError>>
where
    T: Serialize + ?Sized,
    W: JsonMessageWriter,
{
    let mut writer = SourceAwareJsonMessageWriter::new(writer, source_failure);
    let result = write_json(&mut writer, message);
    let progress = writer.dispatch_progress();
    if let Some(source) = source_failure.take() {
        return Err(SourceAwareWriteFailure::Source { source, progress });
    }
    result.map_err(SourceAwareWriteFailure::Outbound)
}

pub(crate) fn write_all_tracked<W: Write + ?Sized>(
    writer: &mut W,
    mut bytes: &[u8],
    progress: &mut DispatchProgress,
) -> io::Result<()> {
    while !bytes.is_empty() {
        match writer.write(bytes) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
            Ok(written) => {
                progress.record_write(written);
                bytes = &bytes[written..];
            }
            Err(source) if source.kind() == io::ErrorKind::Interrupted => {}
            Err(source) => return Err(source),
        }
    }
    Ok(())
}
