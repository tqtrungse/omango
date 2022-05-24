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

use crate::bucket::{Bucket, Selector};
use crate::cache_padded::CachePadded;
use crate::error::{RecvError, SendError, TryRecvError, TrySendError};
use crate::spsc::waker::{Waiter, Waker};

enum Operation {
    Sending,
    Receiving,
}

macro_rules! spsc_get_selector {
    ($data:expr, $buffer:expr, $capacity:expr, $selector:expr) => {
        let mut data = $data.load(Ordering::Relaxed);
        loop {
            let pos = data as u32;
            let lap = (data >> 32) as u32;
            let bucket = unsafe { $buffer.get_unchecked(pos as usize) };
            let bucket_lap = bucket.get_lap();

            if lap == bucket_lap {
                // The element is ready for writing/reading on this lap.
                // Try to claim the right to write/read to this element.
                let new_data: u64;
                if pos + 1 < $capacity {
                    new_data = data + 1;
                } else {
                    new_data = ((lap + 2) as u64) << 32;
                };

                match $data.compare_exchange_weak(data, new_data, Ordering::Acquire, Ordering::Relaxed) {
                    Ok(_) => {
                        // $selector.ptr = bucket as *const Bucket<T> as *const u8;
                        // $selector.lap = bucket_lap;
                        $selector.set(bucket as *const Bucket<T>, bucket_lap);
                        return true;
                    },
                    Err(v) => data = v,
                }
            } else if lap > bucket_lap {
                // The element is not yet write/read on the previous lap,
                // the chan is empty/full.
                if lap > bucket.get_lap() {
                    // $selector.ptr = bucket as *const Bucket<T> as *const u8;
                    // $selector.lap = bucket_lap;
                    $selector.set(bucket as *const Bucket<T>, bucket_lap);
                    return false;
                }
                // The element has already been written/read on this lap,
                // this means that `send_data`/`read_data` has been changed as well,
                // retry.
                data = $data.load(Ordering::Relaxed);
            } else {
                // The element has already been written/read on this lap,
                // this means that `send_data`/`read_data` has been changed as well,
                // retry.
                data = $data.load(Ordering::Relaxed);
            }
        }
    };
}

/// SPSC queue.
pub(crate) struct Spsc<T> {
    head: CachePadded<AtomicU64>,
    tail: CachePadded<AtomicU64>,

    buffer: Box<[Bucket<T>]>,
    capacity: u32,
    is_closed: AtomicBool,

    receiver: Waker,
    sender: Waker,
}

impl<T> Spsc<T> {
    /// Creates SPSC queue with size is roundup to power of 2.
    #[inline]
    pub(crate) fn new(size: u32) -> Self <> {
        let cap = (size + 1).next_power_of_two();
        let buf: Box<[Bucket<T>]> = (0..cap)
            .map(|_i| {
                Bucket::default()
            })
            .collect();

        Self {
            head: CachePadded::new(AtomicU64::new(1 << 32)),
            tail: CachePadded::new(AtomicU64::new(0)),
            buffer: buf,
            capacity: cap,
            is_closed: AtomicBool::new(false),
            receiver: Waker::new(),
            sender: Waker::new(),
        }
    }

    /// Sends non-blocking a message.
    ///
    /// Returns `Full` or `Disconnected` error.
    pub(crate) fn try_send(&mut self, msg: T) -> Result<(), TrySendError<T>> {
        if unlikely(self.is_closed.load(Ordering::Relaxed)) {
            return Err(TrySendError::Disconnected(msg));
        }

        let selector = &mut Selector::default();
        if !self.get_selector(Operation::Sending, selector) {
            return Err(TrySendError::Full(msg));
        }
        // self.write_message(selector, msg);
        selector.write_message(msg);
        self.receiver.notify();
        Ok(())
    }

    /// Receives non-blocking a message.
    ///
    /// Returns `Empty` error.
    pub(crate) fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let selector = &mut Selector::default();
        if !self.get_selector(Operation::Receiving, selector) {
            return Err(TryRecvError);
        }
        let msg = selector.read_message();
        self.sender.notify();
        Ok(msg)
    }

    /// Sends blocking a message.
    ///
    /// Returns `Disconnected` error,
    pub(crate) fn send(&mut self, msg: T) -> Result<(), SendError<T>> {
        let selector = &mut Selector::default();
        loop {
            if unlikely(self.is_closed.load(Ordering::Relaxed)) {
                return Err(SendError(msg));
            }

            for _ in 0..128 {
                if self.get_selector(Operation::Sending, selector) {
                    selector.write_message(msg);
                    self.receiver.notify();
                    return Ok(());
                }
                core::hint::spin_loop();
            }

            let waiter = Waiter::new();
            let out_condition = || {
                selector.is_ready() || self.is_closed.load(Ordering::Relaxed)
            };
            if self.sender.register(&waiter, out_condition) {
                waiter.sleep(out_condition);
                self.sender.unregister(&waiter);
            }
        }
    }

    /// Receives blocking a message.
    ///
    /// Returns `Disconnected` error,
    pub(crate) fn recv(&mut self) -> Result<T, RecvError> {
        let selector = &mut Selector::default();
        loop {
            if unlikely(self.is_closed.load(Ordering::Relaxed)) {
                return Err(RecvError);
            }

            for _ in 0..128 {
                if self.get_selector(Operation::Receiving, selector) {
                    let msg = selector.read_message();
                    self.sender.notify();
                    return Ok(msg);
                }
                core::hint::spin_loop();
            }

            let waiter = Waiter::new();
            let out_condition = || {
                selector.is_ready() || self.is_closed.load(Ordering::Relaxed)
            };
            if self.receiver.register(&waiter, out_condition) {
                waiter.sleep(out_condition);
                self.receiver.unregister(&waiter);
            }
        }
    }

    /// Close the queue.
    ///
    /// After closed, all [`send`] and [`recv`] operations will be failed.
    ///
    /// Uses [`try_recv`] to read remaining messages.
    ///
    /// [`send`]: Spsc::send
    /// [`recv`]: Spsc::recv
    /// [`try_recv`]: Spsc::try_recv
    #[inline]
    pub(crate) fn close(&mut self) {
        if !self.is_closed.swap(true, Ordering::Relaxed) {
            self.receiver.notify_all();
            self.sender.notify_all();
        }
    }

    /// Selects one bucket for sending or receiving.
    fn get_selector(&mut self, operation: Operation, selector: &mut Selector<T>) -> bool {
        match operation {
            Operation::Sending => {
                spsc_get_selector!(self.tail, self.buffer, self.capacity, selector);
            }
            Operation::Receiving => {
                spsc_get_selector!(self.head, self.buffer, self.capacity, selector);
            }
        }
    }
}