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

/// It is an element in bounded queue.
pub(crate) struct Bucket<T> {
    lap: AtomicU32,
    msg: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Default for Bucket<T> {
    fn default() -> Self {
        Self {
            lap: AtomicU32::new(0),
            msg: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

impl<T> Bucket<T> {
    #[inline]
    pub(crate) fn get_lap(&self) -> u32 {
        self.lap.load(Ordering::Acquire)
    }
}

/// Captures a lap and a reference to the selected bucket for writing or reading.
pub(crate) struct Selector<T> {
    ptr: *const Bucket<T>,
    lap: u32,
}

impl<T> Default for Selector<T> {
    fn default() -> Self {
        Self {
            ptr: ptr::null(),
            lap: 0,
        }
    }
}

impl<T> Selector<T> {
    /// Stores reference to selected bucket and lap.
    #[inline]
    pub(crate) fn set(&mut self, bucket: *const Bucket<T>, lap: u32) {
        self.ptr = bucket;
        self.lap = lap;
    }

    /// Writes message to bucket.
    #[inline]
    pub(crate) fn write_message(&self, msg: T) {
        unsafe {
            let bucket = &*(self.ptr);

            // We own the element, do non-atomic write.
            bucket.msg.get().write(MaybeUninit::new(msg));

            // Make the element available for reading.
            bucket.lap.store(self.lap + 1, Ordering::Release);
        }
    }

    /// Reads and removes message from bucket.
    #[inline]
    pub(crate) fn read_message(&self) -> T {
        unsafe {
            let bucket = &*(self.ptr);

            // We own the element, do non-atomic read and remove.
            let msg = bucket.msg.get().read().assume_init();

            // Make the element available for writing.
            bucket.lap.store(self.lap + 1, Ordering::Release);
            msg
        }
    }

    /// Checks the bucket is ready for writing or reading.
    #[inline]
    pub(crate) fn is_ready(&self) -> bool {
        unsafe {
            let bucket = &*(self.ptr);
            if bucket.lap.load(Ordering::Acquire) != self.lap {
                return true;
            }
            false
        }
    }
}