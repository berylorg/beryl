use std::{
    alloc::Layout,
    num::NonZeroUsize,
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
    time::{Duration, Instant},
};

use thiserror::Error;

pub fn fixed_channel<T>(capacity: NonZeroUsize) -> Result<FixedChannel<T>, ChannelBuildError> {
    Layout::array::<Option<T>>(capacity.get()).map_err(|_| ChannelBuildError::SizeOverflow)?;
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(capacity.get())
        .map_err(|_| ChannelBuildError::AllocationFailed)?;
    for _ in 0..capacity.get() {
        slots.push(None);
    }
    let inner = Arc::new(ChannelInner {
        state: Mutex::new(ChannelState {
            slots,
            head: 0,
            tail: 0,
            len: 0,
            sender_open: true,
            receiver_open: true,
            sends: 0,
            receives: 0,
            send_waits: 0,
            receive_waits: 0,
            send_timeouts: 0,
            receive_timeouts: 0,
            full: 0,
            high_water: 0,
        }),
        changed: Condvar::new(),
        capacity: capacity.get(),
    });
    Ok((
        FixedChannelSender {
            inner: Arc::clone(&inner),
        },
        FixedChannelReceiver { inner },
    ))
}

pub type FixedChannel<T> = (FixedChannelSender<T>, FixedChannelReceiver<T>);

struct ChannelInner<T> {
    state: Mutex<ChannelState<T>>,
    changed: Condvar,
    capacity: usize,
}

impl<T> ChannelInner<T> {
    fn lock(&self) -> MutexGuard<'_, ChannelState<T>> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

struct ChannelState<T> {
    slots: Vec<Option<T>>,
    head: usize,
    tail: usize,
    len: usize,
    sender_open: bool,
    receiver_open: bool,
    sends: u64,
    receives: u64,
    send_waits: u64,
    receive_waits: u64,
    send_timeouts: u64,
    receive_timeouts: u64,
    full: u64,
    high_water: usize,
}

pub struct FixedChannelSender<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> FixedChannelSender<T> {
    pub fn try_send(&self, message: T) -> Result<(), SendError<T>> {
        let mut state = self.inner.lock();
        if !state.receiver_open {
            return Err(SendError::Closed(message));
        }
        if state.len == self.inner.capacity {
            state.full = state.full.saturating_add(1);
            return Err(SendError::Full(message));
        }
        let tail = state.tail;
        state.slots[tail] = Some(message);
        state.tail = (tail + 1) % self.inner.capacity;
        state.len += 1;
        state.high_water = state.high_water.max(state.len);
        state.sends = state.sends.saturating_add(1);
        drop(state);
        self.inner.changed.notify_all();
        Ok(())
    }

    pub fn send_timeout(&self, message: T, timeout: Duration) -> Result<(), SendError<T>> {
        let deadline = Instant::now().checked_add(timeout);
        let mut message = Some(message);
        let mut recorded_wait = false;
        let mut state = self.inner.lock();
        loop {
            if !state.receiver_open {
                return Err(SendError::Closed(message.take().expect("message retained")));
            }
            if state.len < self.inner.capacity {
                let tail = state.tail;
                state.slots[tail] = message.take();
                state.tail = (tail + 1) % self.inner.capacity;
                state.len += 1;
                state.high_water = state.high_water.max(state.len);
                state.sends = state.sends.saturating_add(1);
                drop(state);
                self.inner.changed.notify_all();
                return Ok(());
            }
            let now = Instant::now();
            let Some(deadline) = deadline else {
                state.send_timeouts = state.send_timeouts.saturating_add(1);
                return Err(SendError::Timeout(
                    message.take().expect("message retained"),
                ));
            };
            if now >= deadline {
                state.send_timeouts = state.send_timeouts.saturating_add(1);
                return Err(SendError::Timeout(
                    message.take().expect("message retained"),
                ));
            }
            if !recorded_wait {
                state.send_waits = state.send_waits.saturating_add(1);
                recorded_wait = true;
            }
            let remaining = deadline.saturating_duration_since(now);
            (state, _) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub fn diagnostics(&self) -> ChannelDiagnostics {
        diagnostics(&self.inner)
    }

    /// Returns a content-free observer that does not retain the channel ring.
    #[must_use]
    pub fn observer(&self) -> FixedChannelObserver<T> {
        FixedChannelObserver {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl<T> Drop for FixedChannelSender<T> {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        state.sender_open = false;
        drop(state);
        self.inner.changed.notify_all();
    }
}

pub struct FixedChannelReceiver<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> FixedChannelReceiver<T> {
    pub fn try_receive(&self) -> Result<T, ReceiveError> {
        let mut state = self.inner.lock();
        if state.len == 0 {
            return if state.sender_open {
                Err(ReceiveError::Empty)
            } else {
                Err(ReceiveError::Closed)
            };
        }
        let message = take_head(&mut state, self.inner.capacity);
        drop(state);
        self.inner.changed.notify_all();
        Ok(message)
    }

    pub fn receive_timeout(&self, timeout: Duration) -> Result<Option<T>, ReceiveError> {
        let deadline = Instant::now().checked_add(timeout);
        let mut recorded_wait = false;
        let mut state = self.inner.lock();
        loop {
            if state.len > 0 {
                let message = take_head(&mut state, self.inner.capacity);
                drop(state);
                self.inner.changed.notify_all();
                return Ok(Some(message));
            }
            if !state.sender_open {
                return Err(ReceiveError::Closed);
            }
            let now = Instant::now();
            let Some(deadline) = deadline else {
                state.receive_timeouts = state.receive_timeouts.saturating_add(1);
                return Ok(None);
            };
            if now >= deadline {
                state.receive_timeouts = state.receive_timeouts.saturating_add(1);
                return Ok(None);
            }
            if !recorded_wait {
                state.receive_waits = state.receive_waits.saturating_add(1);
                recorded_wait = true;
            }
            let remaining = deadline.saturating_duration_since(now);
            (state, _) = self
                .inner
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub fn diagnostics(&self) -> ChannelDiagnostics {
        diagnostics(&self.inner)
    }

    /// Returns a content-free observer that does not retain the channel ring.
    #[must_use]
    pub fn observer(&self) -> FixedChannelObserver<T> {
        FixedChannelObserver {
            inner: Arc::downgrade(&self.inner),
        }
    }
}

impl<T> Drop for FixedChannelReceiver<T> {
    fn drop(&mut self) {
        let mut state = self.inner.lock();
        state.receiver_open = false;
        drop(state);
        self.inner.changed.notify_all();
    }
}

fn take_head<T>(state: &mut ChannelState<T>, capacity: usize) -> T {
    let head = state.head;
    let message = state.slots[head].take().expect("occupied channel slot");
    state.head = (head + 1) % capacity;
    state.len -= 1;
    state.receives = state.receives.saturating_add(1);
    message
}

fn diagnostics<T>(inner: &ChannelInner<T>) -> ChannelDiagnostics {
    let state = inner.lock();
    ChannelDiagnostics {
        capacity: inner.capacity,
        len: state.len,
        sender_open: state.sender_open,
        receiver_open: state.receiver_open,
        sends: state.sends,
        receives: state.receives,
        send_waits: state.send_waits,
        receive_waits: state.receive_waits,
        send_timeouts: state.send_timeouts,
        receive_timeouts: state.receive_timeouts,
        full: state.full,
        high_water: state.high_water,
    }
}

/// Content-free weak diagnostics access for one fixed-capacity channel.
pub struct FixedChannelObserver<T> {
    inner: Weak<ChannelInner<T>>,
}

impl<T> Clone for FixedChannelObserver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Weak::clone(&self.inner),
        }
    }
}

impl<T> FixedChannelObserver<T> {
    /// Returns current channel diagnostics, or `None` after complete ring release.
    #[must_use]
    pub fn diagnostics(&self) -> Option<ChannelDiagnostics> {
        let inner = self.inner.upgrade()?;
        Some(diagnostics(&inner))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelDiagnostics {
    pub capacity: usize,
    pub len: usize,
    pub sender_open: bool,
    pub receiver_open: bool,
    pub sends: u64,
    pub receives: u64,
    pub send_waits: u64,
    pub receive_waits: u64,
    pub send_timeouts: u64,
    pub receive_timeouts: u64,
    pub full: u64,
    pub high_water: usize,
}

#[derive(Debug, Error)]
pub enum ChannelBuildError {
    #[error("fixed channel storage size overflowed")]
    SizeOverflow,
    #[error("fixed channel storage allocation failed")]
    AllocationFailed,
}

#[derive(Debug, Eq, PartialEq)]
pub enum SendError<T> {
    Full(T),
    Timeout(T),
    Closed(T),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ReceiveError {
    #[error("fixed channel is empty")]
    Empty,
    #[error("fixed channel sender is closed")]
    Closed,
}
