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

use std::sync::atomic::{AtomicU32, Ordering};

use omango_util::hint::likely;

/// A [`WaitGroup`] waits for a collection of threads to finish.
/// The main thread calls [`add`] to set the number of threads to wait for.
/// Then each of the threads runs and calls [`done`] when finished. 
/// At the same time, [`wait`] can be used to block until all threads 
/// have finished.
///
/// A [`WaitGroup`] must not be copied after first use.
///
/// A call to [`done`] “synchronizes before” the return of any 
/// Wait call that it unblocks.
///
/// [`add`]: WaitGroup::add
/// [`done`]: WaitGroup::done
/// [`wait`]: WaitGroup::wait
pub struct WaitGroup {
    count: AtomicU32,
    flag: AtomicU32,
}

impl Default for WaitGroup {
    #[inline(always)]
    fn default() -> Self {
        Self::new(0)
    }
}

impl WaitGroup {
    /// Creates a new [`WaitGroup`] with number member of a group.
    ///
    /// [`WaitGroup`]: WaitGroup
    #[inline(always)]
    pub fn new(n: u32) -> Self {
        Self {
            count: AtomicU32::new(n),
            flag: AtomicU32::new(0),
        }
    }

    /// Adds delta, which may be negative, to the [`WaitGroup`] counter.
    /// If the counter becomes zero, all threads blocked on [`wait`] are released.
    /// If the counter goes negative, [`add`] panics.
    ///
    /// Note that calls with a positive delta that occur when the counter is zero
    /// must happen before a [`wait`]. Calls with a negative delta, or calls with a
    /// positive delta that start when the counter is greater than zero, may happen
    /// at any time.
    /// Typically, this means the calls to [`add`] should execute before the statement
    /// creating the thread or other event to be waited for.
    /// 
    /// If a [`WaitGroup`] is reused to wait for several independent sets of events,
    /// new [`add`] calls must happen after all previous [`wait`] calls have returned.
    ///
    /// Example:
    ///
    /// ```
    /// use omango::wg::WaitGroup;
    ///
    /// let wg = std::sync::Arc::new(WaitGroup::new(1));
    /// let wg_clone = wg.clone();
    ///
    /// let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    /// let count_clone = count.clone();
    ///
    /// let thread = std::thread::spawn(move || {
    ///     wg_clone.add(1);
    ///     count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ///     wg_clone.done();
    ///     wg_clone.wait();
    ///
    ///     assert_eq!(count_clone.load(std::sync::atomic::Ordering::Relaxed), 2);
    /// });
    ///
    /// count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    /// wg.done();
    /// wg.wait();
    ///         
    /// thread.join().unwrap();
    /// assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
    /// ```
    ///
    /// [`WaitGroup`]: WaitGroup
    /// [`wait`]: WaitGroup::wait
    #[inline(always)]
    pub fn add(&self, n: u32) {
       self.count.fetch_add(n, Ordering::SeqCst);
    }

    /// Decrements the [`WaitGroup`] counter by one.
    ///
    /// Example see [`add`]
    ///
    /// [`WaitGroup`]: WaitGroup
    /// [`add`]: WaitGroup::add
    #[inline(always)]
    pub fn done(&self) {
        let count = self.count.fetch_sub(1, Ordering::SeqCst);
        assert!(count >= 1);
        
        if likely(count > 1) {
            return;
        }
        self.flag.store(1, Ordering::Relaxed);
        omango_futex::wake_all(&self.flag);
    }

    /// Blocks until the [`WaitGroup`] counter is zero.
    ///
    /// Example see [`add`]
    ///
    /// [`WaitGroup`]: WaitGroup
    /// [`add`]: WaitGroup::add
    pub fn wait(&self) {
        while self.should_wait() {
            omango_futex::wait(&self.flag, 0);
        }
        self.flag.store(0, Ordering::Relaxed);
    }

    #[inline(always)]
    fn should_wait(&self) -> bool {
        self.count.load(Ordering::SeqCst) > 0
    }
}

mod test {
    #[test]
    fn test_wait_on_one() {
        let wg = std::sync::Arc::new(crate::wg::WaitGroup::new(1));
        let wg_clone = wg.clone();

        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count_clone = count.clone();

        let thread = std::thread::spawn(move || {
            wg_clone.add(1);
            count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            wg_clone.done();
        });

        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        wg.done();
        wg.wait();
        thread.join().unwrap();

        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn test_wait_on_gt_one() {
        let wg = std::sync::Arc::new(crate::wg::WaitGroup::new(1));
        let wg_clone = wg.clone();

        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count_clone = count.clone();

        let thread = std::thread::spawn(move || {
            wg_clone.add(1);
            count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            wg_clone.done();
            wg_clone.wait();

            assert_eq!(count_clone.load(std::sync::atomic::Ordering::Relaxed), 2);
        });

        count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        wg.done();
        wg.wait();

        thread.join().unwrap();
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn test_done_without_size() {
        let result = std::panic::catch_unwind(|| {
            let wg = crate::wg::WaitGroup::default();
            wg.done();
        });
        assert!(result.is_err());
    }
}