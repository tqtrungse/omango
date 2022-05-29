// Copyright (c) 2022 Trung <tqtrungse@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! The implementation is based on Dmitry Vyukov's bounded MPMC queue.
//!
//! Source:
//!   - '<https://docs.google.com/document/d/1yIAYmbvL3JxOKOjuCyon7JhW4cSv1wy5hC0ApeGMV9s/pub>'

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use likely_stable::unlikely;

use crate::backoff::Backoff;
use crate::block;
use crate::cache_padded::CachePadded;
use crate::common::{Bucket, Operation, Selector};
use crate::error::{RecvError, SendError, TryRecvError, TrySendError};
use crate::mpmc::waker::{Waiter, Waker};

macro_rules! get_selector {
    ($data:expr, $buffer:expr, $capacity:expr, $selector:expr) => {
        let mut data = $data.load(Ordering::Relaxed);
        let backoff = Backoff::default();
        loop {
            let pos = data as u32;
            let lap = (data >> 32) as u32;
            let bucket = unsafe { $buffer.get_unchecked(pos as usize) };
            let bucket_lap = bucket.lap.load(Ordering::Acquire);

            if lap == bucket_lap {
                // The element is ready for writing/reading on this lap.
                // Try to claim the right to write/read to this element.
                let new_data = if pos + 1 < $capacity {
                    data + 1
                } else {
                    ((lap + 2) as u64) << 32
                };

                match $data.compare_exchange_weak(data, new_data, Ordering::Acquire, Ordering::Relaxed) {
                    Ok(_) => {
                        $selector.ptr = bucket as *const Bucket<T> as *const u8;
                        $selector.lap = bucket_lap;
                        return true;
                    },
                    Err(v) => {
                        data = v;
                        backoff.spin();
                    }
                }
            } else if lap > bucket_lap {
                // The element is not yet write/read on the previous lap,
                // the chan is empty/full.
                if lap > bucket.lap.load(Ordering::Acquire) {
                    $selector.ptr = bucket as *const Bucket<T> as *const u8;
                    $selector.lap = bucket_lap;
                    return false;
                }
                // The element has already been written/read on this lap,
                // this means that `send_data`/`read_data` has been changed as well,
                // retry.
                backoff.spin();
                data = $data.load(Ordering::Relaxed);
            } else {
                // Snooze because we need to wait for the stamp to get updated.
                backoff.snooze();

                // The element has already been written/read on this lap,
                // this means that `send_data`/`read_data` has been changed as well,
                // retry.
                data = $data.load(Ordering::Relaxed);
            }
        }
    };
}

/// MPMC queue.
pub(crate) struct Mpmc<T> {
    head: CachePadded<AtomicU64>,
    tail: CachePadded<AtomicU64>,

    buffer: Box<[Bucket<T>]>,
    capacity: u32,
    is_closed: AtomicBool,

    receivers: Waker,
    senders: Waker,
}

impl<T> Mpmc<T> {
    /// Creates MPMC queue with size is roundup to power of 2.
    #[inline]
    pub(crate) fn new(size: u32) -> Self <> {
        let buf: Box<[Bucket<T>]> = (0..size + 1)
            .map(|_i| {
                Bucket::default()
            })
            .collect();

        Self {
            head: CachePadded::new(AtomicU64::new(1 << 32)),
            tail: CachePadded::new(AtomicU64::new(0)),
            buffer: buf,
            capacity: size + 1,
            is_closed: AtomicBool::new(false),
            receivers: Waker::new(),
            senders: Waker::new(),
        }
    }

    /// Sends non-blocking a message.
    ///
    /// Returns `Full` or `Disconnected` error if having.
    pub(crate) fn try_send(&mut self, msg: T) -> Result<(), TrySendError<T>> {
        if unlikely(self.is_closed.load(Ordering::Relaxed)) {
            return Err(TrySendError::Disconnected(msg));
        }

        let selector = &mut Selector::default();
        if !self.get_selector(Operation::Sending, selector) {
            return Err(TrySendError::Full(msg));
        }
        selector.write_message(msg);
        self.receivers.notify();
        Ok(())
    }

    /// Receives non-blocking a message.
    ///
    /// Returns `Empty` error if having.
    pub(crate) fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let selector = &mut Selector::default();
        if !self.get_selector(Operation::Receiving, selector) {
            return Err(TryRecvError);
        }
        let msg = selector.read_message();
        self.senders.notify();
        Ok(msg)
    }

    /// Sends blocking a message.
    ///
    /// Returns `Disconnected` error if having.
    pub(crate) fn send(&mut self, msg: T) -> Result<(), SendError<T>> {
        let selector = &mut Selector::default();
        loop {
            if unlikely(self.is_closed.load(Ordering::Relaxed)) {
                return Err(SendError(msg));
            }
            for _ in 0..128 {
                if self.get_selector(Operation::Sending, selector) {
                    selector.write_message(msg);
                    self.receivers.notify();
                    return Ok(());
                }
                core::hint::spin_loop();
            }
            block!(self.is_closed, self.senders, selector);
        }
    }

    /// Receives blocking a message.
    ///
    /// Returns `Disconnected` error if having.
    pub(crate) fn recv(&mut self) -> Result<T, RecvError> {
        let selector = &mut Selector::default();
        loop {
            if unlikely(self.is_closed.load(Ordering::Relaxed)) {
                return Err(RecvError);
            }
            for _ in 0..128 {
                if self.get_selector(Operation::Receiving, selector) {
                    let msg = selector.read_message();
                    self.senders.notify();
                    return Ok(msg);
                }
                core::hint::spin_loop();
            }
            block!(self.is_closed, self.receivers, selector);
        }
    }

    /// Close the queue.
    ///
    /// After closed, all [`send`] and [`recv`] operations will be failed.
    ///
    /// Uses [`try_recv`] to read remaining messages.
    ///
    /// [`send`]: Mpmc::send
    /// [`recv`]: Mpmc::recv
    /// [`try_recv`]: Mpmc::try_recv
    #[inline]
    pub(crate) fn close(&mut self) {
        if !self.is_closed.swap(true, Ordering::Relaxed) {
            self.receivers.notify_all();
            self.senders.notify_all();
        }
    }

    /// Gets selector for writing or reading.
    fn get_selector(&mut self, operation: Operation, selector: &mut Selector) -> bool {
        match operation {
            Operation::Sending => {
                get_selector!(self.tail, self.buffer, self.capacity, selector);
            }
            Operation::Receiving => {
                get_selector!(self.head, self.buffer, self.capacity, selector);
            }
        }
    }
}