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
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::Thread;

use crate::common::N_SPIN;
use crate::spsc::spinlock::Spinlock;

/// An object to implement blocking.
pub(crate) struct Waiter {
    thread: Thread,
}

impl Waiter {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            thread: thread::current(),
        }
    }

    /// Blocks the current thread.
    ///
    /// Before real blocking, it will spin and retry N times to check condition.
    /// This thing significantly improves performance.
    pub(crate) fn sleep<F>(&self, out_condition: F) where F: Fn() -> bool {
        // CHEAT:
        // `out_condition' will be activated soon in multithreading environment.
        // So I will retry N times before blocking the current thread.
        for _ in 0..N_SPIN {
            if out_condition() {
                return;
            }
            core::hint::spin_loop();
        }
        while !out_condition() {
            thread::park();
        }
    }

    /// Unblocks the current thread.
    #[inline]
    fn wake(&self) {
        self.thread.unpark();
    }
}

struct Metadata {
    waiters: VecDeque<*const Waiter>,
}

impl Metadata {
    #[inline]
    fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
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
    fn notify(&mut self) {
        if !self.waiters.is_empty() {
            unsafe { (*(*self.waiters.get(0).unwrap())).wake(); }
        }
    }
}

/// An object to manage waiters and blocking.
pub(crate) struct Waker {
    guard: Spinlock<Metadata>,
    is_empty: AtomicBool,
}

impl Waker {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            guard: Spinlock::new(Metadata::new()),
            is_empty: AtomicBool::new(true),
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
    pub(crate) fn register<F>(&self, waiter: &Waiter, out_condition: F) -> bool where F: Fn() -> bool {
        let mut inner = self.guard.lock();
        if out_condition() {
            return false;
        }
        inner.register(waiter);
        self.is_empty.store(false, Ordering::SeqCst);
        true
    }

    /// Unregisters a waiter.
    ///
    /// It should be used by the blocking thread to avoid lost wakeup.
    #[inline]
    pub(crate) fn unregister(&self, waiter: &Waiter) {
        if !self.is_empty.load(Ordering::SeqCst) {
            let mut inner = self.guard.lock();
            if inner.unregister(waiter) {
                self.is_empty.store(inner.waiters.is_empty(), Ordering::SeqCst);
            }
        }
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    pub(crate) fn notify(&self) {
        if !self.is_empty.load(Ordering::SeqCst) {
            self.guard.lock().notify();
        }
    }
}