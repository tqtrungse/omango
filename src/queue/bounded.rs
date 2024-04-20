// Copyright (c) 2024 Trung Tran <tqtrungse@gmail.com>
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

//! The implementation is based on the blog of Dmitry Vyukov.
//!
//! Source: '<https://docs.google.com/document/d/1yIAYmbvL3JxOKOjuCyon7JhW4cSv1wy5hC0ApeGMV9s/pub>'

use std::{
    ops::{AddAssign, BitAndAssign, BitOrAssign},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use omango_util::{
    backoff::Backoff,
    cache_padded::CachePadded,
    hint::{likely, unlikely},
};

use crate::queue::{
    state::State,
    elem::ElemArr,
    waker::{Checker, Waiter, Waker},
    error::{SendError, RecvError, TrySendError, TryRecvError},
};

const RETRIES: u8 = 2;
const MPMC_MASK_32: u32 = 1 << 31;
const MPMC_MASK_64: u64 = 1 << 63;

pub(crate) trait Bounded<T> {
    /// Sends non-blocking a message.
    ///
    /// Returns `Full` or `Disconnected` error if having.
    #[inline]
    fn try_send(&mut self, msg: T) -> Result<(), TrySendError<T>> {
        let (elem, lap, state) = self.select_bucket_4_send();

        if likely(state == State::Success) {
            elem.write(lap.wrapping_add(1), msg);
            self.wake_reader();
            Ok(())
        } else if unlikely(state == State::Closed) {
            Err(TrySendError::Disconnected(msg))
        } else {
            Err(TrySendError::Full(msg))
        }
    }

    /// Receives non-blocking a message.
    ///
    /// Returns `Empty` error if having.
    #[inline]
    fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let (elem, lap, success) = self.select_bucket_4_recv();
        if unlikely(!success) {
            return Err(TryRecvError);
        }
        let msg = elem.read(lap.wrapping_add(1));
        self.wake_writer();
        Ok(msg)
    }


    /// Sends blocking a message.
    ///
    /// Returns `Disconnected` error if having.
    fn send(&mut self, msg: T, checker: &dyn Checker) -> Result<(), SendError<T>> {
        loop {
            let (elem, lap, state) = self.select_bucket_4_send();

            if likely(state == State::Success) {
                elem.write(lap.wrapping_add(1), msg);
                self.wake_reader();
                return Ok(());
            } else if unlikely(state == State::Closed) {
                return Err(SendError(msg));
            }

            let waiter = &Waiter::new(elem.atom_lap(), lap);
            let mut state = waiter.retry(checker, RETRIES);

            if likely(state == State::Success) {
                continue;
            }
            if unlikely(state == State::Closed) {
                return Err(SendError(msg));
            }

            if likely(self.register_writer_waiter(waiter)) {
                state = waiter.sleep(checker);
                self.unregister_writer_waiter(waiter);
                if unlikely(state == State::Closed) {
                    return Err(SendError(msg));
                }
            }
        }
    }

    /// Receives blocking a message.
    ///
    /// Returns `Empty` error if having.
    fn recv(&mut self, checker: &dyn Checker) -> Result<T, RecvError> {
        let mut state = State::Success;
        loop {
            let (elem, lap, success) = self.select_bucket_4_recv();
            if likely(success) {
                let msg = elem.read(lap.wrapping_add(1));
                self.wake_writer();
                return Ok(msg);
            } else if unlikely(state == State::Closed) {
                return Err(RecvError);
            }

            let waiter = &Waiter::new(elem.atom_lap(), lap);
            state = waiter.retry(checker, RETRIES);

            if likely(state == State::Success) {
                continue;
            }
            if unlikely(state == State::Closed) {
                continue;
            }

            if likely(self.register_reader_waiter(waiter)) {
                state = waiter.sleep(checker);
                self.unregister_reader_waiter(waiter);
            }
        }
    }

    /// Close the queue.
    ///
    /// After closed, all [`try_send`] and [`send`] operations will be failed.
    ///
    /// Uses [`try_recv`] or [`recv`] to read remaining messages.
    fn close(&self);

    /// Get current length of queue.
    fn length(&self) -> u32;

    /// Selects a bucket for writing.
    fn select_bucket_4_send(&mut self) -> (&ElemArr<T>, u32, State);

    /// Selects a bucket for reading.
    fn select_bucket_4_recv(&mut self) -> (&ElemArr<T>, u32, bool);

    fn register_writer_waiter(&self, waiter: &Waiter) -> bool;

    fn register_reader_waiter(&self, waiter: &Waiter) -> bool;

    fn unregister_writer_waiter(&self, waiter: &Waiter);

    fn unregister_reader_waiter(&self, waiter: &Waiter);

    fn wake_reader(&self);

    fn wake_writer(&self);

    fn cast(&self) -> &dyn Checker;
}

struct Meta {
    write_meta: u64,
    closed: AtomicBool,
}

pub(crate) struct SpscBounded<T> {
    read_meta: CachePadded<u64>,
    meta: CachePadded<Meta>,

    read_waker: CachePadded<Waker>,
    write_waker: CachePadded<Waker>,

    buffer: Box<[ElemArr<T>]>,
    capacity: u32,
}

impl<T> SpscBounded<T> {
    /// Creates SPSC queue with size.
    #[inline]
    pub(crate) fn new(cap: u32) -> Self <> {
        assert!(cap <= (1 << 30));

        let raw_cap = if cap > 0 {
            cap
        } else {
            1
        };
        let buf: Box<[ElemArr<T>]> = (0..raw_cap)
            .map(|_i| {
                ElemArr::default()
            })
            .collect();

        Self {
            read_meta: CachePadded::new(1 << 32),
            meta: CachePadded::new(Meta {
                write_meta: 0,
                closed: AtomicBool::new(false),
            }),
            read_waker: CachePadded::new(Waker::default()),
            write_waker: CachePadded::new(Waker::default()),
            buffer: buf,
            capacity: raw_cap,
        }
    }
}

impl<T> Bounded<T> for SpscBounded<T> {
    #[inline(always)]
    fn close(&self) {
        self.meta.closed.store(true, Ordering::Relaxed);
        self.write_waker.close();
        self.read_waker.close();
    }

    #[inline(always)]
    fn length(&self) -> u32 {
        let head = *self.read_meta as u32;
        let tail = self.meta.write_meta as u32;
        if tail > head {
            return tail - head + 1;
        }
        self.capacity - head + tail
    }

    fn select_bucket_4_send(&mut self) -> (&ElemArr<T>, u32, State) {
        let meta = self.meta.write_meta;
        let backoff = Backoff::default();

        loop {
            let idx = meta as u32;
            let lap = (meta >> 32) as u32;
            let elem = unsafe { self.buffer.get_unchecked(idx as usize) };

            if unlikely(self.meta.closed.load(Ordering::Relaxed)) {
                return (elem, 0, State::Closed);
            }

            let elem_lap = elem.load_lap();
            match lap.cmp(&elem_lap) {
                std::cmp::Ordering::Equal => {
                    // The element is ready for writing on this lap.
                    // Try to claim the right to write to this element.
                    if likely(idx + 1 < self.capacity) {
                        self.meta.write_meta += 1;
                    } else {
                        self.meta.write_meta = (lap.wrapping_add(2) as u64) << 32;
                    };
                    return (elem, elem_lap, State::Success);
                },
                std::cmp::Ordering::Greater => {
                    // The element is not yet read on the previous lap.
                    if lap > elem.load_lap() {
                        return (elem, elem_lap, State::Failed);
                    }
                    // The element has already been read on this lap,
                    // this means that `read operation` has been changed as well,
                    // retry.
                    backoff.spin();
                },
                std::cmp::Ordering::Less => {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();
                },
            }
        }
    }

    fn select_bucket_4_recv(&mut self) -> (&ElemArr<T>, u32, bool) {
        let meta = *self.read_meta;
        let idx = meta as u32;
        let lap = (meta >> 32) as u32;
        let elem = unsafe { self.buffer.get_unchecked(idx as usize) };
        let backoff = Backoff::default();

        loop {
            let elem_lap = elem.load_lap();
            match lap.cmp(&elem_lap) {
                std::cmp::Ordering::Equal => {
                    // The element is ready for reading on this lap.
                    // Try to claim the right to read to this element.
                    if likely(idx + 1 < self.capacity) {
                        self.read_meta.add_assign(1);
                    } else {
                        self.read_meta.bitand_assign(0);
                        self.read_meta.bitor_assign((lap.wrapping_add(2) as u64) << 32);
                    };
                    return (elem, elem_lap, true);
                },
                std::cmp::Ordering::Greater => {
                    // The element is not yet write on the previous lap.
                    if lap > elem.load_lap() {
                        return (elem, elem_lap, false);
                    }
                    // The element has already been written on this lap,
                    // this means that `send operation` has been changed as well,
                    // retry.
                    backoff.spin();
                },
                std::cmp::Ordering::Less => {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();
                },
            }
        }
    }

    #[inline(always)]
    fn register_writer_waiter(&self, waiter: &Waiter) -> bool {
        self.write_waker.register(waiter)
    }

    #[inline(always)]
    fn register_reader_waiter(&self, waiter: &Waiter) -> bool {
        self.read_waker.register(waiter)
    }

    #[inline(always)]
    fn unregister_writer_waiter(&self, waiter: &Waiter) {
        self.write_waker.unregister(waiter);
    }

    #[inline(always)]
    fn unregister_reader_waiter(&self, waiter: &Waiter) {
        self.read_waker.unregister(waiter);
    }

    #[inline(always)]
    fn wake_reader(&self) {
        self.read_waker.wake();
    }

    #[inline(always)]
    fn wake_writer(&self) {
        self.write_waker.wake();
    }

    #[inline(always)]
    fn cast(&self) -> &dyn Checker {
        self
    }
}

impl<T> Checker for SpscBounded<T> {
    /// Check queue was closed.
    #[inline(always)]
    fn is_close(&self) -> bool {
        self.meta.closed.load(Ordering::Relaxed)
    }
}

pub(crate) struct MpmcBounded<T> {
    read_meta: CachePadded<AtomicU64>,
    write_meta: CachePadded<AtomicU64>,

    read_waker: CachePadded<Waker>,
    write_waker: CachePadded<Waker>,

    buffer: Box<[ElemArr<T>]>,
    capacity: u32,
}

impl<T> MpmcBounded<T> {
    /// Creates MPMC queue with size.
    #[inline]
    pub(crate) fn new(cap: u32) -> Self <> {
        assert!(cap <= (1 << 30));

        let raw_cap = if cap > 0 {
            cap
        } else {
            1
        };
        let buf: Box<[ElemArr<T>]> = (0..raw_cap)
            .map(|_i| {
                ElemArr::default()
            })
            .collect();

        Self {
            read_meta: CachePadded::new(AtomicU64::new(1 << 32)),
            write_meta: CachePadded::new(AtomicU64::new(0)),
            read_waker: CachePadded::new(Waker::default()),
            write_waker: CachePadded::new(Waker::default()),
            buffer: buf,
            capacity: raw_cap,
        }
    }

    #[inline(always)]
    pub(crate) fn get_cap(&self) -> u32 {
        self.capacity
    }
}

impl<T> Bounded<T> for MpmcBounded<T> {
    fn close(&self) {
        let mut meta = self.write_meta.load(Ordering::Acquire);
        if (meta & MPMC_MASK_64) != 0 {
            return;
        }

        let mut new_meta: u64;
        let backoff = Backoff::default();

        loop {
            new_meta = meta | MPMC_MASK_64;
            match self.write_meta.compare_exchange_weak(
                meta,
                new_meta,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(v) => {
                    meta = v;
                    backoff.spin();
                }
            }
        }

        self.write_waker.close();
        self.read_waker.close()
    }

    #[inline(always)]
    fn length(&self) -> u32 {
        let head = self.read_meta.load(Ordering::Relaxed) as u32;
        let tail = self.write_meta.load(Ordering::Relaxed) as u32;
        if tail > head {
            return tail - head + 1;
        }
        self.capacity - head + tail
    }

    fn select_bucket_4_send(&mut self) -> (&ElemArr<T>, u32, State) {
        let mut meta = self.write_meta.load(Ordering::Relaxed);
        let backoff = Backoff::default();

        loop {
            let idx = meta as u32;
            let lap = (meta >> 32) as u32;
            let elem = unsafe { self.buffer.get_unchecked(idx as usize) };

            if unlikely(lap >= MPMC_MASK_32) {
                return (elem, 0, State::Closed);
            }

            let elem_lap = elem.load_lap();
            match lap.cmp(&elem_lap) {
                std::cmp::Ordering::Equal => {
                    // The element is ready for writing on this lap.
                    // Try to claim the right to write to this element.
                    let new_tail = if likely(idx + 1 < self.capacity) {
                        meta + 1
                    } else {
                        (lap.wrapping_add(2) as u64) << 32
                    };

                    match self.write_meta.compare_exchange_weak(
                        meta,
                        new_tail,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => return (elem, elem_lap, State::Success),
                        Err(v) => {
                            meta = v;
                            backoff.spin();
                        }
                    }
                },
                std::cmp::Ordering::Greater => {
                    // The element is not yet read on the previous lap.
                    if lap > elem.load_lap() {
                        return (elem, elem_lap, State::Failed);
                    }
                    // The element has already been read on this lap,
                    // this means that `read operation` has been changed as well,
                    // retry.
                    backoff.spin();
                    meta = self.write_meta.load(Ordering::Relaxed);
                },
                std::cmp::Ordering::Less => {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();

                    // The element has already been read on this lap,
                    // this means that `read operation` has been changed as well,
                    // retry.
                    meta = self.write_meta.load(Ordering::Relaxed);
                },
            }
        }
    }

    fn select_bucket_4_recv(&mut self) -> (&ElemArr<T>, u32, bool) {
        let mut meta = self.read_meta.load(Ordering::Relaxed);
        let backoff = Backoff::default();

        loop {
            let idx = meta as u32;
            let lap = (meta >> 32) as u32;
            let elem = unsafe { self.buffer.get_unchecked(idx as usize) };
            let elem_lap = elem.load_lap();
            
            match lap.cmp(&elem_lap) {
                std::cmp::Ordering::Equal => {
                    // The element is ready for reading on this lap.
                    // Try to claim the right to read to this element.
                    let new_data = if likely(idx + 1 < self.capacity) {
                        meta + 1
                    } else {
                        (lap.wrapping_add(2) as u64) << 32
                    };

                    match self.read_meta.compare_exchange_weak(
                        meta,
                        new_data,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => return (elem, elem_lap, true),
                        Err(v) => {
                            meta = v;
                            backoff.spin();
                        }
                    }
                },
                std::cmp::Ordering::Greater => {
                    // The element is not yet write on the previous lap.
                    if lap > elem.load_lap() {
                        return (elem, elem_lap, false);
                    }
                    // The element has already been written on this lap,
                    // this means that `send operation` has been changed as well,
                    // retry.
                    backoff.spin();
                    meta = self.read_meta.load(Ordering::Relaxed);
                },
                std::cmp::Ordering::Less => {
                    // Snooze because we need to wait for the stamp to get updated.
                    backoff.snooze();

                    // The element has already been written on this lap,
                    // this means that `send operation` has been changed as well,
                    // retry.
                    meta = self.read_meta.load(Ordering::Relaxed);
                },
            }
        }
    }

    #[inline(always)]
    fn register_writer_waiter(&self, waiter: &Waiter) -> bool {
        self.write_waker.register(waiter)
    }

    #[inline(always)]
    fn register_reader_waiter(&self, waiter: &Waiter) -> bool {
        self.read_waker.register(waiter)
    }

    #[inline(always)]
    fn unregister_writer_waiter(&self, waiter: &Waiter) {
        self.write_waker.unregister(waiter);
    }

    #[inline(always)]
    fn unregister_reader_waiter(&self, waiter: &Waiter) {
        self.read_waker.unregister(waiter);
    }

    #[inline(always)]
    fn wake_reader(&self) {
        self.read_waker.wake();
    }

    #[inline(always)]
    fn wake_writer(&self) {
        self.write_waker.wake();
    }

    #[inline(always)]
    fn cast(&self) -> &dyn Checker {
        self
    }
}

impl<T> Checker for MpmcBounded<T> {
    /// Check queue was closed.
    #[inline(always)]
    fn is_close(&self) -> bool {
        (self.write_meta.load(Ordering::Relaxed) & MPMC_MASK_64) != 0
    }
}