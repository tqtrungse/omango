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

use std::{
    collections::vec_deque::VecDeque,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use parking_lot::Mutex;
use omango_futex::{wait, wake_one};
use omango_util::{
    backoff::Backoff,
    hint::{likely, unlikely},
};

use crate::queue::state::State;

const PARKED: u32 = 1;
const UN_PARKED: u32 = 2;

pub(crate) trait Checker {
    fn is_close(&self) -> bool;
}

/// An object to implement blocking.
pub(crate) struct Waiter {
    atom: *const AtomicU32,
    expected: u32,
    parked: AtomicU32,
}

impl Waiter {
    #[inline]
    pub(crate) fn new(e_lap: *const AtomicU32, lap: u32) -> Self {
        Self {
            atom: e_lap,
            expected: lap,
            parked: AtomicU32::new(UN_PARKED),
        }
    }

    /// Retries and snooze the current thread.
    ///
    /// Before real blocking, it will spin and retry N times to check condition.
    /// This thing significantly improves performance.
    pub(crate) fn retry(&self, checker: &dyn Checker, n_retry: u8) -> State  {
        let backoff = Backoff::default();
        let atom = unsafe { &(*self.atom) };
        for _ in 0..n_retry {
            loop {
                if unlikely(checker.is_close()) {
                    return State::Closed;
                }
                if atom.load(Ordering::Acquire) != self.expected {
                    return State::Success;
                }
                if backoff.snooze_completed() {
                    break;
                }
            }
            backoff.reset();
        }
        State::Failed
    }

    /// Blocks the current thread until woken.
    pub(crate) fn sleep(&self, checker: &dyn Checker) -> State  {
        let atom = unsafe { &(*self.atom) };
        loop {
            if unlikely(checker.is_close()) {
                self.parked.store(UN_PARKED, Ordering::Release);
                return State::Closed;
            }
            if atom.load(Ordering::Acquire) != self.expected {
                self.parked.store(UN_PARKED, Ordering::Release);
                return State::Success;
            }
            self.parked.store(PARKED, Ordering::Relaxed);
            wait(&self.parked, PARKED);
        }
    }
}

struct Metadata {
    waiters: VecDeque<*const Waiter>,
    start: usize,
    closed: bool,
}

impl Metadata {
    #[inline]
    fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
            start: 0,
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
    fn unregister(&mut self, waiter: &Waiter) {
        if let Some((i, _)) =
            self
                .waiters
                .iter()
                .enumerate()
                .find(|&(_, item)| (*item) == (waiter as *const Waiter))
        {
            self.waiters.remove(i);
            if self.start > 0 {
                self.start -= 1;
            }
        }
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    fn notify(&mut self) {
        if likely(self.start < self.waiters.len()) {
            self.start += 1;
        }
        // Prevents to lost wakeup.
        for idx in 0..self.start {
            let waiter = self.waiters.get(idx).unwrap();
            unsafe {
                if (*(*waiter)).parked.load(Ordering::Acquire) == PARKED {
                    wake_one(&(*(*waiter)).parked);
                }
            }
        }
    }

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    #[inline]
    fn close(&mut self) {
        self.closed = true;
        if !self.waiters.is_empty() {
            for iter in self.waiters.iter() {
                unsafe { wake_one(&(*(*iter)).parked); }
            }
        }
    }
    
    /// Check waiters is empty.
    #[inline]
    fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }
}

pub(crate) struct Waker {
    guard: Mutex<Metadata>,
    empty: AtomicBool,
}

impl Default for Waker {
    #[inline]
    fn default() -> Self {
        Self {
            guard: Mutex::new(Metadata::new()),
            empty: AtomicBool::new(true),
        }
    }
}

impl Waker {
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
            if (*waiter.atom).load(Ordering::Acquire) != waiter.expected {
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
        let mut inner = self.guard.lock();
        inner.unregister(waiter);
        self.empty.store(inner.waiters.is_empty(), Ordering::SeqCst);
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    pub(crate) fn wake(&self) {
        if unlikely(!self.empty.load(Ordering::SeqCst)) {
            self.guard.lock().notify();
        }
    }

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    pub(crate) fn close(&self) {
        loop {
            let mut inner = self.guard.lock();
            if !inner.is_empty() {
                inner.close();
            } else {
                self.empty.store(true, Ordering::SeqCst);
                return;
            }
        }
    }
}