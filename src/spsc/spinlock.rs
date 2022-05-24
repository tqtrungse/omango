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

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

/// A spin lock version for low contention multithreading environment.
pub(crate) struct Spinlock<T> {
    flag: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for Spinlock<T> {}

unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    #[inline]
    pub(crate) fn new(value: T) -> Self {
        Self {
            flag: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    #[inline]
    pub(crate) fn lock(&self) -> SpinlockGuard<T> {
        // "compare_exchange" performance is better than "swap".
        // The reason for using a weak "compare_exchange" is explained here:
        // https://github.com/Amanieu/parking_lot/pull/207#issuecomment-575869107
        while self.flag.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            // Waits the lock is unlocked to reduce CPU cache coherence.
            while self.flag.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
        SpinlockGuard { parent: self }
    }
}

pub(crate) struct SpinlockGuard<'a, T> {
    parent: &'a Spinlock<T>,
}

impl<T> Drop for SpinlockGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.parent.flag.store(false, Ordering::Release);
    }
}

impl<T> Deref for SpinlockGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.parent.value.get() }
    }
}

impl<T> DerefMut for SpinlockGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.parent.value.get() }
    }
}