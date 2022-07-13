use std::collections::vec_deque::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::thread::Thread;

use likely_stable::{likely, unlikely};
use parking_lot::Mutex;

use crate::backoff::Backoff;
use crate::common::{CLOSED, FAILED, SUCCESS};

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
        let backoff = Backoff::default();
        let bucket_lap = unsafe { &(*self.bucket_lap) };
        for _ in 0..n_retry {
            loop {
                if unlikely(closed.load(Ordering::Relaxed)) {
                    return CLOSED;
                }
                if bucket_lap.load(Ordering::Acquire) != self.capture_lap {
                    return SUCCESS;
                }
                if backoff.snooze_completed() {
                    break;
                }
            }
            backoff.reset();
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
    start: usize,
    closed: bool,
}

impl Metadata {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            waiters: VecDeque::new(),
            start: 0,
            closed: false,
        }
    }

    /// Registers a waiter.
    #[inline]
    pub(crate) fn register(&mut self, waiter: &Waiter) {
        self.waiters.push_back(waiter as *const Waiter);
    }

    /// Unregisters a waiter.
    #[inline]
    pub(crate) fn unregister(&mut self, waiter: &Waiter) {
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
            return;
        }
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    pub(crate) fn notify(&mut self) {
        if likely(self.start + 1 <= self.waiters.len()) {
            self.start += 1;
        }
        // Prevents to lost wakeup.
        for idx in 0..self.start {
            let waiter = self.waiters.get(idx).unwrap();
            unsafe {
                if (*(*waiter)).parked.load(Ordering::Acquire) {
                    (*(*waiter)).thread.unpark();
                }
            }
        }
    }

    /// Wakes up all waiters from the queue.
    ///
    /// It doesn't remove waiters.
    #[inline]
    pub(crate) fn close(&mut self) {
        self.closed = true;
        if !self.waiters.is_empty() {
            for iter in self.waiters.iter() {
                unsafe { (*(*iter)).thread.unpark(); }
            }
        }
    }
}

pub(crate) struct Waker {
    guard: Mutex<Metadata>,
    empty: AtomicBool,
}

impl Waker {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            guard: Mutex::new(Metadata::new()),
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
        let mut inner = self.guard.lock();
        inner.unregister(waiter);
        self.empty.store(inner.waiters.is_empty(), Ordering::SeqCst);
    }

    /// Wakes up one waiter from the queue.
    ///
    /// It doesn't remove waiter.
    #[inline]
    pub(crate) fn notify(&self) {
        if unlikely(!self.empty.load(Ordering::SeqCst)) {
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