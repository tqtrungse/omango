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

use std::collections::vec_deque::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::thread::Thread;

use likely_stable::unlikely;

use crate::common::{CLOSED, FAILED, N_SPIN, SUCCESS};
use crate::spsc::spinlock::Spinlock;

/// An object to implement blocking.
pub(crate) struct Waiter {
    bucket_lap: *const AtomicU32,
    capture_lap: u32,
    thread: Thread,
    parked: AtomicBool,
}

impl Waiter {
    #[inline]
    pub(crate) fn new(bucket_lap: *const AtomicU32, lap: u32) -> Self {
        Self {
            bucket_lap,
            capture_lap: lap,
            thread: thread::current(),
            parked: AtomicBool::new(false),
        }
    }

    /// Retries and snooze the current thread.
    ///
    /// Before real blocking, it will spin and retry N times to check condition.
    /// This thing significantly improves performance.
    pub(crate) fn retry_and_snooze(&self, closed: &AtomicBool, n_retry: u8) -> u8 {
        let bucket_lap = unsafe { &(*self.bucket_lap) };
        for _ in 0..n_retry {
            for _ in 0..N_SPIN {
                if unlikely(closed.load(Ordering::Relaxed)) {
                    return CLOSED;
                }
                if bucket_lap.load(Ordering::Acquire) != self.capture_lap {
                    return SUCCESS;
                }
                core::hint::spin_loop();
            }
        }
        FAILED
    }

    /// Blocks the current thread until woken.
    pub(crate) fn sleep(&self, closed: &AtomicBool) -> u8 {
        let bucket_lap = unsafe { &(*self.bucket_lap) };
        self.parked.store(true, Ordering::Relaxed);
        loop {
            if unlikely(closed.load(Ordering::Relaxed)) {
                self.parked.store(false, Ordering::Release);
                return CLOSED;
            }
            if bucket_lap.load(Ordering::Acquire) != self.capture_lap {
                self.parked.store(false, Ordering::Release);
                return SUCCESS;
            }
            thread::park();
        }
    }
}

struct Metadata {
    waiters: VecDeque<*const Waiter>,
    closed: bool,
}

impl Metadata {
    #[inline]
    fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
            closed: false,
        }
    }

    /// Registers a waiter.
    #[inline]
    fn register(&mut self, waiter: &Waiter) {
        self.waiters.push_back(waiter as *const Waiter);
    }

    /// Unregisters a waiter.
    #[inline]
    fn unregister(&mut self, waiter: &Waiter) -> bool {
        if !self.waiters.is_empty() {
            if let Some((i, _)) =
            self
                .waiters
                .iter()
                .enumerate()
                .find(|&(_, item)| (*item) == (waiter as *const Waiter))
            {
                self.waiters.remove(i);
                return true;
            }
        }
        false
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    fn notify(&self) {
        if !self.waiters.is_empty() {
            let waiter = self.waiters.get(0).unwrap();
            unsafe {
                if (*(*waiter)).parked.load(Ordering::Acquire) {
                    (*(*waiter)).thread.unpark();
                }
            }
        }
    }

    fn close(&mut self) {
        self.closed = true;
        if !self.waiters.is_empty() {
            for iter in self.waiters.iter() {
                unsafe { (*(*iter)).thread.unpark(); }
            }
        }
    }
}

/// An object to manage waiters and blocking.
pub(crate) struct Waker {
    guard: Spinlock<Metadata>,
    empty: AtomicBool,
}

impl Waker {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            guard: Spinlock::new(Metadata::new()),
            empty: AtomicBool::new(true),
        }
    }

    /// Registers a waiter.
    ///
    /// Before real registering, it will recheck condition (bucket is ready or queue is closed).
    /// This thing will help to avoid lost wakeup.
    ///
    /// Returns `False` if bucket is ready or queue is closed.
    /// Returns `True` if bucket is not ready and the queue is still opened.
    #[inline]
    pub(crate) fn register(&self, waiter: &Waiter) -> bool {
        let mut inner = self.guard.lock();
        if inner.closed {
            return false;
        }
        unsafe {
            if (*waiter.bucket_lap).load(Ordering::Acquire) != waiter.capture_lap {
                return false;
            }
        }
        inner.register(waiter);
        self.empty.store(false, Ordering::SeqCst);
        true
    }

    /// Unregisters a waiter.
    ///
    /// It should be used by the blocking thread to avoid lost wakeup.
    #[inline]
    pub(crate) fn unregister(&self, waiter: &Waiter) {
        if !self.empty.load(Ordering::SeqCst) {
            let mut inner = self.guard.lock();
            if inner.unregister(waiter) {
                self.empty.store(inner.waiters.is_empty(), Ordering::SeqCst);
            }
        }
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    pub(crate) fn notify(&self) {
        if !self.empty.load(Ordering::SeqCst) {
            self.guard.lock().notify();
        }
    }

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    #[inline]
    pub(crate) fn close(&self) {
        self.guard.lock().close();
        self.empty.store(true, Ordering::SeqCst);
    }
}