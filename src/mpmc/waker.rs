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

use std::cell::Cell;
use std::collections::vec_deque::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::Thread;

use crate::backoff::Backoff;
use crate::mpmc::spinlock::Spinlock;

/// An object to implement blocking.
pub(crate) struct Waiter {
    is_moved: Cell<bool>,
    thread: Thread,
}

impl Waiter {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            is_moved: Cell::new(false),
            thread: thread::current(),
        }
    }

    /// Blocks the current thread.
    ///
    /// Before real blocking, it will spin and retry N times to check condition.
    /// This thing significantly improves performance.
    pub(crate) fn sleep<F>(&self, out_condition: F) where F: Fn() -> bool {
        // CHEAT:
        // In case multiple threads are in high contention, `out_condition' will be activated soon.
        // So I will retry N times before blocking the current thread.
        let backoff = Backoff::default();
        loop {
            if out_condition() {
                return;
            }
            if backoff.snooze_completed() {
                break;
            }
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
    selectors: Vec<*const Waiter>,
}

impl Metadata {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
            selectors: Vec::new(),
        }
    }

    /// Registers a waiter.
    #[inline]
    pub(crate) fn register(&mut self, waiter: &Waiter) {
        self.waiters.push_back(waiter as *const Waiter);
    }

    /// Unregisters a waiter.
    #[inline]
    pub(crate) fn unregister(&mut self, waiter: &Waiter) -> bool {
        if waiter.is_moved.get() {
            if !self.selectors.is_empty() {
                if let Some((i, _)) =
                self
                    .selectors
                    .iter()
                    .enumerate()
                    .find(|&(_, item)| (*item) == (waiter as *const Waiter))
                {
                    self.selectors.remove(i);
                    return true;
                }
            }
        } else if !self.waiters.is_empty() {
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
    pub(crate) fn notify(&mut self) {
        if !self.waiters.is_empty() {
            let item = self.waiters.pop_front().unwrap();
            unsafe { (*item).is_moved.set(true); }
            self.selectors.push(item);
        }
        if !self.selectors.is_empty() {
            for item in self.selectors.iter() {
                unsafe { (*(*item)).wake(); }
            }
        }
    }

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    #[inline]
    pub(crate) fn notify_all(&mut self) {
        if !self.waiters.is_empty() {
            for iter in self.waiters.iter() {
                unsafe { (*(*iter)).wake(); }
            }
        }
        if !self.selectors.is_empty() {
            for iter in self.selectors.iter() {
                unsafe { (*(*iter)).wake(); }
            }
        }
    }
}

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
        // Rechecks to avoid wakeup before registering.
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
                self.is_empty.store(inner.waiters.is_empty() && inner.selectors.is_empty(),
                                    Ordering::SeqCst);
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

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    #[inline]
    pub(crate) fn notify_all(&self) {
        if !self.is_empty.load(Ordering::SeqCst) {
            self.guard.lock().notify_all();
            self.is_empty.store(true, Ordering::SeqCst);
        }
    }
}