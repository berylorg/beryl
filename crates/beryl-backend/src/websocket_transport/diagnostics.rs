use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

/// Shared content-free WebSocket bulk-buffer and traffic diagnostics.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct WebSocketDiagnostics {
    state: Arc<DiagnosticsState>,
}

#[derive(Debug, Default)]
struct DiagnosticsState {
    outbound_buffer_capacity_bytes: AtomicUsize,
    maximum_outbound_buffered_bytes: AtomicUsize,
    maximum_inbound_frame_bytes: AtomicUsize,
    maximum_transport_chunk_bytes: AtomicUsize,
    maximum_parser_buffer_bytes: AtomicUsize,
    outbound_frames: AtomicUsize,
    inbound_frames: AtomicUsize,
    decoded_messages: AtomicUsize,
    outbound_logical_bytes: AtomicU64,
    inbound_logical_bytes: AtomicU64,
    verified_user_text_wire_bytes: AtomicU64,
}

/// One content-free WebSocket bulk-buffer and traffic snapshot.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebSocketDiagnosticsSnapshot {
    outbound_buffer_capacity_bytes: usize,
    maximum_outbound_buffered_bytes: usize,
    maximum_inbound_frame_bytes: usize,
    maximum_transport_chunk_bytes: usize,
    maximum_parser_buffer_bytes: usize,
    outbound_frames: usize,
    inbound_frames: usize,
    decoded_messages: usize,
    outbound_logical_bytes: u64,
    inbound_logical_bytes: u64,
    verified_user_text_wire_bytes: u64,
}

impl WebSocketDiagnosticsSnapshot {
    #[must_use]
    pub const fn outbound_buffer_capacity_bytes(self) -> usize {
        self.outbound_buffer_capacity_bytes
    }

    #[must_use]
    pub const fn maximum_outbound_buffered_bytes(self) -> usize {
        self.maximum_outbound_buffered_bytes
    }

    #[must_use]
    pub const fn maximum_inbound_frame_bytes(self) -> usize {
        self.maximum_inbound_frame_bytes
    }

    #[must_use]
    pub const fn maximum_transport_chunk_bytes(self) -> usize {
        self.maximum_transport_chunk_bytes
    }

    #[must_use]
    pub const fn maximum_parser_buffer_bytes(self) -> usize {
        self.maximum_parser_buffer_bytes
    }

    #[must_use]
    pub const fn outbound_frames(self) -> usize {
        self.outbound_frames
    }

    #[must_use]
    pub const fn inbound_frames(self) -> usize {
        self.inbound_frames
    }

    #[must_use]
    pub const fn decoded_messages(self) -> usize {
        self.decoded_messages
    }

    #[must_use]
    pub const fn outbound_logical_bytes(self) -> u64 {
        self.outbound_logical_bytes
    }

    #[must_use]
    pub const fn inbound_logical_bytes(self) -> u64 {
        self.inbound_logical_bytes
    }

    #[must_use]
    pub const fn verified_user_text_wire_bytes(self) -> u64 {
        self.verified_user_text_wire_bytes
    }
}

impl WebSocketDiagnostics {
    #[must_use]
    pub fn snapshot(&self) -> WebSocketDiagnosticsSnapshot {
        WebSocketDiagnosticsSnapshot {
            outbound_buffer_capacity_bytes: load(&self.state.outbound_buffer_capacity_bytes),
            maximum_outbound_buffered_bytes: load(&self.state.maximum_outbound_buffered_bytes),
            maximum_inbound_frame_bytes: load(&self.state.maximum_inbound_frame_bytes),
            maximum_transport_chunk_bytes: load(&self.state.maximum_transport_chunk_bytes),
            maximum_parser_buffer_bytes: load(&self.state.maximum_parser_buffer_bytes),
            outbound_frames: load(&self.state.outbound_frames),
            inbound_frames: load(&self.state.inbound_frames),
            decoded_messages: load(&self.state.decoded_messages),
            outbound_logical_bytes: self.state.outbound_logical_bytes.load(Ordering::SeqCst),
            inbound_logical_bytes: self.state.inbound_logical_bytes.load(Ordering::SeqCst),
            verified_user_text_wire_bytes: self
                .state
                .verified_user_text_wire_bytes
                .load(Ordering::SeqCst),
        }
    }

    pub(crate) fn record_outbound_buffer_capacity(&self, bytes: usize) {
        self.state
            .outbound_buffer_capacity_bytes
            .fetch_max(bytes, Ordering::SeqCst);
    }

    pub(crate) fn record_outbound_buffered(&self, bytes: usize) {
        self.state
            .maximum_outbound_buffered_bytes
            .fetch_max(bytes, Ordering::SeqCst);
    }

    pub(crate) fn record_outbound_frame(&self, bytes: usize) {
        increment(&self.state.outbound_frames, "outbound WebSocket frame");
        add_u64(
            &self.state.outbound_logical_bytes,
            bytes,
            "outbound WebSocket bytes",
        );
    }

    pub(crate) fn record_inbound_frame(&self, bytes: usize) {
        increment(&self.state.inbound_frames, "inbound WebSocket frame");
        self.state
            .maximum_inbound_frame_bytes
            .fetch_max(bytes, Ordering::SeqCst);
    }

    pub(crate) fn record_decoded_message(
        &self,
        bytes: usize,
        maximum_transport_chunk_bytes: usize,
        maximum_parser_buffer_bytes: usize,
        verified_user_text_wire_bytes: usize,
    ) {
        increment(&self.state.decoded_messages, "decoded WebSocket message");
        add_u64(
            &self.state.inbound_logical_bytes,
            bytes,
            "inbound WebSocket bytes",
        );
        add_u64(
            &self.state.verified_user_text_wire_bytes,
            verified_user_text_wire_bytes,
            "verified user-message bytes",
        );
        self.state
            .maximum_transport_chunk_bytes
            .fetch_max(maximum_transport_chunk_bytes, Ordering::SeqCst);
        self.state
            .maximum_parser_buffer_bytes
            .fetch_max(maximum_parser_buffer_bytes, Ordering::SeqCst);
    }
}

fn load(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

fn increment(counter: &AtomicUsize, label: &'static str) {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("{label} count overflowed"));
}

fn add_u64(counter: &AtomicU64, bytes: usize, label: &'static str) {
    let bytes = u64::try_from(bytes).expect("WebSocket chunk bytes fit u64");
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            current.checked_add(bytes)
        })
        .unwrap_or_else(|_| panic!("{label} count overflowed"));
}
