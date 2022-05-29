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
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

#[macro_export]
macro_rules! block {
    ($is_closed:expr, $waker:expr, $selector:expr) => {
        let waiter = Waiter::new();
        let out_condition = || {
            if unlikely($is_closed.load(Ordering::Relaxed)) {
                return true;
            }
            unsafe {
                let bucket = &*($selector.ptr as *const Bucket<T>);
                if bucket.lap.load(Ordering::Acquire) != $selector.lap {
                    return true;
                }
                false
            }
        };
        if $waker.register(&waiter, out_condition) {
            waiter.sleep(out_condition);
            $waker.unregister(&waiter);
        }
    };
}

pub(crate) const N_SPIN: u8 = 128;

pub(crate) enum Operation {
    Sending,
    Receiving,
}

/// It is an element in bounded queue.
pub(crate) struct Bucket<T> {
    pub(crate) lap: AtomicU32,
    pub(crate) msg: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Default for Bucket<T> {
    fn default() -> Self {
        Self {
            lap: AtomicU32::new(0),
            msg: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

/// Captures a lap and a reference to the selected bucket for writing or reading.
pub(crate) struct Selector {
    pub(crate) ptr: *const u8,
    pub(crate) lap: u32,
}

impl Default for Selector {
    fn default() -> Self {
        Self {
            ptr: ptr::null(),
            lap: 0,
        }
    }
}

impl Selector {
    /// Writes message into bucket.
    ///
    /// Also wakes up one blocking receiver if having.
    #[inline]
    pub(crate) fn write_message<T>(&self, msg: T) {
        unsafe {
            let bucket = &*(self.ptr as *const Bucket<T>);

            // We own the element, do non-atomic write.
            bucket.msg.get().write(MaybeUninit::new(msg));

            // Make the element available for reading.
            bucket.lap.store(self.lap + 1, Ordering::Release);
        }
    }

    /// Reads and removes message from bucket.
    ///
    /// Also wakes up one blocking sender if having.
    #[inline]
    pub(crate) fn read_message<T>(&self) -> T {
        unsafe {
            let bucket = &*(self.ptr as *const Bucket<T>);

            // We own the element, do non-atomic read and remove.
            let msg = bucket.msg.get().read().assume_init();

            // Make the element available for writing.
            bucket.lap.store(self.lap + 1, Ordering::Release);
            msg
        }
    }
}