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

use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use likely_stable::unlikely;

use crate::{block_for_recv, block_for_send, read_message, write_message};
use crate::cache_padded::CachePadded;
use crate::common::{Bucket, CLOSED, N_RETRY_SPSC, Operation, SUCCESS};
use crate::error::{RecvError, SendError, TryRecvError, TrySendError};
use crate::spsc::waker::{Waiter, Waker};

macro_rules! select_bucket {
    ($data:expr, $buffer:expr, $capacity:expr) => {
        let mut data = $data.load(Ordering::Relaxed);
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
                    (lap.wrapping_add(2) as u64) << 32
                };

                match $data.compare_exchange_weak(data, new_data, Ordering::Acquire, Ordering::Relaxed) {
                    Ok(_) => return (bucket as &Bucket<T>, bucket_lap, true),
                    Err(v) => data = v,
                }
            } else if lap > bucket_lap {
                // The element is not yet write/read on the previous lap,
                // the chan is empty/full.
                if lap > bucket.lap.load(Ordering::Acquire) {
                    return (bucket as &Bucket<T>, bucket_lap, false);
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
    closed: AtomicBool,

    receiver: Waker,
    sender: Waker,
}

impl<T> Spsc<T> {
    /// Creates SPSC queue with size is roundup to power of 2.
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
            closed: AtomicBool::new(false),
            receiver: Waker::new(),
            sender: Waker::new(),
        }
    }

    /// Sends non-blocking a message.
    ///
    /// Returns `Full` or `Disconnected` error if having.
    pub(crate) fn try_send(&mut self, msg: T) -> Result<(), TrySendError<T>> {
        if unlikely(self.closed.load(Ordering::Relaxed)) {
            return Err(TrySendError::Disconnected(msg));
        }

        let (bucket, capture_lap, success) = self.select_bucket(Operation::Sending);
        if !success {
            return Err(TrySendError::Full(msg));
        }
        write_message!(bucket, capture_lap, msg, self.receiver);
        Ok(())
    }

    /// Receives non-blocking a message.
    ///
    /// Returns `Empty` error if having.
    pub(crate) fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (bucket, capture_lap, success) = self.select_bucket(Operation::Receiving);
        if !success {
            return Err(TryRecvError);
        }
        let msg: T;
        read_message!(bucket, capture_lap, msg, self.sender);
        Ok(msg)
    }

    /// Sends blocking a message.
    ///
    /// Returns `Disconnected` error if having.
    pub(crate) fn send(&mut self, msg: T) -> Result<(), SendError<T>> {
        if unlikely(self.closed.load(Ordering::Relaxed)) {
            return Err(SendError(msg));
        }

        loop {
            let (bucket, capture_lap, success) = self.select_bucket(Operation::Sending);
            if success {
                write_message!(bucket, capture_lap, msg, self.receiver);
                return Ok(());
            }

            let waiter = &Waiter::new(&bucket.lap, capture_lap);
            let state = waiter.retry_and_snooze(&self.closed, N_RETRY_SPSC);
            if state == SUCCESS {
                continue;
            }
            if unlikely(state == CLOSED) {
                return Err(SendError(msg));
            }
            block_for_send!(waiter, self.sender, &self.closed, msg);
        }
    }

    /// Receives blocking a message.
    ///
    /// Returns `Disconnected` error if having.
    pub(crate) fn recv(&mut self) -> Result<T, RecvError> {
        if unlikely(self.closed.load(Ordering::Relaxed)) {
            return Err(RecvError);
        }

        loop {
            let (bucket, capture_lap, success) = self.select_bucket(Operation::Receiving);
            if success {
                let msg: T;
                read_message!(bucket, capture_lap, msg, self.sender);
                return Ok(msg);
            }

            let waiter = &Waiter::new(&bucket.lap, capture_lap);
            let state = waiter.retry_and_snooze(&self.closed, N_RETRY_SPSC);
            if state == SUCCESS {
                continue;
            }
            if unlikely(state == CLOSED) {
                return Err(RecvError);
            }
            block_for_recv!(waiter, self.receiver, &self.closed);
        }
    }

    /// Closes the queue.
    ///
    /// After closed, all [`send`] and [`recv`] operations will be failed.
    ///
    /// Uses [`try_recv`] to read remaining messages if having.
    ///
    /// [`send`]: Spsc::send
    /// [`recv`]: Spsc::recv
    /// [`try_recv`]: Spsc::try_recv
    #[inline]
    pub(crate) fn close(&mut self) {
        if !self.closed.swap(true, Ordering::Relaxed) {
            self.receiver.close();
            self.sender.close();
        }
    }

    /// Selects one bucket for sending or receiving.
    fn select_bucket(&mut self, operation: Operation) -> (&Bucket<T>, u32, bool) {
        match operation {
            Operation::Sending => {
                select_bucket!(self.tail, self.buffer, self.capacity);
            }
            Operation::Receiving => {
                select_bucket!(self.head, self.buffer, self.capacity);
            }
        }
    }
}