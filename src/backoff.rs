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

//! This file was modified based on the following program in Crossbeam-Utils.
//!
//! Source:
//!   - `<https://github.com/crossbeam-rs/crossbeam/blob/master/crossbeam-utils/src/backoff.rs>`
//!
//! Copyright & License:
//!   - The Crossbeam Project Developers
//!   - The MIT License (MIT) or Apache License 2.0
//!   - `<https://opensource.org/licenses/MIT>`
//!   - '<https://www.apache.org/licenses/LICENSE-2.0>'

use core::cell::Cell;
use std::thread;

use likely_stable::likely;

const SPIN_LIMIT: u32 = 6;
const YIELD_LIMIT: u32 = 10;

/// Makes the current thread to wait in the short time.
///
/// It should be used to implement wait-retry in high contention multithreading environment
/// because of improving performance significantly.
///
/// It should not be used to replace other blocking mechanism.
pub(crate) struct Backoff {
    step: Cell<u32>,
}

impl Backoff {
    /// Create an Backoff object.
    #[inline]
    pub(crate) fn new() -> Self {
        Backoff { step: Cell::new(0) }
    }

    #[inline]
    pub(crate) fn reset(&self) {
        self.step.set(0);
    }

    /// Backs off in a lock-free loop.
    ///
    /// This method should be used when we need to retry an operation because another thread made
    /// progress.
    ///
    /// The processor may yield using the *YIELD* or *PAUSE* instruction.
    #[inline]
    pub(crate) fn spin(&self) {
        for _ in 0..1 << self.step.get().min(SPIN_LIMIT) {
            core::hint::spin_loop();
        }
        if self.step.get() <= SPIN_LIMIT {
            self.step.set(self.step.get() + 1);
        }
    }

    /// Backs off in a blocking loop.
    ///
    /// This method should be used when we need to wait for another thread to make progress.
    ///
    /// The processor may yield using the *YIELD* or *PAUSE* instruction and the current thread
    /// may yield by giving up a time slice to the OS scheduler.
    #[inline]
    pub(crate) fn snooze(&self) {
        if self.step.get() <= SPIN_LIMIT {
            for _ in 0..1 << self.step.get() {
                core::hint::spin_loop();
            }
            self.step.set(self.step.get() + 1);
        } else {
            thread::yield_now();
        }
    }

    /// Backs off in a blocking loop.
    ///
    /// This method should be used when we need to wait for another thread to make progress.
    ///
    /// The processor may yield using the *YIELD* or *PAUSE* instruction and the current thread
    /// may yield by giving up a time slice to the OS scheduler.
    ///
    /// Return `true` to advise to stop using backoff and
    /// block the current thread using a different synchronization mechanism instead.
    #[inline]
    pub(crate) fn snooze_completed(&self) -> bool {
        if likely(self.step.get() <= SPIN_LIMIT) {
            for _ in 0..1 << self.step.get() {
                core::hint::spin_loop();
            }
        } else {
            thread::yield_now();
        }

        if likely(self.step.get() <= YIELD_LIMIT) {
            self.step.set(self.step.get() + 1);
            return false;
        }
        true
    }
}

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff::new()
    }
}