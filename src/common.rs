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
use std::sync::atomic::AtomicU32;

#[macro_export]
macro_rules! write_message {
    ($bucket:expr, $lap:expr, $msg:expr, $recv_waker:expr) => {
        unsafe { $bucket.msg.get().write(MaybeUninit::new($msg)); }
        // Make the element available for reading.
        $bucket.lap.store($lap.wrapping_add(1), Ordering::Release);
        $recv_waker.notify();
    };
}

#[macro_export]
macro_rules! read_message {
    ($bucket:expr, $lap:expr, $msg:expr, $send_waker:expr) => {
        unsafe {
            // We own the element, do non-atomic read and remove.
            $msg = $bucket.msg.get().read().assume_init();
        };
        // Make the element available for writing.
        $bucket.lap.store($lap.wrapping_add(1), Ordering::Release);
        $send_waker.notify();
    };
}

#[macro_export]
macro_rules! block_for_send {
    ($waiter:expr, $send_waker:expr, $closed:expr, $msg:expr) => {
        if $send_waker.register($waiter) {
            let state = $waiter.sleep($closed);
            $send_waker.unregister($waiter);
            if unlikely(state == CLOSED) {
                return Err(SendError($msg));
            }
        }
    };
}

#[macro_export]
macro_rules! block_for_recv {
    ($waiter:expr, $recv_waker:expr, $closed:expr) => {
        if $recv_waker.register($waiter) {
            let state = $waiter.sleep($closed);
            $recv_waker.unregister($waiter);
            if unlikely(state == CLOSED) {
                return Err(RecvError);
            }
        }
    };
}

pub(crate) const N_SPIN: u8 = 128;
pub(crate) const N_RETRY_MPMC: u8 = 2;
pub(crate) const N_RETRY_SPSC: u8 = 1;
pub(crate) const SUCCESS: u8 = 0;
pub(crate) const FAILED: u8 = 1;
pub(crate) const CLOSED: u8 = 2;

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